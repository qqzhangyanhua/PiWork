mod process;
mod protocol;
mod xberg;

use std::path::PathBuf;

use async_trait::async_trait;

pub use process::ProcessDocumentRuntime;
pub use protocol::{DocumentFailureCode, DocumentRequest, DocumentResult, DocumentRuntimeResponse};
pub use xberg::{MAX_DOCUMENT_OUTPUT_BYTES, XbergDocumentRuntime, validate_request};

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

pub fn run_child_from_stdio(derivative_root: PathBuf) -> i32 {
    process::run_child_from_stdio(derivative_root)
}
