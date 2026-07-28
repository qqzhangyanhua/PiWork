use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::event::WorkEventEnvelope;

macro_rules! binding_path {
    () => {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings/")
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, TS)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[ts(rename_all = "snake_case", export_to = binding_path!())]
pub enum WorkStatus {
    Draft,
    Queued,
    Running,
    Waiting,
    Idle,
    Completed,
    Failed,
    Stopped,
    Interrupted,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, TS)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[ts(rename_all = "snake_case", export_to = binding_path!())]
pub enum RunStatus {
    Queued,
    Running,
    Waiting,
    Completed,
    Failed,
    Stopped,
    Interrupted,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, TS)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[ts(rename_all = "snake_case", export_to = binding_path!())]
pub enum PermissionMode {
    #[default]
    Balanced,
    Strict,
    FullAccess,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct WorkSummary {
    pub id: Uuid,
    pub title: String,
    pub goal: String,
    pub root_path: String,
    pub permission_mode: PermissionMode,
    pub status: WorkStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct RunSummary {
    pub id: Uuid,
    pub work_id: Uuid,
    pub model_label: String,
    pub status: RunStatus,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct WorkDetail {
    pub summary: WorkSummary,
    pub runs: Vec<RunSummary>,
    pub events: Vec<WorkEventEnvelope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct CreateWorkInput {
    pub title: String,
    pub goal: String,
    pub root_path: String,
    pub permission_mode: PermissionMode,
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use serde_json::json;
    use uuid::Uuid;

    use super::{CreateWorkInput, PermissionMode, WorkStatus, WorkSummary};

    #[test]
    fn permission_mode_defaults_to_balanced() {
        assert_eq!(PermissionMode::default(), PermissionMode::Balanced);
    }

    #[test]
    fn work_dto_uses_camel_case_fields_and_snake_case_values() {
        let id = Uuid::nil();
        let timestamp = Utc.with_ymd_and_hms(2026, 7, 28, 7, 0, 0).unwrap();
        let summary = WorkSummary {
            id,
            title: "Ship PiWork".into(),
            goal: "Build the foundation".into(),
            root_path: "D:/dev/PiWork".into(),
            permission_mode: PermissionMode::FullAccess,
            status: WorkStatus::Waiting,
            created_at: timestamp,
            updated_at: timestamp,
        };

        assert_eq!(
            serde_json::to_value(summary).unwrap(),
            json!({
                "id": id,
                "title": "Ship PiWork",
                "goal": "Build the foundation",
                "rootPath": "D:/dev/PiWork",
                "permissionMode": "full_access",
                "status": "waiting",
                "createdAt": "2026-07-28T07:00:00Z",
                "updatedAt": "2026-07-28T07:00:00Z"
            })
        );
    }

    #[test]
    fn create_work_input_uses_the_shared_permission_mode() {
        let input = CreateWorkInput {
            title: "Research".into(),
            goal: "Compare designs".into(),
            root_path: "D:/work".into(),
            permission_mode: PermissionMode::Balanced,
        };

        assert_eq!(
            serde_json::to_value(input).unwrap()["permissionMode"],
            "balanced"
        );
    }
}
