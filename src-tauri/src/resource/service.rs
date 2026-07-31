use std::{
    io::Cursor,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration as StdDuration, SystemTime},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{Duration as ChronoDuration, Utc};
use image::{ImageFormat, ImageReader, Limits};
use uuid::Uuid;

use crate::{
    document_runtime::{
        DocumentRequest, DocumentRuntime, DocumentRuntimeError, ProcessDocumentRuntime,
        is_supported_media_type,
    },
    domain::resource::{ImportResourcesInput, ResourceSummary, ResourceThumbnail},
    engine::{EngineDocument, EngineImage},
    error::AppError,
    resource::{
        blob_store::{BlobInput, BlobStore},
        context::{MAX_DOCUMENTS_PER_RUN, MAX_TOTAL_DOCUMENT_CHARS_PER_RUN, bounded_document},
        local_blob_store::LocalBlobStore,
        repository::{DocumentDerivativeCompletion, ResourceRepository},
    },
};

pub const LOCAL_PERSONAL_SPACE_ID: &str = "local-personal";
pub const LOCAL_BLOB_STORE_ID: &str = "local-default";
pub const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;
pub const MAX_DOCUMENT_BYTES: u64 = 50 * 1024 * 1024;
pub const MAX_IMAGE_DIMENSION: u32 = 16_384;
pub const MAX_DECODE_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_IMAGES_PER_RUN: usize = 8;
pub const MAX_RUN_IMAGE_BYTES: u64 = 24 * 1024 * 1024;
pub const THUMBNAIL_EDGE: u32 = 192;
pub const STAGING_GRACE_HOURS: i64 = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineAttachments {
    pub images: Vec<EngineImage>,
    pub documents: Vec<EngineDocument>,
}

#[derive(Clone)]
pub struct ResourceService {
    repository: ResourceRepository,
    blob_store: Arc<LocalBlobStore>,
    cache_root: PathBuf,
    document_runtime: Arc<dyn DocumentRuntime>,
}

impl ResourceService {
    pub fn new(
        repository: ResourceRepository,
        blob_store: Arc<LocalBlobStore>,
        cache_root: PathBuf,
    ) -> Self {
        let document_root = cache_root.join("documents");
        Self {
            repository,
            blob_store,
            cache_root,
            document_runtime: Arc::new(ProcessDocumentRuntime::new(document_root)),
        }
    }

    pub fn with_document_runtime(mut self, runtime: Arc<dyn DocumentRuntime>) -> Self {
        self.document_runtime = runtime;
        self
    }

    pub async fn import_resources(
        &self,
        input: ImportResourcesInput,
    ) -> Result<Vec<ResourceSummary>, AppError> {
        let target_count =
            usize::from(input.draft_id.is_some()) + usize::from(input.work_id.is_some());
        if target_count != 1 {
            return Err(AppError::invalid_input(
                "resourceTarget",
                "exactly one Work or draft target is required",
            ));
        }
        if input.source_paths.is_empty() {
            return Err(AppError::invalid_input(
                "sourcePaths",
                "at least one attachment is required",
            ));
        }

        let mut imported = Vec::with_capacity(input.source_paths.len());
        for source_path in input.source_paths {
            imported.push(
                self.import_one(
                    PathBuf::from(source_path),
                    input.draft_id.as_deref(),
                    input.work_id.as_deref(),
                )
                .await?,
            );
        }
        Ok(imported)
    }

