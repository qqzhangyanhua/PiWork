use std::{path::PathBuf, sync::Arc};

use piwork_lib::{
    document_runtime::{DocumentRequest, DocumentResult, DocumentRuntime, DocumentRuntimeError},
    domain::{
        resource::{ImportResourcesInput, ResourceStatus, ResourceSummary},
        work::{CreateWorkInput, PermissionMode, WorkDetail},
    },
    error::AppError,
    resource::{
        blob_store::{BlobInput, BlobStore},
        local_blob_store::LocalBlobStore,
        repository::ResourceRepository,
        service::ResourceService,
    },
    storage::sqlite::Database,
    work::repository::WorkRepository,
};

struct FakeDocumentRuntime;

#[async_trait::async_trait]
impl DocumentRuntime for FakeDocumentRuntime {
    async fn extract(
        &self,
        request: DocumentRequest,
    ) -> Result<DocumentResult, DocumentRuntimeError> {
        let parent = request.output_path.parent().unwrap();
        std::fs::create_dir_all(parent).unwrap();
        std::fs::write(&request.output_path, "# Quarterly report\n\nRevenue: 42").unwrap();
        Ok(DocumentResult {
            content_sha256: "a".repeat(64),
            content_chars: 31,
            used_ocr: false,
            extractor: "fake-docs".into(),
            extractor_version: "1".into(),
        })
    }
}

struct Fixture {
    _root: tempfile::TempDir,
    database: Database,
    repository: ResourceRepository,
    work_repository: WorkRepository,
    blob_store: Arc<LocalBlobStore>,
    service: ResourceService,
}

impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let database = Database::open_in_memory().await.unwrap();
        let repository = ResourceRepository::new(database.pool().clone());
        let work_repository = WorkRepository::new(database.pool().clone());
        let blob_store = Arc::new(LocalBlobStore::new(root.path().join("resources")));
        let service = ResourceService::new(
            repository.clone(),
            Arc::clone(&blob_store),
            root.path().join("resource-cache"),
        );
        Self {
            _root: root,
            database,
            repository,
            work_repository,
            blob_store,
            service,
        }
    }

    fn workspace_path(&self) -> String {
        let path = self._root.path().join("workspace");
        std::fs::create_dir_all(&path).unwrap();
        path.to_string_lossy().into_owned()
    }

    async fn create_work(&self, title: &str) -> WorkDetail {
        self.work_repository
            .create(CreateWorkInput {
                title: title.into(),
                goal: title.into(),
                root_path: self.workspace_path(),
                permission_mode: PermissionMode::Balanced,
                resource_draft_id: None,
            })
            .await
            .unwrap()
    }

    async fn import_ready_png(&self, draft_id: &str, name: &str) -> ResourceSummary {
        let source = self.write_png(name);
        self.service
            .import_resources(ImportResourcesInput {
                source_paths: vec![source.to_string_lossy().into_owned()],
                draft_id: Some(draft_id.into()),
                work_id: None,
            })
            .await
            .unwrap()
            .remove(0)
    }

    async fn import_ready_work_png(&self, work_id: &str, name: &str) -> ResourceSummary {
        let source = self.write_png(name);
        self.service
            .import_resources(ImportResourcesInput {
                source_paths: vec![source.to_string_lossy().into_owned()],
                draft_id: None,
                work_id: Some(work_id.into()),
            })
            .await
            .unwrap()
            .remove(0)
    }

    fn write_png(&self, name: &str) -> PathBuf {
        let path = self._root.path().join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        image::DynamicImage::new_rgba8(1, 1).save(&path).unwrap();
        path
    }

    fn write_bytes(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self._root.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }
}

#[tokio::test]
async fn creating_a_work_adopts_its_resource_draft() {
    let fixture = Fixture::new().await;
    let resource = fixture.import_ready_png("draft-42", "brief.png").await;
    let work = fixture
        .work_repository
        .create(CreateWorkInput {
            title: "Review brief".into(),
            goal: "Review the attached image".into(),
            root_path: fixture.workspace_path(),
            permission_mode: PermissionMode::Balanced,
            resource_draft_id: Some("draft-42".into()),
        })
        .await
        .unwrap();

    let resources = fixture
        .repository
        .list_for_work(&work.summary.id)
        .await
        .unwrap();
    assert_eq!(resources, vec![resource]);
    let draft_links: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM resource_links WHERE draft_id = 'draft-42'")
            .fetch_one(fixture.database.pool())
            .await
            .unwrap();
    assert_eq!(draft_links, 0);
}

