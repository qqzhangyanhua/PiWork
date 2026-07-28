use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

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
    /// A Work checkpoint with no active execution; its current Run is terminal or absent.
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
    AskEveryStep,
    #[default]
    Balanced,
    AutoExecute,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct WorkSummary {
    // Canonical UUID string.
    pub id: String,
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
    // Canonical UUID string.
    pub id: String,
    // Canonical UUID string of the owning Work.
    pub work_id: String,
    pub engine_kind: String,
    pub engine_session_id: Option<String>,
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

    use super::{CreateWorkInput, PermissionMode, RunStatus, RunSummary, WorkStatus, WorkSummary};

    fn assert_string(_: &String) {}

    #[test]
    fn permission_mode_defaults_to_balanced() {
        assert_eq!(PermissionMode::default(), PermissionMode::Balanced);
    }

    #[test]
    fn permission_modes_use_the_approved_wire_values() {
        for (mode, wire_value) in [
            (PermissionMode::AskEveryStep, "ask_every_step"),
            (PermissionMode::Balanced, "balanced"),
            (PermissionMode::AutoExecute, "auto_execute"),
        ] {
            assert_eq!(serde_json::to_value(mode).unwrap(), wire_value);
            assert_eq!(
                serde_json::from_value::<PermissionMode>(json!(wire_value)).unwrap(),
                mode
            );
        }
    }

    #[test]
    fn work_dto_uses_camel_case_fields_and_snake_case_values() {
        let id = "00000000-0000-0000-0000-000000000000".to_string();
        let timestamp = Utc.with_ymd_and_hms(2026, 7, 28, 7, 0, 0).unwrap();
        let summary = WorkSummary {
            id: id.clone(),
            title: "Ship PiWork".into(),
            goal: "Build the foundation".into(),
            root_path: "D:/dev/PiWork".into(),
            permission_mode: PermissionMode::Balanced,
            status: WorkStatus::Waiting,
            created_at: timestamp,
            updated_at: timestamp,
        };

        assert_string(&summary.id);

        assert_eq!(
            serde_json::to_value(summary).unwrap(),
            json!({
                "id": id,
                "title": "Ship PiWork",
                "goal": "Build the foundation",
                "rootPath": "D:/dev/PiWork",
                "permissionMode": "balanced",
                "status": "waiting",
                "createdAt": "2026-07-28T07:00:00Z",
                "updatedAt": "2026-07-28T07:00:00Z"
            })
        );
    }

    #[test]
    fn run_ids_round_trip_as_strings() {
        let timestamp = Utc.with_ymd_and_hms(2026, 7, 28, 7, 0, 0).unwrap();
        let run = RunSummary {
            id: "10000000-0000-0000-0000-000000000000".into(),
            work_id: "20000000-0000-0000-0000-000000000000".into(),
            engine_kind: "test-engine".into(),
            engine_session_id: Some("session-1".into()),
            model_label: "test-model".into(),
            status: RunStatus::Queued,
            created_at: timestamp,
            started_at: None,
            completed_at: None,
        };

        let serialized = serde_json::to_string(&run).unwrap();
        let round_trip: RunSummary = serde_json::from_str(&serialized).unwrap();

        assert_string(&round_trip.id);
        assert_string(&round_trip.work_id);
        assert_eq!(round_trip, run);
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
