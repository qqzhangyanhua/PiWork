use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::work::MessageRole;

macro_rules! binding_path {
    () => {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings/")
    };
}

macro_rules! wire_enum {
    ($name:ident { $($variant:ident),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, TS)]
        #[serde(rename_all = "snake_case")]
        #[sqlx(type_name = "TEXT", rename_all = "snake_case")]
        #[ts(rename_all = "snake_case", export_to = binding_path!())]
        pub enum $name {
            $($variant),+
        }
    };
}

wire_enum!(AssignmentStatus {
    Queued,
    Claimed,
    Running,
    Waiting,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
    DeadLetter,
    RecoveryConfirmationRequired,
});
wire_enum!(AssignmentKind { Lead, Member });
wire_enum!(AssignmentSideEffect {
    ReadOnly,
    IdempotentWrite,
    NonIdempotentWrite,
    Unknown,
});
wire_enum!(AgentSessionStatus {
    Ready,
    Running,
    Invalidated,
});
wire_enum!(QueueControlMode {
    EnqueueNext,
    SteerCurrent,
    InterruptAndReplace,
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct AssignmentSummary {
    pub id: String,
    pub work_id: String,
    pub parent_assignment_id: Option<String>,
    pub created_by_agent_id: Option<String>,
    pub assigned_agent_id: String,
    pub capability_pack_id: Option<String>,
    pub kind: AssignmentKind,
    pub side_effect: AssignmentSideEffect,
    pub title: String,
    pub instruction: String,
    #[ts(type = "unknown")]
    pub context_manifest: Value,
    #[ts(type = "unknown")]
    pub expected_result_schema: Value,
    #[ts(type = "unknown")]
    pub acceptance_criteria: Value,
    #[ts(type = "unknown")]
    pub permission_scope: Value,
    pub priority: u32,
    pub status: AssignmentStatus,
    pub attempt_count: u32,
    pub max_attempts: u32,
    pub not_before: Option<DateTime<Utc>>,
    pub result_summary: Option<String>,
    pub last_error: Option<String>,
    pub next_attempt_at: Option<DateTime<Utc>>,
    pub recovery_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct AgentSessionSummary {
    pub id: String,
    pub work_id: String,
    pub agent_instance_id: String,
    pub engine_kind: String,
    pub generation: u32,
    pub engine_session_id: Option<String>,
    pub current_assignment_id: Option<String>,
    pub last_successful_turn_id: Option<String>,
    pub rotation_reason: Option<String>,
    pub status: AgentSessionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub invalidated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct QueueWorkInput {
    pub instruction: String,
    pub referenced_files: Vec<String>,
    pub resource_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct SteerAssignmentInput {
    pub instruction: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct InterruptWorkInput {
    pub assignment_id: String,
    pub replacement: QueueWorkInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct UserMessageSummary {
    pub id: String,
    pub work_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub assignment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
    pub role: MessageRole,
    pub content: String,
    pub resource_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use serde::{Serialize, de::DeserializeOwned};
    use serde_json::json;

    use super::{
        AgentSessionStatus, AgentSessionSummary, AssignmentKind, AssignmentSideEffect,
        AssignmentStatus, AssignmentSummary, InterruptWorkInput, QueueControlMode, QueueWorkInput,
        SteerAssignmentInput, UserMessageSummary,
    };

    fn assert_wire_values<T>(cases: &[(T, &str)])
    where
        T: Copy + std::fmt::Debug + PartialEq + Serialize + DeserializeOwned,
    {
        for (value, wire_value) in cases {
            assert_eq!(serde_json::to_value(value).unwrap(), json!(wire_value));
            assert_eq!(
                serde_json::from_value::<T>(json!(wire_value)).unwrap(),
                *value
            );
        }
    }

    #[test]
    fn assignment_enums_use_stable_snake_case_wire_values() {
        assert_wire_values(&[
            (AssignmentStatus::Queued, "queued"),
            (AssignmentStatus::Claimed, "claimed"),
            (AssignmentStatus::Running, "running"),
            (AssignmentStatus::Waiting, "waiting"),
            (AssignmentStatus::Completed, "completed"),
            (AssignmentStatus::Failed, "failed"),
            (AssignmentStatus::Cancelled, "cancelled"),
            (AssignmentStatus::Interrupted, "interrupted"),
            (AssignmentStatus::DeadLetter, "dead_letter"),
            (
                AssignmentStatus::RecoveryConfirmationRequired,
                "recovery_confirmation_required",
            ),
        ]);
        assert_wire_values(&[
            (AssignmentKind::Lead, "lead"),
            (AssignmentKind::Member, "member"),
        ]);
        assert_wire_values(&[
            (AssignmentSideEffect::ReadOnly, "read_only"),
            (AssignmentSideEffect::IdempotentWrite, "idempotent_write"),
            (
                AssignmentSideEffect::NonIdempotentWrite,
                "non_idempotent_write",
            ),
            (AssignmentSideEffect::Unknown, "unknown"),
        ]);
        assert_wire_values(&[
            (AgentSessionStatus::Ready, "ready"),
            (AgentSessionStatus::Running, "running"),
            (AgentSessionStatus::Invalidated, "invalidated"),
        ]);
        assert_wire_values(&[
            (QueueControlMode::EnqueueNext, "enqueue_next"),
            (QueueControlMode::SteerCurrent, "steer_current"),
            (
                QueueControlMode::InterruptAndReplace,
                "interrupt_and_replace",
            ),
        ]);
    }

    #[test]
    fn assignment_summary_uses_complete_camel_case_wire_shape() {
        let timestamp = Utc.with_ymd_and_hms(2026, 8, 15, 4, 0, 0).unwrap();
        let assignment = AssignmentSummary {
            id: "assignment-1".into(),
            work_id: "work-1".into(),
            parent_assignment_id: None,
            created_by_agent_id: None,
            assigned_agent_id: "agent-1".into(),
            capability_pack_id: None,
            kind: AssignmentKind::Lead,
            side_effect: AssignmentSideEffect::ReadOnly,
            title: "Investigate".into(),
            instruction: "Inspect the repository".into(),
            context_manifest: json!({"resourceIds": ["resource-1"]}),
            expected_result_schema: json!({"type": "object"}),
            acceptance_criteria: json!(["tests pass"]),
            permission_scope: json!({"mode": "read_only"}),
            priority: 10,
            status: AssignmentStatus::Queued,
            attempt_count: 0,
            max_attempts: 3,
            not_before: None,
            result_summary: None,
            last_error: None,
            next_attempt_at: None,
            recovery_reason: None,
            created_at: timestamp,
            claimed_at: None,
            started_at: None,
            completed_at: None,
            updated_at: timestamp,
        };

        let value = serde_json::to_value(assignment).unwrap();
        for field in [
            "id",
            "workId",
            "parentAssignmentId",
            "createdByAgentId",
            "assignedAgentId",
            "capabilityPackId",
            "kind",
            "sideEffect",
            "title",
            "instruction",
            "contextManifest",
            "expectedResultSchema",
            "acceptanceCriteria",
            "permissionScope",
            "priority",
            "status",
            "attemptCount",
            "maxAttempts",
            "notBefore",
            "resultSummary",
            "lastError",
            "nextAttemptAt",
            "recoveryReason",
            "createdAt",
            "claimedAt",
            "startedAt",
            "completedAt",
            "updatedAt",
        ] {
            assert!(value.get(field).is_some(), "missing {field}");
        }
        assert!(value.get("work_id").is_none());
        assert_eq!(value["kind"], "lead");
        assert_eq!(value["sideEffect"], "read_only");
        assert_eq!(value["status"], "queued");
    }

    #[test]
    fn execution_inputs_and_summaries_use_minimal_camel_case_shapes() {
        let timestamp = Utc.with_ymd_and_hms(2026, 8, 15, 4, 0, 0).unwrap();
        let session = AgentSessionSummary {
            id: "agent-session-1".into(),
            work_id: "work-1".into(),
            agent_instance_id: "agent-1".into(),
            engine_kind: "codex".into(),
            generation: 2,
            engine_session_id: Some("engine-session-1".into()),
            current_assignment_id: Some("assignment-1".into()),
            last_successful_turn_id: Some("turn-1".into()),
            rotation_reason: Some("context_limit".into()),
            status: AgentSessionStatus::Running,
            created_at: timestamp,
            updated_at: timestamp,
            invalidated_at: None,
        };
        let queued = QueueWorkInput {
            instruction: "Investigate".into(),
            referenced_files: vec!["src/main.rs".into()],
            resource_ids: vec!["resource-1".into()],
        };
        let steer = SteerAssignmentInput {
            instruction: "Focus on the parser".into(),
        };
        let interrupt = InterruptWorkInput {
            assignment_id: "assignment-1".into(),
            replacement: queued.clone(),
        };
        let message = UserMessageSummary {
            id: "message-1".into(),
            work_id: "work-1".into(),
            assignment_id: Some("assignment-1".into()),
            run_id: None,
            role: crate::domain::work::MessageRole::User,
            content: "Investigate".into(),
            resource_ids: vec!["resource-1".into()],
            created_at: timestamp,
        };

        let session = serde_json::to_value(session).unwrap();
        assert_eq!(session["agentInstanceId"], "agent-1");
        assert_eq!(session["generation"], 2);
        assert_eq!(session["lastSuccessfulTurnId"], "turn-1");
        assert_eq!(session["rotationReason"], "context_limit");
        assert_eq!(
            serde_json::to_value(queued).unwrap()["referencedFiles"][0],
            "src/main.rs"
        );
        let steer = serde_json::to_value(steer).unwrap();
        assert_eq!(steer, json!({ "instruction": "Focus on the parser" }));
        assert_eq!(
            serde_json::to_value(interrupt).unwrap()["replacement"]["instruction"],
            "Investigate"
        );
        let message = serde_json::to_value(message).unwrap();
        assert_eq!(message["assignmentId"], "assignment-1");
        assert!(message.get("runId").is_none());
    }
}
