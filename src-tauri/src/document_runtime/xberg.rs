use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use xberg::{ExtractInput, ExtractionConfig, OcrConfig, OcrStrategy, OutputFormat};

use super::{
    DocumentRequest, DocumentResult, DocumentRuntime, DocumentRuntimeError, is_supported_media_type,
};

pub const MAX_DOCUMENT_OUTPUT_BYTES: u64 = 10 * 1024 * 1024;

pub fn validate_request(
    request: &DocumentRequest,
    derivative_root: &Path,
) -> Result<(), DocumentRuntimeError> {
    let has_parent = request
        .output_path
        .components()
        .any(|component| component == Component::ParentDir);
    if !request.source_path.is_absolute()
        || !request.source_path.is_file()
        || !request.output_path.is_absolute()
        || !derivative_root.is_absolute()
        || has_parent
        || !request.output_path.starts_with(derivative_root)
        || !is_supported_media_type(&request.media_type)
    {
        return Err(DocumentRuntimeError::InvalidRequest);
    }
    Ok(())
}

#[derive(Clone)]
pub struct XbergDocumentRuntime {
    derivative_root: Arc<PathBuf>,
}

impl XbergDocumentRuntime {
    pub fn new(derivative_root: PathBuf) -> Self {
        Self {
            derivative_root: Arc::new(derivative_root),
        }
    }
}

#[async_trait]
impl DocumentRuntime for XbergDocumentRuntime {
    async fn extract(
        &self,
        request: DocumentRequest,
    ) -> Result<DocumentResult, DocumentRuntimeError> {
        validate_request(&request, &self.derivative_root)?;
        extract_document(request).await
    }
}

async fn extract_document(
    request: DocumentRequest,
) -> Result<DocumentResult, DocumentRuntimeError> {
    let config = ExtractionConfig {
        output_format: OutputFormat::Markdown,
        ocr: Some(OcrConfig::default()),
        ocr_strategy: OcrStrategy::ScannedPages {
            min_confidence: 0.7,
        },
        extraction_timeout_secs: Some(110),
        ..Default::default()
    };
    let mut input = ExtractInput::from_uri(request.source_path.to_string_lossy().into_owned());
    input.mime_type = Some(request.media_type.clone());
    let extracted = xberg::extract(input, &config)
        .await
        .map_err(|error| {
            if request.media_type == "application/pdf"
                && error.to_string().to_ascii_lowercase().contains("ocr")
            {
                DocumentRuntimeError::OcrFailed
            } else {
                DocumentRuntimeError::ParseFailed
            }
        })?
        .results
        .into_iter()
        .next()
        .ok_or(DocumentRuntimeError::ParseFailed)?;
    let bytes = extracted.content.as_bytes();
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_DOCUMENT_OUTPUT_BYTES {
        return Err(DocumentRuntimeError::OutputTooLarge);
    }
    let parent = request
        .output_path
        .parent()
        .ok_or(DocumentRuntimeError::InvalidRequest)?;
    std::fs::create_dir_all(parent).map_err(|_| DocumentRuntimeError::Unavailable)?;
    let partial = request.output_path.with_extension("md.part");
    let publish = std::fs::write(&partial, bytes)
        .and_then(|()| std::fs::rename(&partial, &request.output_path));
    if publish.is_err() {
        let _ = std::fs::remove_file(partial);
        return Err(DocumentRuntimeError::Unavailable);
    }
    Ok(DocumentResult {
        content_sha256: format!("{:x}", Sha256::digest(bytes)),
        content_chars: u64::try_from(extracted.content.chars().count()).unwrap_or(u64::MAX),
        used_ocr: extracted
            .extraction_method
            .is_some_and(|method| method.used_ocr()),
        extractor: "xberg".into(),
        extractor_version: "1.0.5".into(),
    })
}
