pub mod agent;
pub mod assignment;
pub mod environment;
pub mod event;
pub mod resource;
pub mod work;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use ts_rs::TS;

    use super::{
        agent::{
            AgentDefinitionSummary, AgentInstanceSummary, AgentStatus, AssemblyDiagnostic,
            AssemblyDiagnosticCode, CapabilityPackStatus, CapabilityPackSummary, MemoryPolicy,
            PermissionPolicy, RoleKind, RoleTemplateSummary, SaveAgentAssemblyInput,
            WorkAgentStatus, WorkAgentSummary, WorkTeamSummary,
        },
        assignment::{
            AgentSessionStatus, AgentSessionSummary, AssignmentKind, AssignmentSideEffect,
            AssignmentStatus, AssignmentSummary, InterruptWorkInput, QueueControlMode,
            QueueWorkInput, SteerAssignmentInput, UserMessageSummary,
        },
        environment::{RuntimeCheck, RuntimeStatus},
        event::{
            LivenessState, PermissionOutcome, SessionTransition, WorkEventEnvelope,
            WorkEventPayload,
        },
        resource::{
            ImportResourcesInput, ResourceOrigin, ResourceStatus, ResourceSummary,
            ResourceThumbnail,
        },
        work::{
            CreateWorkInput, MessageRole, MessageSummary, PermissionMode, RunStatus, RunSummary,
            StartWorkOutput, WorkDetail, WorkStatus, WorkSummary,
        },
    };

    #[test]
    fn assignment_contract_types_are_exportable() {
        fn assert_exportable<T: TS>() {}

        assert_exportable::<AssignmentStatus>();
        assert_exportable::<AssignmentKind>();
        assert_exportable::<AssignmentSideEffect>();
        assert_exportable::<AgentSessionStatus>();
        assert_exportable::<QueueControlMode>();
        assert_exportable::<AssignmentSummary>();
        assert_exportable::<AgentSessionSummary>();
        assert_exportable::<QueueWorkInput>();
        assert_exportable::<SteerAssignmentInput>();
        assert_exportable::<InterruptWorkInput>();
        assert_exportable::<UserMessageSummary>();
    }

    fn bindings_semantically_equal(before: &str, after: &str) -> bool {
        before.replace("\r\n", "\n") == after.replace("\r\n", "\n")
    }

    const AGENT_BINDING_TYPE_NAMES: [&str; 15] = [
        "RoleKind",
        "AgentStatus",
        "CapabilityPackStatus",
        "WorkAgentStatus",
        "PermissionPolicy",
        "MemoryPolicy",
        "AssemblyDiagnosticCode",
        "RoleTemplateSummary",
        "AgentDefinitionSummary",
        "AgentInstanceSummary",
        "CapabilityPackSummary",
        "WorkAgentSummary",
        "WorkTeamSummary",
        "AssemblyDiagnostic",
        "SaveAgentAssemblyInput",
    ];

    #[test]
    fn export_bindings() {
        let output_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/bindings");
        let agent_bindings_before = AGENT_BINDING_TYPE_NAMES.map(|type_name| {
            let binding =
                std::fs::read_to_string(output_dir.join(format!("{type_name}.ts"))).unwrap();
            (type_name, binding)
        });

        RoleKind::export().unwrap();
        AgentStatus::export().unwrap();
        CapabilityPackStatus::export().unwrap();
        WorkAgentStatus::export().unwrap();
        PermissionPolicy::export().unwrap();
        MemoryPolicy::export().unwrap();
        AssemblyDiagnosticCode::export().unwrap();
        RoleTemplateSummary::export().unwrap();
        AgentDefinitionSummary::export().unwrap();
        AgentInstanceSummary::export().unwrap();
        CapabilityPackSummary::export().unwrap();
        WorkAgentSummary::export().unwrap();
        WorkTeamSummary::export().unwrap();
        AssemblyDiagnostic::export().unwrap();
        SaveAgentAssemblyInput::export().unwrap();
        AssignmentStatus::export().unwrap();
        AssignmentKind::export().unwrap();
        AssignmentSideEffect::export().unwrap();
        AgentSessionStatus::export().unwrap();
        QueueControlMode::export().unwrap();
        AssignmentSummary::export().unwrap();
        AgentSessionSummary::export().unwrap();
        QueueWorkInput::export().unwrap();
        SteerAssignmentInput::export().unwrap();
        InterruptWorkInput::export().unwrap();
        UserMessageSummary::export().unwrap();
        RuntimeCheck::export().unwrap();
        RuntimeStatus::export().unwrap();
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
        ResourceStatus::export().unwrap();
        ResourceOrigin::export().unwrap();
        ResourceSummary::export().unwrap();
        ResourceThumbnail::export().unwrap();
        ImportResourcesInput::export().unwrap();
        PermissionOutcome::export().unwrap();
        SessionTransition::export().unwrap();
        LivenessState::export().unwrap();
        WorkEventEnvelope::export().unwrap();
        WorkEventPayload::export().unwrap();

        for (type_name, before) in agent_bindings_before {
            let after =
                std::fs::read_to_string(output_dir.join(format!("{type_name}.ts"))).unwrap();
            assert!(
                bindings_semantically_equal(&before, &after),
                "generated Agent binding drifted: {type_name}"
            );
        }

        for type_name in [
            "RoleKind",
            "AgentStatus",
            "CapabilityPackStatus",
            "WorkAgentStatus",
            "PermissionPolicy",
            "MemoryPolicy",
            "AssemblyDiagnosticCode",
            "RoleTemplateSummary",
            "AgentDefinitionSummary",
            "AgentInstanceSummary",
            "CapabilityPackSummary",
            "WorkAgentSummary",
            "WorkTeamSummary",
            "AssemblyDiagnostic",
            "SaveAgentAssemblyInput",
            "AssignmentStatus",
            "AssignmentKind",
            "AssignmentSideEffect",
            "AgentSessionStatus",
            "QueueControlMode",
            "AssignmentSummary",
            "AgentSessionSummary",
            "QueueWorkInput",
            "SteerAssignmentInput",
            "InterruptWorkInput",
            "UserMessageSummary",
            "RuntimeCheck",
            "RuntimeStatus",
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
            "ResourceStatus",
            "ResourceOrigin",
            "ResourceSummary",
            "ResourceThumbnail",
            "ImportResourcesInput",
            "WorkEventEnvelope",
            "WorkEventPayload",
            "PermissionOutcome",
            "SessionTransition",
            "LivenessState",
        ] {
            assert!(
                output_dir.join(format!("{type_name}.ts")).is_file(),
                "missing generated binding for {type_name}"
            );
        }

        for (type_name, number_fields) in [
            ("RoleTemplateSummary", &["version: number"][..]),
            (
                "AgentDefinitionSummary",
                &["defaultParallelism: number", "version: number"][..],
            ),
            (
                "AgentInstanceSummary",
                &["parallelismOverride: number | null"][..],
            ),
            ("CapabilityPackSummary", &["version: number"][..]),
            (
                "SaveAgentAssemblyInput",
                &["parallelismOverride: number | null"][..],
            ),
        ] {
            let binding =
                std::fs::read_to_string(output_dir.join(format!("{type_name}.ts"))).unwrap();
            assert!(
                !binding.contains("bigint"),
                "{type_name} must use JSON-transportable number fields"
            );
            for field in number_fields {
                assert!(binding.contains(field), "{type_name} is missing {field}");
            }
        }

        let binding_index = std::fs::read_to_string(output_dir.join("index.ts")).unwrap();
        for type_name in AGENT_BINDING_TYPE_NAMES {
            assert!(
                binding_index.contains(&format!(
                    "export type {{ {type_name} }} from \"./{type_name}\";"
                )),
                "binding index is missing {type_name}"
            );
        }
        for type_name in [
            "AssignmentStatus",
            "AssignmentKind",
            "AssignmentSideEffect",
            "AgentSessionStatus",
            "QueueControlMode",
            "AssignmentSummary",
            "AgentSessionSummary",
            "QueueWorkInput",
            "SteerAssignmentInput",
            "InterruptWorkInput",
            "UserMessageSummary",
        ] {
            assert!(
                binding_index.contains(&format!(
                    "export type {{ {type_name} }} from \"./{type_name}\";"
                )),
                "binding index is missing {type_name}"
            );
        }

        let envelope = std::fs::read_to_string(output_dir.join("WorkEventEnvelope.ts")).unwrap();
        assert!(
            envelope.contains("workId: string")
                && envelope.contains("runId: string")
                && envelope.contains("sequence: number")
                && envelope.contains("eventId?: string")
                && envelope.contains("turnId?: string")
                && envelope.contains("sessionId?: string")
                && envelope.contains("correlationId?: string"),
            "event identifiers and sequence must be JSON-safe in TypeScript"
        );

        let payload = std::fs::read_to_string(output_dir.join("WorkEventPayload.ts")).unwrap();
        for discriminator in [
            "thoughtDelta",
            "planChanged",
            "toolPending",
            "toolProgress",
            "permissionRequested",
            "permissionResolved",
            "waiting",
            "liveness",
            "sessionChanged",
            "artifactProduced",
            "validationProduced",
            "usageUpdated",
            "rawEngineEvent",
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
        ] {
            assert!(
                payload.contains(&format!("\"type\": \"{discriminator}\"")),
                "missing generated payload discriminator {discriminator}"
            );
        }

        let generated_type = |type_name: &str| {
            std::fs::read_to_string(output_dir.join(format!("{type_name}.ts")))
                .unwrap()
                .lines()
                .find(|line| line.starts_with("export type "))
                .unwrap()
                .to_owned()
        };
        assert_eq!(
            generated_type("PermissionOutcome"),
            "export type PermissionOutcome = \"allowed_once\" | \"allowed_for_run\" | \"denied\" | \"cancelled\";"
        );
        assert_eq!(
            generated_type("SessionTransition"),
            "export type SessionTransition = \"created\" | \"resumed\" | \"rotated\";"
        );
        assert_eq!(
            generated_type("LivenessState"),
            "export type LivenessState = \"alive\" | \"stalled\";"
        );

        let work = std::fs::read_to_string(output_dir.join("WorkSummary.ts")).unwrap();
        assert!(work.contains("id: string"));

        let run = std::fs::read_to_string(output_dir.join("RunSummary.ts")).unwrap();
        assert!(
            run.contains("id: string")
                && run.contains("workId: string")
                && run.contains("assignmentId: string | null")
                && run.contains("agentInstanceId: string | null")
        );

        let start = std::fs::read_to_string(output_dir.join("StartWorkOutput.ts")).unwrap();
        assert!(
            start.contains("assignment: AssignmentSummary")
                && start.contains("run: RunSummary | null")
                && start.contains("userMessage: UserMessageSummary")
        );

        let session = std::fs::read_to_string(output_dir.join("AgentSessionSummary.ts")).unwrap();
        assert!(
            !session.contains("engineSessionId"),
            "AgentSessionSummary must not expose the opaque Engine session reference"
        );

        let permission = std::fs::read_to_string(output_dir.join("PermissionMode.ts")).unwrap();
        assert!(permission.contains("\"ask_every_step\" | \"balanced\" | \"auto_execute\""));
    }

    #[test]
    fn stale_agent_binding_content_is_detected() {
        let stale = "export type AgentDefinitionSummary = { version: bigint };\r\n";
        let generated = "export type AgentDefinitionSummary = { version: number };\n";
        let unchanged_with_different_newlines =
            "export type AgentDefinitionSummary = { version: number };\r\n";

        assert!(bindings_semantically_equal(
            unchanged_with_different_newlines,
            generated
        ));
        assert!(!bindings_semantically_equal(stale, generated));
    }
}
