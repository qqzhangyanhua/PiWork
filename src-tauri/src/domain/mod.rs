pub mod event;
pub mod work;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use ts_rs::TS;

    use super::{
        event::{WorkEventEnvelope, WorkEventPayload},
        work::{
            CreateWorkInput, PermissionMode, RunStatus, RunSummary, WorkDetail, WorkStatus,
            WorkSummary,
        },
    };

    #[test]
    fn export_bindings() {
        WorkStatus::export().unwrap();
        RunStatus::export().unwrap();
        PermissionMode::export().unwrap();
        WorkSummary::export().unwrap();
        WorkDetail::export().unwrap();
        RunSummary::export().unwrap();
        CreateWorkInput::export().unwrap();
        WorkEventEnvelope::export().unwrap();
        WorkEventPayload::export().unwrap();

        let output_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/bindings");
        for type_name in [
            "WorkStatus",
            "RunStatus",
            "PermissionMode",
            "WorkSummary",
            "WorkDetail",
            "RunSummary",
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
            envelope.contains("sequence: number"),
            "event sequence must be JSON-safe in TypeScript"
        );
    }
}
