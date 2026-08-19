use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

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

wire_enum!(ResultStatus {
    Completed,
    Failed,
    NeedsClarification,
});

wire_enum!(MemoryCandidateStatus {
    Proposed,
    Confirmed,
    Rejected,
});

wire_enum!(LedgerPlanStepStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct ResultFinding {
    pub title: String,
    pub detail: String,
    pub confidence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_event_id: Option<String>,
    pub author_agent_id: String,
    pub assignment_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub occurred_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct ResultEvidence {
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_resource_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_path: Option<String>,
    pub author_agent_id: String,
    pub assignment_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub occurred_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct ResultArtifact {
    pub path: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub produced_by_command: Option<String>,
    pub author_agent_id: String,
    pub assignment_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct ResultValidation {
    pub command: String,
    pub success: bool,
    pub summary: String,
    pub author_agent_id: String,
    pub assignment_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub occurred_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct DelegationRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub suggested_agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub capability_pack_id: Option<String>,
    pub reason: String,
    pub context_needed: String,
    pub expected_output: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct MemoryCandidateInput {
    pub content: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct ResultEnvelope {
    pub status: ResultStatus,
    pub summary: String,
    pub findings: Vec<ResultFinding>,
    pub evidence: Vec<ResultEvidence>,
    pub artifacts: Vec<ResultArtifact>,
    pub validation: Vec<ResultValidation>,
    pub decisions_recommended: Vec<String>,
    pub uncertainties: Vec<String>,
    pub delegation_requests: Vec<DelegationRequest>,
    pub memory_candidates: Vec<MemoryCandidateInput>,
    pub limitations: Vec<String>,
    #[ts(type = "Record<string, unknown>")]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct LedgerPlanStep {
    pub id: String,
    pub title: String,
    pub status: LedgerPlanStepStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct LedgerDecision {
    pub id: String,
    pub summary: String,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct WorkLedger {
    pub goal: String,
    pub plan: Vec<LedgerPlanStep>,
    pub decisions: Vec<LedgerDecision>,
    pub constraints: Vec<String>,
    pub permissions: Vec<String>,
    pub active_assignments: Vec<String>,
    pub waiting_assignments: Vec<String>,
    pub completed_assignments: Vec<String>,
    pub artifacts: Vec<String>,
    pub validation: Vec<String>,
    pub open_questions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_delivery: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct MemoryCandidateSummary {
    pub id: String,
    pub source_work_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_event_id: Option<String>,
    pub author_agent_id: String,
    pub content: String,
    pub reason: String,
    pub version: u32,
    pub status: MemoryCandidateStatus,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resolved_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resolved_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct DelegateAssignmentInput {
    pub assigned_agent_id: String,
    pub capability_pack_id: Option<String>,
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
    pub max_attempts: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct RecordWorkDecisionInput {
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct UpdateWorkPlanInput {
    pub plan: Vec<LedgerPlanStep>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct CompleteWorkDeliveryInput {
    pub summary: String,
    pub artifacts: Vec<String>,
    pub validation: Vec<String>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct SubmitAssignmentResultInput {
    #[ts(type = "unknown")]
    pub envelope: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct RequestClarificationInput {
    pub question: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "tool",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(
    tag = "tool",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    export_to = binding_path!()
)]
#[allow(clippy::large_enum_variant)] // wire DTO, not a hot-loop type
pub enum HostToolCall {
    ListWorkMembers,
    InspectCapabilityPacks { ids: Vec<String> },
    DelegateAssignment(DelegateAssignmentInput),
    GetAssignmentStatus { assignment_ids: Vec<String> },
    CancelAssignment { assignment_id: String },
    RequestAssignmentRetry { assignment_id: String },
    RecordWorkDecision(RecordWorkDecisionInput),
    UpdateWorkPlan(UpdateWorkPlanInput),
    CompleteWorkDelivery(CompleteWorkDeliveryInput),
    SubmitAssignmentResult(SubmitAssignmentResultInput),
    RequestClarification(RequestClarificationInput),
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn valid_envelope() -> ResultEnvelope {
        ResultEnvelope {
            status: ResultStatus::Completed,
            summary: "Investigated the queue state machine".into(),
            findings: vec![ResultFinding {
                title: "Eight portable invariants".into(),
                detail: "Derived from the Buzz queue".into(),
                confidence: Some("high".into()),
                source_event_id: Some("event-1".into()),
                author_agent_id: "agent-1".into(),
                assignment_id: "assignment-1".into(),
                occurred_at: None,
            }],
            evidence: vec![ResultEvidence {
                description: "Source inspection".into(),
                source_event_id: Some("event-1".into()),
                source_resource_id: None,
                source_path: Some("src/assignment/queue.rs".into()),
                author_agent_id: "agent-1".into(),
                assignment_id: "assignment-1".into(),
                occurred_at: None,
            }],
            artifacts: vec![ResultArtifact {
                path: "notes.md".into(),
                description: "Research notes".into(),
                produced_by_command: None,
                author_agent_id: "agent-1".into(),
                assignment_id: "assignment-1".into(),
            }],
            validation: vec![ResultValidation {
                command: "cargo test".into(),
                success: true,
                summary: "passed".into(),
                author_agent_id: "agent-1".into(),
                assignment_id: "assignment-1".into(),
                occurred_at: None,
            }],
            decisions_recommended: vec!["Adopt the queue".into()],
            uncertainties: vec!["Recovery timing".into()],
            delegation_requests: vec![],
            memory_candidates: vec![],
            limitations: vec!["Static analysis only".into()],
            extensions: BTreeMap::new(),
        }
    }

    #[test]
    fn result_envelope_round_trips_with_stable_wire_shape() {
        let envelope = valid_envelope();
        let value = serde_json::to_value(&envelope).unwrap();
        assert_eq!(value["status"], "completed");
        assert_eq!(value["summary"], "Investigated the queue state machine");
        assert_eq!(value["findings"][0]["authorAgentId"], "agent-1");
        assert_eq!(
            value["evidence"][0]["sourcePath"],
            "src/assignment/queue.rs"
        );
        assert_eq!(value["artifacts"][0]["path"], "notes.md");
        assert_eq!(value["validation"][0]["command"], "cargo test");
        assert_eq!(value["extensions"], json!({}));
        let round_trip: ResultEnvelope = serde_json::from_value(value).unwrap();
        assert_eq!(round_trip.status, ResultStatus::Completed);
        assert_eq!(round_trip.findings.len(), 1);
    }

    #[test]
    fn host_tool_call_is_a_strictly_tagged_union() {
        let call = HostToolCall::DelegateAssignment(DelegateAssignmentInput {
            assigned_agent_id: "agent-2".into(),
            capability_pack_id: None,
            title: "Review".into(),
            instruction: "Review the diff".into(),
            context_manifest: json!({}),
            expected_result_schema: json!({}),
            acceptance_criteria: json!([]),
            permission_scope: json!({}),
            priority: 10,
            max_attempts: 1,
        });
        let value = serde_json::to_value(&call).unwrap();
        assert_eq!(value["tool"], "delegateAssignment");
        assert_eq!(value["assignedAgentId"], "agent-2");
        let round_trip: HostToolCall = serde_json::from_value(value).unwrap();
        match round_trip {
            HostToolCall::DelegateAssignment(input) => {
                assert_eq!(input.assigned_agent_id, "agent-2")
            }
            _ => panic!("expected delegateAssignment"),
        }
    }
}
