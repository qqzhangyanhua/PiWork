use crate::engine::EngineDocument;

pub const MAX_DOCUMENTS_PER_RUN: usize = 6;
pub const MAX_DOCUMENT_CHARS_PER_RUN: usize = 24_000;
pub const MAX_TOTAL_DOCUMENT_CHARS_PER_RUN: usize = 64_000;

pub fn bounded_document(
    name: String,
    media_type: String,
    content: &str,
    remaining_total: usize,
) -> EngineDocument {
    let limit = MAX_DOCUMENT_CHARS_PER_RUN.min(remaining_total);
    let mut chars = content.chars();
    let bounded = chars.by_ref().take(limit).collect::<String>();
    let truncated = chars.next().is_some();
    EngineDocument {
        name,
        media_type,
        content: bounded,
        truncated,
    }
}
