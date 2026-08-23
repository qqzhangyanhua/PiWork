mod process;
mod protocol;

use async_trait::async_trait;

pub use process::ProcessDocumentRuntime;
pub use protocol::{
    DocumentFailureCode, DocumentRequest, DocumentResult, DocumentRuntimeError,
    DocumentRuntimeResponse, MAX_DOCUMENT_OUTPUT_BYTES, is_supported_media_type, validate_request,
};

#[async_trait]
pub trait DocumentRuntime: Send + Sync {
    async fn extract(
        &self,
        request: DocumentRequest,
    ) -> Result<DocumentResult, DocumentRuntimeError>;
}
