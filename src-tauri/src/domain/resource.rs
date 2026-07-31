use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

macro_rules! binding_path {
    () => {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings/")
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, TS)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[ts(rename_all = "snake_case", export_to = binding_path!())]
pub enum ResourceStatus {
    Staging,
    Processing,
    Ready,
    Failed,
    Deleting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, TS)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[ts(rename_all = "snake_case", export_to = binding_path!())]
pub enum ResourceOrigin {
    UserUpload,
    GeneratedArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct ResourceSummary {
    pub id: String,
    pub original_name: String,
    pub media_type: String,
    pub size: u64,
    pub origin: ResourceOrigin,
    pub status: ResourceStatus,
    pub failure_code: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct ResourceThumbnail {
    pub media_type: String,
    pub data_base64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct ImportResourcesInput {
    pub source_paths: Vec<String>,
    pub draft_id: Option<String>,
    pub work_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::{ImportResourcesInput, ResourceOrigin, ResourceStatus, ResourceSummary};

    #[test]
    fn import_input_hides_source_paths_from_resource_output() {
        let input = ImportResourcesInput {
            source_paths: vec!["C:/secret/photo.png".into()],
            draft_id: Some("draft-1".into()),
            work_id: None,
        };
        assert_eq!(serde_json::to_value(input).unwrap()["draftId"], "draft-1");

        let summary = ResourceSummary {
            id: "resource-1".into(),
            original_name: "photo.png".into(),
            media_type: "image/png".into(),
            size: 42,
            origin: ResourceOrigin::UserUpload,
            status: ResourceStatus::Ready,
            failure_code: None,
            created_at: Utc.with_ymd_and_hms(2026, 7, 31, 0, 0, 0).unwrap(),
        };
        let value = serde_json::to_value(summary).unwrap();
        assert!(value.get("sourcePath").is_none());
        assert_eq!(value["originalName"], "photo.png");
    }
}
