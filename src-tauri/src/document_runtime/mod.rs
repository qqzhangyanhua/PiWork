mod process;
mod protocol;

use std::path::{Component, Path};

use async_trait::async_trait;

pub use process::ProcessDocumentRuntime;
pub use protocol::{DocumentFailureCode, DocumentRequest, DocumentResult, DocumentRuntimeResponse};

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

#[derive(Debug, thiserror::Error)]
pub enum DocumentRuntimeError {
    #[error("document request is invalid")]
    InvalidRequest,
    #[error("document parsing failed")]
    ParseFailed,
    #[error("document OCR failed")]
    OcrFailed,
    #[error("document output exceeds its limit")]
    OutputTooLarge,
    #[error("document runtime is unavailable")]
    Unavailable,
    #[error("document runtime timed out")]
    Timeout,
}

#[async_trait]
pub trait DocumentRuntime: Send + Sync {
    async fn extract(
        &self,
        request: DocumentRequest,
    ) -> Result<DocumentResult, DocumentRuntimeError>;
}

pub fn is_supported_media_type(media_type: &str) -> bool {
    matches!(
        media_type,
        "application/pdf"
            | "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            | "application/vnd.ms-excel"
            | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            | "application/vnd.ms-excel.sheet.macroenabled.12"
            | "application/vnd.ms-excel.sheet.binary.macroenabled.12"
            | "text/csv"
    )
}
