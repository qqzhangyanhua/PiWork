use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, de};
use ts_rs::TS;

macro_rules! binding_path {
    () => {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings/")
    };
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct WorkEventEnvelope {
    pub version: u32,
    // Canonical UUID string of the owning Work.
    pub work_id: String,
    // Canonical UUID string of the owning Run.
    pub run_id: String,
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
            work_id: String,
            run_id: String,
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
            work_id: wire.work_id,
            run_id: wire.run_id,
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
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    use super::{WorkEventEnvelope, WorkEventPayload};

    fn assert_string(_: &String) {}

    #[test]
    fn envelope_ids_round_trip_as_strings() {
        let envelope = WorkEventEnvelope {
            version: 1,
            work_id: "10000000-0000-0000-0000-000000000000".into(),
            run_id: "20000000-0000-0000-0000-000000000000".into(),
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
