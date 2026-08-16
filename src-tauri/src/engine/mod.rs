use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::domain::{
    event::{LivenessState, PermissionOutcome, SessionTransition},
    work::PermissionMode,
};

pub mod activity_observer;
pub mod fake;
pub mod pi;
pub mod publisher;
pub mod supervisor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineImage {
    pub media_type: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineDocument {
    pub name: String,
    pub media_type: String,
    pub content: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineInput {
    pub message: String,
    pub images: Vec<EngineImage>,
    pub documents: Vec<EngineDocument>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EngineCapabilities {
    pub session_resume: bool,
    pub session_rotate: bool,
    pub native_steer: bool,
    pub cancel: bool,
    pub thought_stream: bool,
    pub plan_updates: bool,
    pub permission_requests: bool,
    pub tool_progress: bool,
    pub usage_reporting: bool,
    pub parallel_tool_calls: bool,
}

#[async_trait]
pub trait EngineAdapter: Send + Sync {
    fn kind(&self) -> &'static str;

    fn capabilities(&self) -> EngineCapabilities {
        EngineCapabilities::default()
    }

    async fn model_label(&self, fallback: &str) -> Result<String, EngineError> {
        Ok(fallback.to_owned())
    }

    async fn start(
        &self,
        context: EngineRunContext,
        input: EngineInput,
        sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError>;

    async fn resume(
        &self,
        _context: EngineRunContext,
        _input: EngineInput,
        _sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        Err(EngineError::Unsupported("session_resume"))
    }

    async fn rotate(
        &self,
        _context: EngineRunContext,
        _reason: &str,
    ) -> Result<EngineSessionRef, EngineError> {
        Err(EngineError::Unsupported("session_rotate"))
    }

    async fn steer(&self, _run_id: &str, _input: EngineInput) -> Result<(), EngineError> {
        Err(EngineError::Unsupported("native_steer"))
    }

    async fn abort(&self, _run_id: &str) -> Result<(), EngineError> {
        Err(EngineError::Unsupported("cancel"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineSessionRef {
    pub engine_kind: String,
    pub session_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("engine failed to start: {0}")]
    Start(String),
    #[error("engine event channel closed")]
    ChannelClosed,
    #[error("engine run is not active")]
    NotRunning,
    #[error("engine run was aborted")]
    Aborted,
    #[error("engine cleanup could not be confirmed")]
    CleanupUnconfirmed,
    #[error("engine capability is unsupported: {0}")]
    Unsupported(&'static str),
}

const MAX_ENGINE_IDENTITY_BYTES: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineRunIdentity {
    work_id: String,
    run_id: String,
    assignment_id: String,
    agent_instance_id: String,
    agent_session_id: String,
    session_generation: u32,
}

impl EngineRunIdentity {
    pub fn new(
        work_id: String,
        run_id: String,
        assignment_id: String,
        agent_instance_id: String,
        agent_session_id: String,
        session_generation: u32,
    ) -> Result<Self, EngineError> {
        for (name, value) in [
            ("work", work_id.as_str()),
            ("run", run_id.as_str()),
            ("assignment", assignment_id.as_str()),
            ("agent instance", agent_instance_id.as_str()),
            ("agent session", agent_session_id.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(EngineError::Start(format!(
                    "{name} identity must not be empty"
                )));
            }
            if value.len() > MAX_ENGINE_IDENTITY_BYTES {
                return Err(EngineError::Start(format!(
                    "{name} identity exceeds {MAX_ENGINE_IDENTITY_BYTES} bytes"
                )));
            }
        }
        for (name, value) in [("work", work_id.as_str()), ("run", run_id.as_str())] {
            let portable = value != "."
                && value != ".."
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
            if !portable {
                return Err(EngineError::Start(format!(
                    "{name} identity must be a portable path segment"
                )));
            }
        }
        Ok(Self {
            work_id,
            run_id,
            assignment_id,
            agent_instance_id,
            agent_session_id,
            session_generation,
        })
    }
}

/// Validated, read-only execution context passed to an engine adapter.
///
/// Identity fields cannot be changed after construction:
///
/// ```compile_fail,E0616
/// use piwork_lib::engine::EngineRunContext;
///
/// fn invalidate(context: &mut EngineRunContext) {
///     context.work_id.clear();
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineRunContext {
    work_id: String,
    run_id: String,
    root_path: PathBuf,
    permission_mode: PermissionMode,
    assignment_id: String,
    agent_instance_id: String,
    agent_session_id: String,
    session_generation: u32,
    resolved_model_configuration_id: Option<String>,
    effective_permission: PermissionMode,
}

impl EngineRunContext {
    pub fn new(
        identity: EngineRunIdentity,
        root_path: PathBuf,
        permission_mode: PermissionMode,
        resolved_model_configuration_id: Option<String>,
        effective_permission: PermissionMode,
    ) -> Result<Self, EngineError> {
        if !root_path.is_absolute() {
            return Err(EngineError::Start("workspace root must be absolute".into()));
        }
        let root_path = dunce::canonicalize(root_path)
            .map_err(|_| EngineError::Start("workspace root could not be canonicalized".into()))?;
        if !root_path.is_dir() {
            return Err(EngineError::Start(
                "workspace root must be a directory".into(),
            ));
        }

        let EngineRunIdentity {
            work_id,
            run_id,
            assignment_id,
            agent_instance_id,
            agent_session_id,
            session_generation,
        } = identity;
        Ok(Self {
            work_id,
            run_id,
            root_path,
            permission_mode,
            assignment_id,
            agent_instance_id,
            agent_session_id,
            session_generation,
            resolved_model_configuration_id,
            effective_permission,
        })
    }

    pub fn work_id(&self) -> &str {
        &self.work_id
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    pub fn permission_mode(&self) -> PermissionMode {
        self.permission_mode
    }

    pub fn assignment_id(&self) -> &str {
        &self.assignment_id
    }

    pub fn agent_instance_id(&self) -> &str {
        &self.agent_instance_id
    }

    pub fn agent_session_id(&self) -> &str {
        &self.agent_session_id
    }

    pub fn session_generation(&self) -> u32 {
        self.session_generation
    }

    pub fn resolved_model_configuration_id(&self) -> Option<&str> {
        self.resolved_model_configuration_id.as_deref()
    }

    pub fn effective_permission(&self) -> PermissionMode {
        self.effective_permission
    }

    #[cfg(test)]
    pub fn test(work_id: &str, run_id: &str) -> Self {
        let identity = EngineRunIdentity::new(
            work_id.into(),
            run_id.into(),
            format!("assignment:{run_id}"),
            "agent-instance:test".into(),
            work_id.into(),
            0,
        )
        .unwrap();
        Self::new(
            identity,
            std::env::current_dir().unwrap(),
            PermissionMode::Balanced,
            None,
            PermissionMode::Balanced,
        )
        .unwrap()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineEvent {
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
}

impl EngineEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::RunStarted { .. } => "run_started",
            Self::AssistantDelta { .. } => "assistant_delta",
            Self::ToolStarted { .. } => "tool_started",
            Self::ToolFinished { .. } => "tool_finished",
            Self::RunCompleted { .. } => "run_completed",
            Self::RunFailed { .. } => "run_failed",
            Self::ThoughtDelta { .. } => "thought_delta",
            Self::PlanChanged { .. } => "plan_changed",
            Self::ToolPending { .. } => "tool_pending",
            Self::ToolProgress { .. } => "tool_progress",
            Self::PermissionRequested { .. } => "permission_requested",
            Self::PermissionResolved { .. } => "permission_resolved",
            Self::Waiting { .. } => "waiting",
            Self::Liveness { .. } => "liveness",
            Self::SessionChanged { .. } => "session_changed",
            Self::ArtifactProduced { .. } => "artifact_produced",
            Self::ValidationProduced { .. } => "validation_produced",
            Self::UsageUpdated { .. } => "usage_updated",
            Self::RawEngineEvent { .. } => "raw_engine_event",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::RunCompleted { .. } | Self::RunFailed { .. })
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::{
        event::{LivenessState, PermissionOutcome, SessionTransition},
        work::PermissionMode,
    };

    use super::{EngineEvent, EngineRunContext, EngineRunIdentity};

    fn identity(run_id: &str) -> EngineRunIdentity {
        EngineRunIdentity::new(
            "work-1".into(),
            run_id.into(),
            "assignment-1".into(),
            "agent-instance-1".into(),
            "agent-session-1".into(),
            0,
        )
        .unwrap()
    }

    #[test]
    fn only_run_terminal_events_are_terminal() {
        assert!(
            !EngineEvent::ThoughtDelta {
                text: "Inspecting the Work".into(),
            }
            .is_terminal()
        );
        assert!(
            !EngineEvent::Waiting {
                reason: "Waiting for input".into(),
            }
            .is_terminal()
        );
        assert!(
            EngineEvent::RunFailed {
                message: "failed".into(),
            }
            .is_terminal()
        );
    }

    #[test]
    fn activity_event_kinds_are_stable_snake_case() {
        let cases = [
            (
                EngineEvent::ThoughtDelta {
                    text: String::new(),
                },
                "thought_delta",
            ),
            (
                EngineEvent::PlanChanged {
                    plan_id: String::new(),
                    revision: 1,
                    text: String::new(),
                },
                "plan_changed",
            ),
            (
                EngineEvent::ToolPending {
                    tool_call_id: String::new(),
                    tool_name: String::new(),
                    input_summary: String::new(),
                },
                "tool_pending",
            ),
            (
                EngineEvent::ToolProgress {
                    tool_call_id: String::new(),
                    tool_name: String::new(),
                    output_summary: String::new(),
                },
                "tool_progress",
            ),
            (
                EngineEvent::PermissionRequested {
                    request_id: String::new(),
                    tool_call_id: None,
                    title: String::new(),
                    detail: String::new(),
                },
                "permission_requested",
            ),
            (
                EngineEvent::PermissionResolved {
                    request_id: String::new(),
                    outcome: PermissionOutcome::Denied,
                },
                "permission_resolved",
            ),
            (
                EngineEvent::Waiting {
                    reason: String::new(),
                },
                "waiting",
            ),
            (
                EngineEvent::Liveness {
                    state: LivenessState::Alive,
                },
                "liveness",
            ),
            (
                EngineEvent::SessionChanged {
                    transition: SessionTransition::Created,
                    reason: None,
                },
                "session_changed",
            ),
            (
                EngineEvent::ArtifactProduced {
                    path: String::new(),
                },
                "artifact_produced",
            ),
            (
                EngineEvent::ValidationProduced {
                    command: String::new(),
                    success: true,
                    summary: String::new(),
                },
                "validation_produced",
            ),
            (
                EngineEvent::UsageUpdated {
                    input_tokens: 1,
                    output_tokens: 2,
                    cache_read_tokens: 3,
                    cache_write_tokens: 4,
                    total_tokens: 10,
                },
                "usage_updated",
            ),
            (
                EngineEvent::RawEngineEvent {
                    kind: String::new(),
                    payload_json: String::new(),
                },
                "raw_engine_event",
            ),
        ];

        for (event, expected_kind) in cases {
            assert_eq!(event.kind(), expected_kind);
        }
    }

    #[test]
    fn run_context_stores_the_canonical_workspace_directory() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let child = temporary_directory.path().join("child");
        std::fs::create_dir(&child).unwrap();
        let non_canonical = child.join("..").join("child");

        let context = EngineRunContext::new(
            identity("run-1"),
            non_canonical,
            PermissionMode::Balanced,
            Some("model-configuration-1".into()),
            PermissionMode::AskEveryStep,
        )
        .unwrap();

        assert_eq!(context.root_path(), dunce::canonicalize(child).unwrap());
        assert_eq!(context.work_id(), "work-1");
        assert_eq!(context.run_id(), "run-1");
        assert_eq!(context.assignment_id(), "assignment-1");
        assert_eq!(context.agent_instance_id(), "agent-instance-1");
        assert_eq!(context.agent_session_id(), "agent-session-1");
        assert_eq!(context.session_generation(), 0);
        assert_eq!(
            context.resolved_model_configuration_id(),
            Some("model-configuration-1")
        );
        assert_eq!(context.permission_mode(), PermissionMode::Balanced);
        assert_eq!(context.effective_permission(), PermissionMode::AskEveryStep);
    }

    #[test]
    fn run_identity_rejects_blank_product_identifiers_and_preserves_generation_zero() {
        let valid = [
            "work-1",
            "run-1",
            "assignment-1",
            "agent-instance-1",
            "agent-session-1",
        ];
        let identity = EngineRunIdentity::new(
            valid[0].into(),
            valid[1].into(),
            valid[2].into(),
            valid[3].into(),
            valid[4].into(),
            0,
        )
        .unwrap();
        assert_eq!(identity.session_generation, 0);

        for blank_index in 0..valid.len() {
            let mut values = valid;
            values[blank_index] = " \n ";
            let error = EngineRunIdentity::new(
                values[0].into(),
                values[1].into(),
                values[2].into(),
                values[3].into(),
                values[4].into(),
                7,
            )
            .unwrap_err();
            assert!(error.to_string().contains("must not be empty"));
        }
    }

    #[test]
    fn run_identity_rejects_non_portable_work_and_run_segments() {
        let valid = [
            "work.AZ_09-",
            "run.AZ_09-",
            "assignment-1",
            "agent-instance-1",
            "agent-session-1",
        ];
        EngineRunIdentity::new(
            valid[0].into(),
            valid[1].into(),
            valid[2].into(),
            valid[3].into(),
            valid[4].into(),
            0,
        )
        .unwrap();

        for (field_index, field_name) in [(0, "work"), (1, "run")] {
            for invalid in [
                "/tmp/escape",
                r"C:\escape",
                r"\\server\share",
                ".",
                "..",
                "nested/path",
                r"nested\path",
                "nested//path",
                r"nested\\path",
                "contains space",
                "\0",
                "工作",
            ] {
                let mut values = valid.map(str::to_owned);
                values[field_index] = invalid.into();

                let error = EngineRunIdentity::new(
                    values[0].clone(),
                    values[1].clone(),
                    values[2].clone(),
                    values[3].clone(),
                    values[4].clone(),
                    0,
                )
                .unwrap_err();

                assert_eq!(
                    error.to_string(),
                    format!(
                        "engine failed to start: {field_name} identity must be a portable path segment"
                    )
                );
            }
        }
    }

    #[test]
    fn run_identity_bounds_every_identity_field_without_echoing_the_value() {
        let baseline = [
            "work-1".to_owned(),
            "run-1".to_owned(),
            "assignment-1".to_owned(),
            "agent-instance-1".to_owned(),
            "agent-session-1".to_owned(),
        ];

        for (field_index, field_name) in [
            (0, "work"),
            (1, "run"),
            (2, "assignment"),
            (3, "agent instance"),
            (4, "agent session"),
        ] {
            let mut at_limit = baseline.clone();
            at_limit[field_index] = "a".repeat(255);
            EngineRunIdentity::new(
                at_limit[0].clone(),
                at_limit[1].clone(),
                at_limit[2].clone(),
                at_limit[3].clone(),
                at_limit[4].clone(),
                0,
            )
            .unwrap();

            let mut over_limit = baseline.clone();
            over_limit[field_index] = "z".repeat(256);
            let error = EngineRunIdentity::new(
                over_limit[0].clone(),
                over_limit[1].clone(),
                over_limit[2].clone(),
                over_limit[3].clone(),
                over_limit[4].clone(),
                0,
            )
            .unwrap_err();

            assert_eq!(
                error.to_string(),
                format!("engine failed to start: {field_name} identity exceeds 255 bytes")
            );
            assert!(!format!("{error:?}").contains(&over_limit[field_index]));
        }
    }

    #[test]
    fn run_context_rejects_relative_missing_and_file_workspace_paths() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let missing = temporary_directory.path().join("missing");
        let file = temporary_directory.path().join("file.txt");
        std::fs::write(&file, "not a directory").unwrap();

        for (root_path, expected_message) in [
            (
                std::path::PathBuf::from("relative-workspace"),
                "engine failed to start: workspace root must be absolute",
            ),
            (
                missing,
                "engine failed to start: workspace root could not be canonicalized",
            ),
            (
                file,
                "engine failed to start: workspace root must be a directory",
            ),
        ] {
            let error = EngineRunContext::new(
                identity("run-1"),
                root_path,
                PermissionMode::Balanced,
                None,
                PermissionMode::Balanced,
            )
            .unwrap_err();

            assert_eq!(error.to_string(), expected_message);
        }
    }
}
