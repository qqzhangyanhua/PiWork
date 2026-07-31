use std::path::PathBuf;

use piwork_lib::document_runtime::{DocumentRequest, validate_request};

#[test]
fn document_runtime_rejects_relative_and_escaping_paths() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.pdf");
    std::fs::write(&source, b"pdf").unwrap();

    let relative = DocumentRequest {
        source_path: PathBuf::from("source.pdf"),
        media_type: "application/pdf".into(),
        output_path: root.path().join("documents/resource.md"),
    };
    assert!(validate_request(&relative, root.path()).is_err());

    let escaping = DocumentRequest {
        source_path: source,
        media_type: "application/pdf".into(),
        output_path: root.path().join("../outside.md"),
    };
    assert!(validate_request(&escaping, root.path()).is_err());
}

#[test]
fn document_runtime_protocol_is_typed_and_rejects_unknown_fields() {
    use piwork_lib::document_runtime::{DocumentResult, DocumentRuntimeResponse};

    let response = DocumentRuntimeResponse::Success {
        result: DocumentResult {
            content_sha256: "a".repeat(64),
            content_chars: 42,
            used_ocr: true,
            extractor: "xberg".into(),
            extractor_version: "1.0.5".into(),
        },
    };
    let json = serde_json::to_string(&response).unwrap();
    assert_eq!(
        serde_json::from_str::<DocumentRuntimeResponse>(&json).unwrap(),
        response
    );
    assert!(
        serde_json::from_str::<DocumentRuntimeResponse>(
            r#"{"status":"failure","code":"parse_failed","diagnostic":"private path"}"#,
        )
        .is_err()
    );
}

#[test]
fn document_runtime_accepts_only_the_locked_document_formats() {
    let supported = [
        "application/pdf",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "application/vnd.ms-excel",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "application/vnd.ms-excel.sheet.macroenabled.12",
        "application/vnd.ms-excel.sheet.binary.macroenabled.12",
        "text/csv",
    ];
    for media_type in supported {
        assert!(piwork_lib::document_runtime::is_supported_media_type(
            media_type
        ));
    }
    assert!(!piwork_lib::document_runtime::is_supported_media_type(
        "application/zip"
    ));
}