#[tokio::test]
async fn begin_run_links_only_ready_resources_owned_by_the_work() {
    let fixture = Fixture::new().await;
    let work = fixture.create_work("Attached run").await;
    let resource = fixture
        .import_ready_work_png(&work.summary.id, "chart.png")
        .await;

    let started = fixture
        .work_repository
        .begin_run(
            &work.summary.id,
            "Explain this",
            std::slice::from_ref(&resource.id),
            "fake",
            "Fake model",
        )
        .await
        .unwrap();

    assert_eq!(started.user_message.resource_ids, vec![resource.id.clone()]);
    let reloaded = fixture
        .work_repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reloaded.messages[0].resource_ids, vec![resource.id]);
}

#[tokio::test]
async fn attachment_only_run_is_valid_but_empty_run_is_rejected() {
    let fixture = Fixture::new().await;
    let work = fixture.create_work("Image only").await;
    let resource = fixture
        .import_ready_work_png(&work.summary.id, "photo.png")
        .await;

    fixture
        .work_repository
        .begin_run(&work.summary.id, "", &[resource.id], "fake", "Fake model")
        .await
        .unwrap();

    let other = fixture.create_work("Empty").await;
    let error = fixture
        .work_repository
        .begin_run(&other.summary.id, "", &[], "fake", "Fake model")
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::InvalidInput { field, .. } if field == "prompt"));
}