    async fn import_one(
        &self,
        source_path: PathBuf,
        draft_id: Option<&str>,
        work_id: Option<&str>,
    ) -> Result<ResourceSummary, AppError> {
        let resource_id = Uuid::new_v4().to_string();
        let original_name = source_path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("attachment")
            .to_owned();
        let metadata = std::fs::symlink_metadata(&source_path).ok();
        let observed_size = metadata.as_ref().map_or(0, std::fs::Metadata::len);
        let document_media_type_hint = document_media_type_from_extension(&source_path);
        self.repository
            .create_staging(
                &resource_id,
                &original_name,
                observed_size,
                draft_id,
                work_id,
            )
            .await?;

        if metadata
            .as_ref()
            .is_none_or(|metadata| metadata.file_type().is_symlink() || !metadata.is_file())
        {
            return self
                .repository
                .fail_import(
                    &resource_id,
                    if document_media_type_hint.is_some() {
                        "unsupported_document"
                    } else {
                        "unsupported_image"
                    },
                )
                .await;
        }
        let document_media_type = detect_document_media_type(&source_path);
        let limit = if document_media_type_hint.is_some() {
            MAX_DOCUMENT_BYTES
        } else {
            MAX_IMAGE_BYTES
        };
        if observed_size > limit {
            return self
                .repository
                .fail_import(
                    &resource_id,
                    if document_media_type_hint.is_some() {
                        "document_too_large"
                    } else {
                        "image_too_large"
                    },
                )
                .await;
        }

        let stored = match self
            .blob_store
            .put(BlobInput {
                source_path: source_path.clone(),
            })
            .await
        {
            Ok(stored) => stored,
            Err(_) => {
                return self
                    .repository
                    .fail_import(
                        &resource_id,
                        if document_media_type_hint.is_some() {
                            "document_parse_failed"
                        } else {
                            "image_decode_failed"
                        },
                    )
                    .await;
            }
        };
        if stored.size > limit {
            return self
                .repository
                .fail_import(
                    &resource_id,
                    if document_media_type_hint.is_some() {
                        "document_too_large"
                    } else {
                        "image_too_large"
                    },
                )
                .await;
        }
        if document_media_type.is_none()
            && let Some(media_type) = document_media_type_hint
        {
            self.repository
                .commit_blob_for_processing(
                    &resource_id,
                    media_type,
                    &stored.sha256,
                    stored.size,
                    &stored.object_key,
                )
                .await?;
            return self
                .repository
                .fail_document_import(&resource_id, "unsupported_document")
                .await;
        }
        if let Some(media_type) = document_media_type {
            self.repository
                .commit_blob_for_processing(
                    &resource_id,
                    media_type,
                    &stored.sha256,
                    stored.size,
                    &stored.object_key,
                )
                .await?;
            let cache_key = format!("documents/{resource_id}.md");
            let output_path = self.cache_root.join(&cache_key);
            let source_path = self
                .blob_store
                .path_for(&stored.reference)
                .map_err(|_| AppError::ResourceStorage)?;
            return match self
                .document_runtime
                .extract(DocumentRequest {
                    source_path,
                    media_type: media_type.into(),
                    output_path,
                })
                .await
            {
                Ok(result) => {
                    self.repository
                        .complete_document_import(
                            &resource_id,
                            DocumentDerivativeCompletion {
                                cache_key: &cache_key,
                                result: &result,
                            },
                        )
                        .await
                }
                Err(error) => {
                    self.repository
                        .fail_document_import(&resource_id, document_failure_code(&error))
                        .await
                }
            };
        }
        let bytes = match self.blob_store.read(&stored.reference).await {
            Ok(bytes) => bytes,
            Err(_) => {
                return self
                    .repository
                    .fail_import(&resource_id, "image_decode_failed")
                    .await;
            }
        };
        let (media_type, decoded) = match decode_image(bytes).await {
            Ok(decoded) => decoded,
            Err(code) => return self.repository.fail_import(&resource_id, code).await,
        };
        if write_thumbnail(&self.cache_root, &resource_id, decoded)
            .await
            .is_err()
        {
            return self
                .repository
                .fail_import(&resource_id, "thumbnail_failed")
                .await;
        }
        self.repository
            .complete_import(
                &resource_id,
                media_type,
                &stored.sha256,
                stored.size,
                &stored.object_key,
            )
            .await
    }

    pub async fn list_work_resources(
        &self,
        work_id: &str,
    ) -> Result<Vec<ResourceSummary>, AppError> {
        self.repository.list_for_work(work_id).await
    }

    pub async fn thumbnail(&self, resource_id: &str) -> Result<ResourceThumbnail, AppError> {
        self.repository
            .find(resource_id)
            .await?
            .ok_or_else(|| AppError::resource_not_found(resource_id))?;
        let path = self.thumbnail_path(resource_id);
        let data = tokio::task::spawn_blocking(move || std::fs::read(path))
            .await
            .map_err(|_| AppError::ResourceStorage)?
            .map_err(|_| AppError::ResourceStorage)?;
        Ok(ResourceThumbnail {
            media_type: "image/png".into(),
            data_base64: STANDARD.encode(data),
        })
    }

    pub async fn engine_images(
        &self,
        work_id: &str,
        resource_ids: &[String],
    ) -> Result<Vec<EngineImage>, AppError> {
        if resource_ids.len() > MAX_IMAGES_PER_RUN {
            return Err(AppError::resource_import("too_many_images"));
        }
        let resources = self
            .repository
            .engine_resources(work_id, resource_ids)
            .await?;
        if resources.len() != resource_ids.len() {
            return Err(AppError::resource_import("resource_not_ready_or_unlinked"));
        }
        let by_id = resources
            .into_iter()
            .map(|resource| (resource.id.clone(), resource))
            .collect::<std::collections::HashMap<_, _>>();
        let mut total = 0_u64;
        let mut images = Vec::with_capacity(resource_ids.len());
        for resource_id in resource_ids {
            let resource = by_id
                .get(resource_id)
                .ok_or_else(|| AppError::resource_import("resource_not_ready_or_unlinked"))?;
            total = total
                .checked_add(resource.size)
                .ok_or_else(|| AppError::resource_import("image_budget_exceeded"))?;
            if total > MAX_RUN_IMAGE_BYTES {
                return Err(AppError::resource_import("image_budget_exceeded"));
            }
            let data = self
                .blob_store
                .read(&crate::resource::blob_store::StorageRef {
                    object_key: resource.object_key.clone(),
                })
                .await
                .map_err(|_| AppError::ResourceStorage)?;
            if u64::try_from(data.len()).ok() != Some(resource.size) {
                return Err(AppError::ResourceStorage);
            }
            images.push(EngineImage {
                media_type: resource.media_type.clone(),
                data,
            });
        }
        Ok(images)
    }

