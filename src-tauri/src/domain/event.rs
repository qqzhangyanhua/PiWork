use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

macro_rules! binding_path {
    () => {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings/")
    };
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct WorkEventEnvelope {
    pub version: u32,
    pub work_id: Uuid,
    pub run_id: Uuid,
    pub sequence: u32,
    pub occurred_at: DateTime<Utc>,
    pub payload: WorkEventPayload,
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
    use serde_json::json;

    use super::WorkEventPayload;

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