#[tokio::test]
async fn recovery_garbage_collects_abandoned_drafts() {
    let fixture = Fixture::new().await;
    let source = fixture.write_png("abandoned.png");
    let resource = fixture
        .service
        .import_resources(ImportResourcesInput {
            source_paths: vec![source.to_string_lossy().into_owned()],
            draft_id: Some("stale-draft".into()),
            work_id: None,
        })
        .await
        .unwrap()
        .remove(0);
    sqlx::query(
        "UPDATE resource_links SET created_at = '2026-01-01T00:00:00Z' \
         WHERE resource_id = ?",
    )
    .bind(&resource.id)
    .execute(fixture.database.pool())
    .await
    .unwrap();
    let object = fixture.repository.storage_ref(&resource.id).await.unwrap();
    let object_path = fixture.blob_store.path_for(&object).unwrap();

    fixture.service.recover_interrupted_imports().await.unwrap();

    assert!(
        fixture
            .repository
            .find(&resource.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(!object_path.exists());
}

#[tokio::test]
async fn recovery_removes_old_local_objects_without_replica_records() {
    let fixture = Fixture::new().await;
    let source = fixture.write_png("orphan.png");
    let stored = fixture
        .blob_store
        .put(BlobInput {
            source_path: source,
        })
        .await
        .unwrap();
    let object_path = fixture.blob_store.path_for(&stored.reference).unwrap();
    filetime::set_file_mtime(
        &object_path,
        filetime::FileTime::from_unix_time(1_700_000_000, 0),
    )
    .unwrap();

    fixture.service.recover_interrupted_imports().await.unwrap();

    assert!(!object_path.exists());
}

#[tokio::test]
async fn imports_a_png_without_retaining_its_source_path() {
    let fixture = Fixture::new().await;
    let source = fixture.write_png("private/source/avatar.png");
    let imported = fixture
        .service
        .import_resources(ImportResourcesInput {
            source_paths: vec![source.to_string_lossy().into_owned()],
            draft_id: Some("draft-1".into()),
            work_id: None,
        })
        .await
        .unwrap();

    assert_eq!(imported.len(), 1);
    assert_eq!(imported[0].status, ResourceStatus::Ready);
    assert_eq!(imported[0].original_name, "avatar.png");
    let stored: String =
        sqlx::query_scalar("SELECT object_key FROM blob_replicas WHERE store_id = 'local-default'")
            .fetch_one(fixture.database.pool())
            .await
            .unwrap();
    assert!(!stored.contains("private"));
    assert!(!stored.contains("avatar.png"));
}

#[tokio::test]
async fn one_invalid_file_does_not_discard_a_valid_file() {
    let fixture = Fixture::new().await;
    let valid = fixture.write_png("valid.png");
    let invalid = fixture.write_bytes("script.png", b"not an image");
    let imported = fixture
        .service
        .import_resources(ImportResourcesInput {
            source_paths: vec![
                valid.to_string_lossy().into_owned(),
                invalid.to_string_lossy().into_owned(),
            ],
            draft_id: Some("draft-2".into()),
            work_id: None,
        })
        .await
        .unwrap();

    assert_eq!(imported[0].status, ResourceStatus::Ready);
    assert_eq!(imported[1].status, ResourceStatus::Failed);
    assert_eq!(
        imported[1].failure_code.as_deref(),
        Some("unsupported_image")
    );
}

#[tokio::test]
async fn repeated_bytes_share_a_blob_but_keep_distinct_resources() {
    let fixture = Fixture::new().await;
    let first = fixture.write_png("first.png");
    let second = fixture._root.path().join("second.png");
    std::fs::copy(&first, &second).unwrap();
    let imported = fixture
        .service
        .import_resources(ImportResourcesInput {
            source_paths: vec![
                first.to_string_lossy().into_owned(),
                second.to_string_lossy().into_owned(),
            ],
            draft_id: Some("draft-3".into()),
            work_id: None,
        })
        .await
        .unwrap();

    assert_ne!(imported[0].id, imported[1].id);
    let blobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM resource_blobs")
        .fetch_one(fixture.database.pool())
        .await
        .unwrap();
    assert_eq!(blobs, 1);
}

#[tokio::test]
async fn engine_images_rejects_more_than_eight_images() {
    let fixture = Fixture::new().await;
    let work = fixture.create_work("Image count limit").await;
    let mut resource_ids = Vec::new();
    for index in 0..9 {
        resource_ids.push(
            fixture
                .import_ready_work_png(&work.summary.id, &format!("image-{index}.png"))
                .await
                .id,
        );
    }

    let error = fixture
        .service
        .engine_images(&work.summary.id, &resource_ids)
        .await
        .unwrap_err();

    assert!(matches!(error, AppError::ResourceImport { code } if code == "too_many_images"));
}

#[tokio::test]
async fn engine_images_rejects_a_combined_payload_over_twenty_four_mib() {
    let fixture = Fixture::new().await;
    let work = fixture.create_work("Image byte limit").await;
    let mut resources = Vec::new();
    for index in 0..3 {
        resources.push(
            fixture
                .import_ready_work_png(&work.summary.id, &format!("large-{index}.png"))
                .await,
        );
    }
    let mut resource_ids = Vec::new();
    for resource in resources {
        let object = fixture.repository.storage_ref(&resource.id).await.unwrap();
        let object_path = fixture.blob_store.path_for(&object).unwrap();
        sqlx::query("UPDATE managed_resources SET size = ? WHERE id = ?")
            .bind(9_i64 * 1024 * 1024)
            .bind(&resource.id)
            .execute(fixture.database.pool())
            .await
            .unwrap();
        std::fs::write(object_path, vec![0_u8; 9 * 1024 * 1024]).unwrap();
        resource_ids.push(resource.id);
    }

    let error = fixture
        .service
        .engine_images(&work.summary.id, &resource_ids)
        .await
        .unwrap_err();

    assert!(matches!(error, AppError::ResourceImport { code } if code == "image_budget_exceeded"));
}

#[tokio::test]
async fn engine_images_rejects_a_resource_owned_by_another_work() {
    let fixture = Fixture::new().await;
    let owner = fixture.create_work("Owner").await;
    let requester = fixture.create_work("Requester").await;
    let resource = fixture
        .import_ready_work_png(&owner.summary.id, "private.png")
        .await;

    let error = fixture
        .service
        .engine_images(&requester.summary.id, &[resource.id])
        .await
        .unwrap_err();

    assert!(
        matches!(error, AppError::ResourceImport { code } if code == "resource_not_ready_or_unlinked")
    );
}

#[tokio::test]
async fn imports_a_pdf_into_a_retained_blob_and_ready_markdown_derivative() {
    let fixture = Fixture::new().await;
    let work = fixture.create_work("Document import").await;
    let source = fixture.write_bytes("quarterly.pdf", b"%PDF-1.7 fake fixture");
    let service = ResourceService::new(
        fixture.repository.clone(),
        Arc::clone(&fixture.blob_store),
        fixture._root.path().join("resource-cache"),
    )
    .with_document_runtime(Arc::new(FakeDocumentRuntime));

    let imported = service
        .import_resources(ImportResourcesInput {
            source_paths: vec![source.to_string_lossy().into_owned()],
            draft_id: None,
            work_id: Some(work.summary.id),
        })
        .await
        .unwrap()
        .remove(0);
    std::fs::remove_file(source).unwrap();

    assert_eq!(imported.status, ResourceStatus::Ready);
    assert_eq!(imported.media_type, "application/pdf");
    let blob = fixture.repository.storage_ref(&imported.id).await.unwrap();
    assert!(fixture.blob_store.path_for(&blob).unwrap().is_file());
    assert!(
        fixture
            ._root
            .path()
            .join("resource-cache/documents")
            .join(format!("{}.md", imported.id))
            .is_file()
    );
}

#[tokio::test]
async fn document_import_accepts_every_first_pack_format_by_extension_and_signature() {
    let fixture = Fixture::new().await;
    let work = fixture.create_work("Document formats").await;
    let service = ResourceService::new(
        fixture.repository.clone(),
        Arc::clone(&fixture.blob_store),
        fixture._root.path().join("resource-cache"),
    )
    .with_document_runtime(Arc::new(FakeDocumentRuntime));
    let cases: [(&str, &[u8], &str); 7] = [
        ("sample.pdf", b"%PDF-1.7 fixture", "application/pdf"),
        (
            "sample.docx",
            b"PK\x03\x04fixture",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ),
        (
            "sample.xls",
            b"\xD0\xCF\x11\xE0fixture",
            "application/vnd.ms-excel",
        ),
        (
            "sample.xlsx",
            b"PK\x03\x04fixture",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        ),
        (
            "sample.xlsm",
            b"PK\x03\x04fixture",
            "application/vnd.ms-excel.sheet.macroenabled.12",
        ),
        (
            "sample.xlsb",
            b"PK\x03\x04fixture",
            "application/vnd.ms-excel.sheet.binary.macroenabled.12",
        ),
        ("sample.csv", b"name,value\nRevenue,42", "text/csv"),
    ];

    for (name, bytes, expected_media_type) in cases {
        let source = fixture.write_bytes(name, bytes);
        let resource = service
            .import_resources(ImportResourcesInput {
                source_paths: vec![source.to_string_lossy().into_owned()],
                draft_id: None,
                work_id: Some(work.summary.id.clone()),
            })
            .await
            .unwrap()
            .remove(0);
        assert_eq!(resource.status, ResourceStatus::Ready, "{name}");
        assert_eq!(resource.media_type, expected_media_type, "{name}");
    }
}

#[tokio::test]
async fn corrupt_office_upload_fails_independently_and_retains_its_original_blob() {
    let fixture = Fixture::new().await;
    let work = fixture.create_work("Corrupt document").await;
    let valid = fixture.write_png("still-valid.png");
    let corrupt = fixture.write_bytes("corrupt.docx", b"not an office zip");

    let imported = fixture
        .service
        .import_resources(ImportResourcesInput {
            source_paths: vec![
                corrupt.to_string_lossy().into_owned(),
                valid.to_string_lossy().into_owned(),
            ],
            draft_id: None,
            work_id: Some(work.summary.id),
        })
        .await
        .unwrap();

    assert_eq!(imported[0].status, ResourceStatus::Failed);
    assert_eq!(
        imported[0].failure_code.as_deref(),
        Some("unsupported_document")
    );
    assert_eq!(imported[1].status, ResourceStatus::Ready);
    let retained: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM managed_resources WHERE id = ? AND blob_id IS NOT NULL",
    )
    .bind(&imported[0].id)
    .fetch_one(fixture.database.pool())
    .await
    .unwrap();
    assert_eq!(retained, 1);
}

#[tokio::test]
async fn document_context_preserves_order_and_unicode_budget_boundaries() {
    let fixture = Fixture::new().await;
    let work = fixture.create_work("Document context").await;
    let service = ResourceService::new(
        fixture.repository.clone(),
        Arc::clone(&fixture.blob_store),
        fixture._root.path().join("resource-cache"),
    )
    .with_document_runtime(Arc::new(FakeDocumentRuntime));
    let mut resource_ids = Vec::new();
    for index in 0..7 {
        let source = fixture.write_bytes(
            &format!("report-{index}.pdf"),
            format!("%PDF-1.7 fixture {index}").as_bytes(),
        );
        let resource = service
            .import_resources(ImportResourcesInput {
                source_paths: vec![source.to_string_lossy().into_owned()],
                draft_id: None,
                work_id: Some(work.summary.id.clone()),
            })
            .await
            .unwrap()
            .remove(0);
        let content = if index == 0 {
            "界".repeat(24_001)
        } else {
            format!("document-{index}")
        };
        std::fs::write(
            fixture
                ._root
                .path()
                .join("resource-cache/documents")
                .join(format!("{}.md", resource.id)),
            content,
        )
        .unwrap();
        resource_ids.push(resource.id);
    }

    let attachments = service
        .engine_attachments(&work.summary.id, &resource_ids)
        .await
        .unwrap();

    assert!(attachments.images.is_empty());
    assert_eq!(attachments.documents.len(), 6);
    assert_eq!(attachments.documents[0].name, "report-0.pdf");
    assert_eq!(attachments.documents[1].name, "report-1.pdf");
    assert_eq!(attachments.documents[0].content.chars().count(), 24_000);
    assert!(attachments.documents[0].truncated);
    assert_eq!(
        attachments
            .documents
            .iter()
            .map(|document| document.content.chars().count())
            .sum::<usize>(),
        24_050
    );
}

#[tokio::test]
async fn recovery_marks_stale_document_processing_with_a_safe_runtime_code() {
    let fixture = Fixture::new().await;
    let work = fixture.create_work("Interrupted document").await;
    let source = fixture.write_bytes("interrupted.pdf", b"%PDF-1.7 fixture");
    let service = ResourceService::new(
        fixture.repository.clone(),
        Arc::clone(&fixture.blob_store),
        fixture._root.path().join("resource-cache"),
    )
    .with_document_runtime(Arc::new(FakeDocumentRuntime));
    let resource = service
        .import_resources(ImportResourcesInput {
            source_paths: vec![source.to_string_lossy().into_owned()],
            draft_id: None,
            work_id: Some(work.summary.id),
        })
        .await
        .unwrap()
        .remove(0);
    sqlx::query(
        "UPDATE managed_resources SET status = 'processing', updated_at = '2026-01-01T00:00:00Z' WHERE id = ?",
    )
    .bind(&resource.id)
    .execute(fixture.database.pool())
    .await
    .unwrap();
    sqlx::query(
        "UPDATE resource_derivatives SET state = 'processing', updated_at = '2026-01-01T00:00:00Z' WHERE resource_id = ?",
    )
    .bind(&resource.id)
    .execute(fixture.database.pool())
    .await
    .unwrap();
    let orphan_part = fixture
        ._root
        .path()
        .join("resource-cache/documents/orphan.md.part");
    std::fs::write(&orphan_part, "partial private extraction").unwrap();

    service.recover_interrupted_imports().await.unwrap();

    let recovered = fixture
        .repository
        .find(&resource.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered.status, ResourceStatus::Failed);
    assert_eq!(
        recovered.failure_code.as_deref(),
        Some("document_runtime_unavailable")
    );
    assert!(fixture.repository.storage_ref(&resource.id).await.is_err());
    let retained_blob: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM managed_resources WHERE id = ? AND blob_id IS NOT NULL",
    )
    .bind(resource.id)
    .fetch_one(fixture.database.pool())
    .await
    .unwrap();
    assert_eq!(retained_blob, 1);
    assert!(!orphan_part.exists());
}

#[tokio::test]
async fn recovery_rebuilds_a_missing_document_derivative_from_the_retained_blob() {
    let fixture = Fixture::new().await;
    let work = fixture.create_work("Rebuild document").await;
    let source = fixture.write_bytes("rebuild.pdf", b"%PDF-1.7 fixture");
    let service = ResourceService::new(
        fixture.repository.clone(),
        Arc::clone(&fixture.blob_store),
        fixture._root.path().join("resource-cache"),
    )
    .with_document_runtime(Arc::new(FakeDocumentRuntime));
    let resource = service
        .import_resources(ImportResourcesInput {
            source_paths: vec![source.to_string_lossy().into_owned()],
            draft_id: None,
            work_id: Some(work.summary.id),
        })
        .await
        .unwrap()
        .remove(0);
    let derivative = fixture
        ._root
        .path()
        .join("resource-cache/documents")
        .join(format!("{}.md", resource.id));
    std::fs::remove_file(&derivative).unwrap();

    service.recover_interrupted_imports().await.unwrap();

    assert!(derivative.is_file());
    let recovered = fixture
        .repository
        .find(&resource.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered.status, ResourceStatus::Ready);
    assert_eq!(recovered.failure_code, None);
}
