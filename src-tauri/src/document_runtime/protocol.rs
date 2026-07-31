use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
