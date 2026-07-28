pub mod event;
pub mod work;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use ts_rs::TS;

    use super::{
        event::{WorkEventEnvelope, WorkEventPayload},
        work::{
            CreateWorkInput, MessageRole, MessageSummary, PermissionMode, RunStatus, RunSummary,
            StartWorkOutput, WorkDetail, WorkStatus, WorkSummary,
        },
    };

    #[test]
    fn export_bindings() {
        WorkStatus::export().unwrap();
        RunStatus::export().unwrap();
        MessageRole::export().unwrap();
        PermissionMode::export().unwrap();
        WorkSummary::export().unwrap();
        WorkDetail::export().unwrap();
        RunSummary::export().unwrap();
        MessageSummary::export().unwrap();
        StartWorkOutput::export().unwrap();
        CreateWorkInput::export().unwrap();
        WorkEventEnvelope::export().unwrap();
        WorkEventPayload::export().unwrap();

        let output_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/bindings");
        for type_name in [
            "WorkStatus",
            "RunStatus",
            "MessageRole",
            "PermissionMode",
            "WorkSummary",
            "WorkDetail",
            "RunSummary",
            "MessageSummary",
            "StartWorkOutput",
            "CreateWorkInput",
            "WorkEventEnvelope",
            "WorkEventPayload",
        ] {
            assert!(
                output_dir.join(format!("{type_name}.ts")).is_file(),
                "missing generated binding for {type_name}"
            );
        }

        let envelope = std::fs::read_to_string(output_dir.join("WorkEventEnvelope.ts")).unwrap();
        assert!(
            envelope.contains("workId: string")
                && envelope.contains("runId: string")
                && envelope.contains("sequence: number"),
            "event identifiers and sequence must be JSON-safe in TypeScript"
        );

        let work = std::fs::read_to_string(output_dir.join("WorkSummary.ts")).unwrap();
        assert!(work.contains("id: string"));

        let run = std::fs::read_to_string(output_dir.join("RunSummary.ts")).unwrap();
        assert!(run.contains("id: string") && run.contains("workId: string"));

        let permission = std::fs::read_to_string(output_dir.join("PermissionMode.ts")).unwrap();
        assert!(permission.contains("\"ask_every_step\" | \"balanced\" | \"auto_execute\""));
    }
}
