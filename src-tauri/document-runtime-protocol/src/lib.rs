use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const MAX_DOCUMENT_OUTPUT_BYTES: u64 = 10 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentRequest {
    pub source_path: PathBuf,
    pub media_type: String,
    pub output_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentResult {
    pub content_sha256: String,
    pub content_chars: u64,
    pub used_ocr: bool,
    pub extractor: String,
    pub extractor_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentFailureCode {
    InvalidRequest,
    ParseFailed,
    OcrFailed,
    OutputTooLarge,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum DocumentRuntimeResponse {
    Success { result: DocumentResult },
    Failure { code: DocumentFailureCode },
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

impl From<DocumentFailureCode> for DocumentRuntimeError {
    fn from(code: DocumentFailureCode) -> Self {
        match code {
            DocumentFailureCode::InvalidRequest => Self::InvalidRequest,
            DocumentFailureCode::ParseFailed => Self::ParseFailed,
            DocumentFailureCode::OcrFailed => Self::OcrFailed,
            DocumentFailureCode::OutputTooLarge => Self::OutputTooLarge,
            DocumentFailureCode::Unavailable => Self::Unavailable,
        }
    }
}

impl From<&DocumentRuntimeError> for DocumentFailureCode {
    fn from(error: &DocumentRuntimeError) -> Self {
        match error {
            DocumentRuntimeError::InvalidRequest => Self::InvalidRequest,
            DocumentRuntimeError::ParseFailed => Self::ParseFailed,
            DocumentRuntimeError::OcrFailed => Self::OcrFailed,
            DocumentRuntimeError::OutputTooLarge => Self::OutputTooLarge,
            DocumentRuntimeError::Unavailable | DocumentRuntimeError::Timeout => Self::Unavailable,
        }
    }
}
