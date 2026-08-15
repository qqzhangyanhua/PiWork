use std::path::PathBuf;

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
        let _declared = self.capabilities().session_resume;
        Err(EngineError::Unsupported("session_resume"))
    }

    async fn rotate(
        &self,
        _context: EngineRunContext,
        _reason: &str,
    ) -> Result<EngineSessionRef, EngineError> {
        let _declared = self.capabilities().session_rotate;
        Err(EngineError::Unsupported("session_rotate"))
    }

    async fn steer(&self, _run_id: &str, _input: EngineInput) -> Result<(), EngineError> {
        let _declared = self.capabilities().native_steer;
        Err(EngineError::Unsupported("native_steer"))
    }

    async fn abort(&self, _run_id: &str) -> Result<(), EngineError> {
        let _declared = self.capabilities().cancel;
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
    #[error("engine capability is unsupported: {0}")]
    Unsupported(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineRunContext {
    pub work_id: String,
    pub run_id: String,
    pub root_path: PathBuf,
    pub permission_mode: PermissionMode,
    pub assignment_id: String,
    pub agent_instance_id: String,
    pub agent_session_id: String,
    pub session_generation: u32,
    pub resolved_model_configuration_id: Option<String>,
    pub effective_permission: PermissionMode,
}

impl EngineRunContext {
    pub fn new(
        work_id: String,
        run_id: String,
        root_path: PathBuf,
        permission_mode: PermissionMode,
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

        Ok(Self {
            work_id,
            run_id,
            root_path,
            permission_mode,
            assignment_id: String::new(),
            agent_instance_id: String::new(),
            agent_session_id: String::new(),
            session_generation: 0,
            resolved_model_configuration_id: None,
            effective_permission: permission_mode,
        })
    }

    #[cfg(test)]
    pub fn test(work_id: &str, run_id: &str) -> Self {
        Self {
            work_id: work_id.into(),
            run_id: run_id.into(),
            root_path: std::env::current_dir().unwrap(),
            permission_mode: PermissionMode::Balanced,
            assignment_id: "assignment:test".into(),
            agent_instance_id: "agent-instance:test".into(),
            agent_session_id: "agent-session:test".into(),
            session_generation: 1,
            resolved_model_configuration_id: None,
            effective_permission: PermissionMode::Balanced,
        }
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

    use super::{EngineEvent, EngineRunContext};

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
            "work-1".into(),
            "run-1".into(),
            non_canonical,
            PermissionMode::Balanced,
        )
        .unwrap();

        assert_eq!(context.root_path, dunce::canonicalize(child).unwrap());
        assert!(context.assignment_id.is_empty());
        assert!(context.agent_instance_id.is_empty());
        assert!(context.agent_session_id.is_empty());
        assert_eq!(context.session_generation, 0);
        assert_eq!(context.resolved_model_configuration_id, None);
        assert_eq!(context.effective_permission, PermissionMode::Balanced);
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
                "work-1".into(),
                "run-1".into(),
                root_path,
                PermissionMode::Balanced,
            )
            .unwrap_err();

            assert_eq!(error.to_string(), expected_message);
        }
    }
}