    pub async fn engine_attachments(
        &self,
        work_id: &str,
        resource_ids: &[String],
    ) -> Result<EngineAttachments, AppError> {
        let resources = self
            .repository
            .engine_resources(work_id, resource_ids)
            .await?;
        if resources.len() != resource_ids.len() {
            return Err(AppError::resource_import("resource_not_ready_or_unlinked"));
        }
        let derivatives = self
            .repository
            .document_derivatives_for_engine(work_id, resource_ids)
            .await?;
        let resource_by_id = resources
            .into_iter()
            .map(|resource| (resource.id.clone(), resource))
            .collect::<std::collections::HashMap<_, _>>();
        let derivative_by_id = derivatives
            .into_iter()
            .map(|derivative| (derivative.resource_id.clone(), derivative))
            .collect::<std::collections::HashMap<_, _>>();

        let image_ids = resource_ids
            .iter()
            .filter(|resource_id| {
                resource_by_id
                    .get(*resource_id)
                    .is_some_and(|resource| resource.media_type.starts_with("image/"))
            })
            .cloned()
            .collect::<Vec<_>>();
        let images = self.engine_images(work_id, &image_ids).await?;
        let mut documents = Vec::new();
        let mut used_chars = 0_usize;
        for resource_id in resource_ids {
            let resource = resource_by_id
                .get(resource_id)
                .ok_or_else(|| AppError::resource_import("resource_not_ready_or_unlinked"))?;
            if resource.media_type.starts_with("image/") {
                continue;
            }
            if documents.len() == MAX_DOCUMENTS_PER_RUN {
                break;
            }
            let derivative = derivative_by_id
                .get(resource_id)
                .ok_or_else(|| AppError::resource_import("resource_not_ready_or_unlinked"))?;
            let cache_path = safe_cache_path(&self.cache_root, &derivative.cache_key)?;
            let content = tokio::fs::read_to_string(cache_path)
                .await
                .map_err(|_| AppError::resource_import("resource_not_ready_or_unlinked"))?;
            let remaining = MAX_TOTAL_DOCUMENT_CHARS_PER_RUN.saturating_sub(used_chars);
            let document = bounded_document(
                resource.original_name.clone(),
                resource.media_type.clone(),
                &content,
                remaining,
            );
            used_chars += document.content.chars().count();
            documents.push(document);
        }
        Ok(EngineAttachments { images, documents })
    }

    pub async fn detach_draft_resource(
        &self,
        draft_id: &str,
        resource_id: &str,
    ) -> Result<(), AppError> {
        self.repository
            .detach_draft_resource(draft_id, resource_id)
            .await
    }

    pub async fn recover_interrupted_imports(&self) -> Result<u64, AppError> {
        let cutoff = Utc::now() - ChronoDuration::hours(STAGING_GRACE_HOURS);
        let recovered = self.repository.recover_stale_staging(cutoff).await?;
        let abandoned = self.repository.collect_abandoned_drafts(cutoff).await?;
        for reference in abandoned {
            self.blob_store
                .delete(&reference)
                .await
                .map_err(|_| AppError::ResourceStorage)?;
        }
        let tracked = self.repository.local_replica_refs().await?;
        let cutoff_system = if cutoff.timestamp() <= 0 {
            SystemTime::UNIX_EPOCH
        } else {
            SystemTime::UNIX_EPOCH + StdDuration::from_secs(cutoff.timestamp() as u64)
        };
        for reference in self
            .blob_store
            .list_objects_older_than(cutoff_system)
            .await
            .map_err(|_| AppError::ResourceStorage)?
        {
            if !tracked.contains(&reference) {
                self.blob_store
                    .delete(&reference)
                    .await
                    .map_err(|_| AppError::ResourceStorage)?;
            }
        }
        let temporary_directories = [
            self.blob_store.staging_dir(),
            self.cache_root.join("documents"),
        ];
        tokio::task::spawn_blocking(move || {
            for directory in temporary_directories {
                if let Ok(entries) = std::fs::read_dir(directory) {
                    for entry in entries.flatten() {
                        if entry
                            .path()
                            .extension()
                            .is_some_and(|extension| extension == "part")
                        {
                            let _ = std::fs::remove_file(entry.path());
                        }
                    }
                }
            }
        })
        .await
        .map_err(|_| AppError::ResourceStorage)?;
        let mut rebuilt = 0_u64;
        for document in self.repository.recoverable_documents().await? {
            let output_path = safe_cache_path(&self.cache_root, &document.cache_key)?;
            if output_path.is_file() {
                continue;
            }
            self.repository
                .begin_document_rebuild(&document.resource_id)
                .await?;
            let source_path = self
                .blob_store
                .path_for(&crate::resource::blob_store::StorageRef {
                    object_key: document.object_key,
                })
                .map_err(|_| AppError::ResourceStorage)?;
            match self
                .document_runtime
                .extract(DocumentRequest {
                    source_path,
                    media_type: document.media_type,
                    output_path,
                })
                .await
            {
                Ok(result) => {
                    self.repository
                        .complete_document_import(
                            &document.resource_id,
                            DocumentDerivativeCompletion {
                                cache_key: &document.cache_key,
                                result: &result,
                            },
                        )
                        .await?;
                }
                Err(error) => {
                    self.repository
                        .fail_document_import(&document.resource_id, document_failure_code(&error))
                        .await?;
                }
            }
            rebuilt += 1;
        }
        Ok(recovered + rebuilt)
    }

