use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, de};
use ts_rs::TS;

use super::assignment::QueueControlMode;

macro_rules! binding_path {
    () => {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings/")
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case", export_to = binding_path!())]
pub enum PermissionOutcome {
    AllowedOnce,
    AllowedForRun,
    Denied,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case", export_to = binding_path!())]
pub enum SessionTransition {
    Created,
    Resumed,
    Rotated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case", export_to = binding_path!())]
pub enum LivenessState {
    Alive,
    Stalled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct WorkEventEnvelope {
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub event_id: Option<String>,
    // Canonical UUID string of the owning Work.
    pub work_id: String,
    // Canonical UUID string of the owning Run.
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub assignment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub causation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub correlation_id: Option<String>,
    pub sequence: u32,
    pub occurred_at: DateTime<Utc>,
    pub payload: WorkEventPayload,
}

impl<'de> Deserialize<'de> for WorkEventEnvelope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct WireEnvelope {
            version: u32,
            #[serde(default)]
            event_id: Option<String>,
            work_id: String,
            run_id: String,
            #[serde(default)]
            turn_id: Option<String>,
            #[serde(default)]
            session_id: Option<String>,
            #[serde(default)]
            agent_id: Option<String>,
            #[serde(default)]
            assignment_id: Option<String>,
            #[serde(default)]
            causation_id: Option<String>,
            #[serde(default)]
            correlation_id: Option<String>,
            sequence: u32,
            occurred_at: DateTime<Utc>,
            payload: WorkEventPayload,
        }

        let wire = WireEnvelope::deserialize(deserializer)?;
        if wire.sequence == 0 {
            return Err(de::Error::custom("event sequence must be at least 1"));
        }

        Ok(Self {
            version: wire.version,
            event_id: wire.event_id,
            work_id: wire.work_id,
            run_id: wire.run_id,
            turn_id: wire.turn_id,
            session_id: wire.session_id,
            agent_id: wire.agent_id,
            assignment_id: wire.assignment_id,
            causation_id: wire.causation_id,
            correlation_id: wire.correlation_id,
            sequence: wire.sequence,
            occurred_at: wire.occurred_at,
            payload: wire.payload,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    export_to = binding_path!()
)]
pub enum WorkEventPayload {
    RunStarted {
        model_label: String,
    },
    AssistantDelta {
        text: String,
    },
    ToolStarted {
        tool_call_id: String,
        tool_name: String,
        input_summary: String,
    },
    ToolFinished {
        tool_call_id: String,
        tool_name: String,
        output_summary: String,
        success: bool,
    },
    RunCompleted {
        summary: String,
        artifacts: Vec<String>,
        validation: Vec<String>,
        limitations: Vec<String>,
    },
    RunFailed {
        message: String,
    },
    ThoughtDelta {
        text: String,
    },
    PlanChanged {
        plan_id: String,
        revision: u32,
        text: String,
    },
    ToolPending {
        tool_call_id: String,
        tool_name: String,
        input_summary: String,
    },
    ToolProgress {
        tool_call_id: String,
        tool_name: String,
        output_summary: String,
    },
    PermissionRequested {
        request_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        tool_call_id: Option<String>,
        title: String,
        detail: String,
    },
    PermissionResolved {
        request_id: String,
        outcome: PermissionOutcome,
    },
    Waiting {
        reason: String,
    },
    Liveness {
        state: LivenessState,
    },
    SessionChanged {
        transition: SessionTransition,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        reason: Option<String>,
    },
    ArtifactProduced {
        path: String,
    },
    ValidationProduced {
        command: String,
        success: bool,
        summary: String,
    },
    UsageUpdated {
        input_tokens: u32,
        output_tokens: u32,
        cache_read_tokens: u32,
        cache_write_tokens: u32,
        total_tokens: u32,
    },
    RawEngineEvent {
        kind: String,
        payload_json: String,
    },
    AssignmentQueued {
        assignment_id: String,
        assigned_agent_id: String,
        title: String,
        priority: u32,
    },
    AssignmentClaimed {
        assignment_id: String,
        agent_instance_id: String,
        agent_session_id: String,
    },
    AssignmentStarted {
        assignment_id: String,
        agent_instance_id: String,
        agent_session_id: String,
        run_id: String,
    },
    AssignmentWaiting {
        assignment_id: String,
        agent_instance_id: String,
        agent_session_id: String,
        reason: String,
    },
    AssignmentRetryScheduled {
        assignment_id: String,
        agent_instance_id: String,
        attempt_count: u32,
        next_attempt_at: DateTime<Utc>,
        reason: String,
    },
    AssignmentCompleted {
        assignment_id: String,
        agent_instance_id: String,
        agent_session_id: String,
        result_summary: String,
    },
    AssignmentFailed {
        assignment_id: String,
        agent_instance_id: String,
        agent_session_id: String,
        error: String,
    },
    AssignmentInterrupted {
        assignment_id: String,
        agent_instance_id: String,
        agent_session_id: Option<String>,
        reason: String,
    },
    AssignmentDeadLettered {
        assignment_id: String,
        agent_instance_id: String,
        attempt_count: u32,
        error: String,
    },
    AssignmentRecoveryRequired {
        assignment_id: String,
        agent_instance_id: String,
        recovery_reason: String,
    },
    QueueControlApplied {
        mode: QueueControlMode,
        assignment_id: String,
        replaced_assignment_id: Option<String>,
        summary: String,
    },
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use chrono::{TimeZone, Utc};
    use serde::{Serialize, de::DeserializeOwned};
    use serde_json::json;

    use super::{
        LivenessState, PermissionOutcome, SessionTransition, WorkEventEnvelope, WorkEventPayload,
    };

    fn assert_string(_: &String) {}

    fn assert_copy<T: Copy>() {}

    fn assert_wire_values<T>(cases: &[(T, &str)])
    where
        T: Copy + Debug + PartialEq + Serialize + DeserializeOwned,
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
    fn envelope_ids_round_trip_as_strings() {
        let envelope = WorkEventEnvelope {
            version: 1,
            event_id: None,
            work_id: "10000000-0000-0000-0000-000000000000".into(),
            run_id: "20000000-0000-0000-0000-000000000000".into(),
            turn_id: None,
            session_id: None,
            agent_id: None,
            assignment_id: None,
            causation_id: None,
            correlation_id: None,
            sequence: 1,
            occurred_at: Utc.with_ymd_and_hms(2026, 7, 28, 7, 0, 0).unwrap(),
            payload: WorkEventPayload::AssistantDelta {
                text: "hello".into(),
            },
        };

        let serialized = serde_json::to_string(&envelope).unwrap();
        let round_trip: WorkEventEnvelope = serde_json::from_str(&serialized).unwrap();

        assert_string(&round_trip.work_id);
        assert_string(&round_trip.run_id);
        assert_eq!(round_trip, envelope);
    }

    #[test]
    fn v2_envelope_round_trips_all_activity_identity() {
        let envelope = WorkEventEnvelope {
            version: 2,
            event_id: Some("event-1".into()),
            work_id: "work-1".into(),
            run_id: "run-1".into(),
            turn_id: Some("turn-1".into()),
            session_id: Some("session-1".into()),
            agent_id: Some("agent-1".into()),
            assignment_id: Some("assignment-1".into()),
            causation_id: Some("event-0".into()),
            correlation_id: Some("correlation-1".into()),
            sequence: 2,
            occurred_at: Utc.with_ymd_and_hms(2026, 8, 9, 7, 0, 0).unwrap(),
            payload: WorkEventPayload::ThoughtDelta {
                text: "checking".into(),
            },
        };

        let serialized = serde_json::to_value(&envelope).unwrap();
        let round_trip: WorkEventEnvelope = serde_json::from_value(serialized.clone()).unwrap();

        assert_eq!(serialized["eventId"], "event-1");
        assert_eq!(serialized["turnId"], "turn-1");
        assert_eq!(serialized["sessionId"], "session-1");
        assert_eq!(serialized["agentId"], "agent-1");
        assert_eq!(serialized["assignmentId"], "assignment-1");
        assert_eq!(serialized["causationId"], "event-0");
        assert_eq!(serialized["correlationId"], "correlation-1");
        assert_eq!(serialized["payload"]["type"], "thoughtDelta");
        assert_eq!(round_trip, envelope);
    }

    #[test]
    fn v1_envelope_without_activity_identity_still_deserializes() {
        let serialized = json!({
            "version": 1,
            "workId": "work-1",
            "runId": "run-1",
            "sequence": 1,
            "occurredAt": "2026-08-09T07:00:00Z",
            "payload": { "type": "assistantDelta", "text": "hello" }
        });

        let envelope: WorkEventEnvelope = serde_json::from_value(serialized).unwrap();

        assert_eq!(
            [
                envelope.event_id,
                envelope.turn_id,
                envelope.session_id,
                envelope.agent_id,
                envelope.assignment_id,
                envelope.causation_id,
                envelope.correlation_id,
            ],
            [None, None, None, None, None, None, None]
        );
    }

    #[test]
    fn activity_wire_enums_are_copy() {
        assert_copy::<PermissionOutcome>();
        assert_copy::<SessionTransition>();
        assert_copy::<LivenessState>();
    }

    #[test]
    fn activity_wire_enums_round_trip_every_snake_case_value() {
        assert_wire_values(&[
            (PermissionOutcome::AllowedOnce, "allowed_once"),
            (PermissionOutcome::AllowedForRun, "allowed_for_run"),
            (PermissionOutcome::Denied, "denied"),
            (PermissionOutcome::Cancelled, "cancelled"),
        ]);
        assert_wire_values(&[
            (LivenessState::Alive, "alive"),
            (LivenessState::Stalled, "stalled"),
        ]);
        assert_wire_values(&[
            (SessionTransition::Created, "created"),
            (SessionTransition::Resumed, "resumed"),
            (SessionTransition::Rotated, "rotated"),
        ]);
    }

    #[test]
    fn v2_payloads_round_trip_with_stable_wire_shapes() {
        let cases: Vec<(WorkEventPayload, &str, &[&str])> = vec![
            (
                WorkEventPayload::ThoughtDelta {
                    text: "thinking".into(),
                },
                "thoughtDelta",
                &["text"],
            ),
            (
                WorkEventPayload::PlanChanged {
                    plan_id: "plan-1".into(),
                    revision: 2,
                    text: "updated plan".into(),
                },
                "planChanged",
                &["planId", "revision", "text"],
            ),
            (
                WorkEventPayload::ToolPending {
                    tool_call_id: "call-pending".into(),
                    tool_name: "shell".into(),
                    input_summary: "cargo test".into(),
                },
                "toolPending",
                &["toolCallId", "toolName", "inputSummary"],
            ),
            (
                WorkEventPayload::ToolProgress {
                    tool_call_id: "call-progress".into(),
                    tool_name: "shell".into(),
                    output_summary: "compiling".into(),
                },
                "toolProgress",
                &["toolCallId", "toolName", "outputSummary"],
            ),
            (
                WorkEventPayload::PermissionRequested {
                    request_id: "request-1".into(),
                    tool_call_id: Some("call-permission".into()),
                    title: "Run command".into(),
                    detail: "cargo test".into(),
                },
                "permissionRequested",
                &["requestId", "toolCallId", "title", "detail"],
            ),
            (
                WorkEventPayload::PermissionResolved {
                    request_id: "request-1".into(),
                    outcome: PermissionOutcome::AllowedForRun,
                },
                "permissionResolved",
                &["requestId", "outcome"],
            ),
            (
                WorkEventPayload::Waiting {
                    reason: "approval".into(),
                },
                "waiting",
                &["reason"],
            ),
            (
                WorkEventPayload::Liveness {
                    state: LivenessState::Stalled,
                },
                "liveness",
                &["state"],
            ),
            (
                WorkEventPayload::SessionChanged {
                    transition: SessionTransition::Rotated,
                    reason: Some("expired".into()),
                },
                "sessionChanged",
                &["transition", "reason"],
            ),
            (
                WorkEventPayload::ArtifactProduced {
                    path: "artifact.txt".into(),
                },
                "artifactProduced",
                &["path"],
            ),
            (
                WorkEventPayload::ValidationProduced {
                    command: "cargo test".into(),
                    success: true,
                    summary: "passed".into(),
                },
                "validationProduced",
                &["command", "success", "summary"],
            ),
            (
                WorkEventPayload::UsageUpdated {
                    input_tokens: 1,
                    output_tokens: 2,
                    cache_read_tokens: 3,
                    cache_write_tokens: 4,
                    total_tokens: 10,
                },
                "usageUpdated",
                &[
                    "inputTokens",
                    "outputTokens",
                    "cacheReadTokens",
                    "cacheWriteTokens",
                    "totalTokens",
                ],
            ),
            (
                WorkEventPayload::RawEngineEvent {
                    kind: "engine_kind".into(),
                    payload_json: r#"{"raw":true}"#.into(),
                },
                "rawEngineEvent",
                &["kind", "payloadJson"],
            ),
        ];

        for (payload, discriminator, fields) in cases {
            let serialized = serde_json::to_value(&payload).unwrap();
            let object = serialized.as_object().unwrap();

            assert_eq!(serialized["type"], discriminator, "{payload:?}");
            assert_eq!(object.len(), fields.len() + 1, "{payload:?}");
            for &field in fields {
                assert!(object.contains_key(field), "missing {field} in {payload:?}");
            }
            match &payload {
                WorkEventPayload::PermissionRequested { .. } => {
                    assert_eq!(serialized["toolCallId"], "call-permission");
                }
                WorkEventPayload::PermissionResolved { .. } => {
                    assert_eq!(serialized["outcome"], "allowed_for_run");
                }
                WorkEventPayload::Liveness { .. } => {
                    assert_eq!(serialized["state"], "stalled");
                }
                WorkEventPayload::SessionChanged { .. } => {
                    assert_eq!(serialized["transition"], "rotated");
                }
                _ => {}
            }

            let round_trip: WorkEventPayload = serde_json::from_value(serialized).unwrap();
            assert_eq!(round_trip, payload);
        }
    }

    #[test]
    fn assignment_payloads_use_product_discriminators_without_scheduler_internals() {
        let retry_at = Utc.with_ymd_and_hms(2026, 8, 15, 4, 5, 0).unwrap();
        let cases = vec![
            WorkEventPayload::AssignmentQueued {
                assignment_id: "assignment-1".into(),
                assigned_agent_id: "agent-1".into(),
                title: "Investigate".into(),
                priority: 10,
            },
            WorkEventPayload::AssignmentClaimed {
                assignment_id: "assignment-1".into(),
                agent_instance_id: "agent-1".into(),
                agent_session_id: "session-1".into(),
            },
            WorkEventPayload::AssignmentStarted {
                assignment_id: "assignment-1".into(),
                agent_instance_id: "agent-1".into(),
                agent_session_id: "session-1".into(),
                run_id: "run-1".into(),
            },
            WorkEventPayload::AssignmentWaiting {
                assignment_id: "assignment-1".into(),
                agent_instance_id: "agent-1".into(),
                agent_session_id: "session-1".into(),
                reason: "approval".into(),
            },
            WorkEventPayload::AssignmentRetryScheduled {
                assignment_id: "assignment-1".into(),
                agent_instance_id: "agent-1".into(),
                attempt_count: 2,
                next_attempt_at: retry_at,
                reason: "transient failure".into(),
            },
            WorkEventPayload::AssignmentCompleted {
                assignment_id: "assignment-1".into(),
                agent_instance_id: "agent-1".into(),
                agent_session_id: "session-1".into(),
                result_summary: "done".into(),
            },
            WorkEventPayload::AssignmentFailed {
                assignment_id: "assignment-1".into(),
                agent_instance_id: "agent-1".into(),
                agent_session_id: "session-1".into(),
                error: "failed".into(),
            },
            WorkEventPayload::AssignmentInterrupted {
                assignment_id: "assignment-1".into(),
                agent_instance_id: "agent-1".into(),
                agent_session_id: Some("session-1".into()),
                reason: "replaced".into(),
            },
            WorkEventPayload::AssignmentDeadLettered {
                assignment_id: "assignment-1".into(),
                agent_instance_id: "agent-1".into(),
                attempt_count: 3,
                error: "exhausted".into(),
            },
            WorkEventPayload::AssignmentRecoveryRequired {
                assignment_id: "assignment-1".into(),
                agent_instance_id: "agent-1".into(),
                recovery_reason: "unknown side effect".into(),
            },
            WorkEventPayload::QueueControlApplied {
                mode: crate::domain::assignment::QueueControlMode::InterruptAndReplace,
                assignment_id: "assignment-2".into(),
                replaced_assignment_id: Some("assignment-1".into()),
                summary: "replacement queued".into(),
            },
        ];
        let expected = [
            "assignmentQueued",
            "assignmentClaimed",
            "assignmentStarted",
            "assignmentWaiting",
            "assignmentRetryScheduled",
            "assignmentCompleted",
            "assignmentFailed",
            "assignmentInterrupted",
            "assignmentDeadLettered",
            "assignmentRecoveryRequired",
            "queueControlApplied",
        ];

        for (payload, expected_discriminator) in cases.into_iter().zip(expected) {
            let value = serde_json::to_value(&payload).unwrap();
            assert_eq!(value["type"], expected_discriminator);
            let json = serde_json::to_string(&value).unwrap();
            assert!(!json.contains("slot"));
            assert!(!json.contains("permit"));
            assert_eq!(
                serde_json::from_value::<WorkEventPayload>(value).unwrap(),
                payload
            );
        }
    }

    #[test]
    fn optional_payload_fields_are_omitted_and_round_trip_as_none() {
        let cases = [
            (
                WorkEventPayload::PermissionRequested {
                    request_id: "request-1".into(),
                    tool_call_id: None,
                    title: "Run command".into(),
                    detail: "cargo test".into(),
                },
                "toolCallId",
            ),
            (
                WorkEventPayload::SessionChanged {
                    transition: SessionTransition::Created,
                    reason: None,
                },
                "reason",
            ),
        ];

        for (payload, omitted_field) in cases {
            let serialized = serde_json::to_value(&payload).unwrap();
            assert!(serialized.get(omitted_field).is_none());

            let round_trip: WorkEventPayload = serde_json::from_value(serialized).unwrap();
            assert_eq!(round_trip, payload);
        }
    }

    #[test]
    fn envelope_rejects_zero_sequence_during_deserialization() {
        let serialized = json!({
            "version": 1,
            "workId": "10000000-0000-0000-0000-000000000000",
            "runId": "20000000-0000-0000-0000-000000000000",
            "sequence": 0,
            "occurredAt": "2026-07-28T07:00:00Z",
            "payload": { "type": "assistantDelta", "text": "hello" }
        });

        assert!(serde_json::from_value::<WorkEventEnvelope>(serialized).is_err());
    }

    #[test]
    fn envelope_accepts_first_sequence_during_deserialization() {
        let serialized = json!({
            "version": 1,
            "workId": "10000000-0000-0000-0000-000000000000",
            "runId": "20000000-0000-0000-0000-000000000000",
            "sequence": 1,
            "occurredAt": "2026-07-28T07:00:00Z",
            "payload": { "type": "assistantDelta", "text": "hello" }
        });

        let envelope: WorkEventEnvelope = serde_json::from_value(serialized).unwrap();
        assert_eq!(envelope.sequence, 1);
    }

    #[test]
    fn assistant_delta_has_a_camel_case_discriminator() {
        let payload = WorkEventPayload::AssistantDelta {
            text: "hello".into(),
        };

        assert_eq!(
            serde_json::to_value(payload).unwrap(),
            json!({ "type": "assistantDelta", "text": "hello" })
        );
    }

    #[test]
    fn completed_event_carries_the_stable_result_contract() {
        let payload = WorkEventPayload::RunCompleted {
            summary: "Implemented the feature".into(),
            artifacts: vec!["src/main.rs".into()],
            validation: vec!["cargo test".into()],
            limitations: vec!["fake provider".into()],
        };

        assert_eq!(
            serde_json::to_value(payload).unwrap(),
            json!({
                "type": "runCompleted",
                "summary": "Implemented the feature",
                "artifacts": ["src/main.rs"],
                "validation": ["cargo test"],
                "limitations": ["fake provider"]
            })
        );
    }

    #[test]
    fn tool_fields_are_camel_case() {
        let payload = WorkEventPayload::ToolFinished {
            tool_call_id: "call-1".into(),
            tool_name: "shell".into(),
            output_summary: "tests passed".into(),
            success: true,
        };

        assert_eq!(
            serde_json::to_value(payload).unwrap(),
            json!({
                "type": "toolFinished",
                "toolCallId": "call-1",
                "toolName": "shell",
                "outputSummary": "tests passed",
                "success": true
            })
        );
    }
}