    fn thumbnail_path(&self, resource_id: &str) -> PathBuf {
        self.cache_root
            .join("thumbnails")
            .join(format!("{resource_id}.png"))
    }
}

fn safe_cache_path(cache_root: &Path, cache_key: &str) -> Result<PathBuf, AppError> {
    let relative = Path::new(cache_key);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(AppError::ResourceStorage);
    }
    Ok(cache_root.join(relative))
}

fn detect_document_media_type(path: &Path) -> Option<&'static str> {
    let media_type = document_media_type_from_extension(path)?;
    let mut prefix = [0_u8; 8];
    let read = std::fs::File::open(path)
        .and_then(|mut file| std::io::Read::read(&mut file, &mut prefix))
        .ok()?;
    let prefix = &prefix[..read];
    let signature_matches = match media_type {
        "application/pdf" => prefix.starts_with(b"%PDF-"),
        "application/vnd.ms-excel" => prefix.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]),
        "text/csv" => !prefix.contains(&0),
        _ => prefix.starts_with(b"PK"),
    };
    if !signature_matches {
        return None;
    }
    is_supported_media_type(media_type).then_some(media_type)
}

fn document_media_type_from_extension(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "pdf" => Some("application/pdf"),
        "docx" => Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
        "xls" => Some("application/vnd.ms-excel"),
        "xlsx" => Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
        "xlsm" => Some("application/vnd.ms-excel.sheet.macroenabled.12"),
        "xlsb" => Some("application/vnd.ms-excel.sheet.binary.macroenabled.12"),
        "csv" => Some("text/csv"),
        _ => None,
    }
}

fn document_failure_code(error: &DocumentRuntimeError) -> &'static str {
    match error {
        DocumentRuntimeError::InvalidRequest | DocumentRuntimeError::ParseFailed => {
            "document_parse_failed"
        }
        DocumentRuntimeError::OutputTooLarge => "document_output_too_large",
        DocumentRuntimeError::OcrFailed => "document_ocr_failed",
        DocumentRuntimeError::Unavailable => "document_runtime_unavailable",
        DocumentRuntimeError::Timeout => "document_runtime_timeout",
    }
}

async fn decode_image(bytes: Vec<u8>) -> Result<(&'static str, image::DynamicImage), &'static str> {
    tokio::task::spawn_blocking(move || {
        let format = image::guess_format(&bytes).map_err(|_| "unsupported_image")?;
        let media_type = match format {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Gif => "image/gif",
            ImageFormat::WebP => "image/webp",
            _ => return Err("unsupported_image"),
        };
        let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
        limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
        limits.max_alloc = Some(MAX_DECODE_BYTES);
        reader.limits(limits);
        reader
            .decode()
            .map(|image| (media_type, image))
            .map_err(|_| "image_decode_failed")
    })
    .await
    .map_err(|_| "image_decode_failed")?
}

async fn write_thumbnail(
    cache_root: &Path,
    resource_id: &str,
    image: image::DynamicImage,
) -> Result<(), ()> {
    let path = cache_root
        .join("thumbnails")
        .join(format!("{resource_id}.png"));
    tokio::task::spawn_blocking(move || {
        let parent = path.parent().ok_or(())?;
        std::fs::create_dir_all(parent).map_err(|_| ())?;
        image
            .thumbnail(THUMBNAIL_EDGE, THUMBNAIL_EDGE)
            .save_with_format(path, ImageFormat::Png)
            .map_err(|_| ())
    })
    .await
    .map_err(|_| ())?
}
