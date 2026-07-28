use std::path::PathBuf;

use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::domain::work::PermissionMode;

pub mod fake;
pub mod publisher;
pub mod supervisor;

#[async_trait]
pub trait EngineAdapter: Send + Sync {
    fn kind(&self) -> &'static str;

    async fn start(
        &self,
        context: EngineRunContext,
        prompt: String,
        sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError>;

    async fn abort(&self, run_id: &str) -> Result<(), EngineError>;
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineRunContext {
    pub work_id: String,
    pub run_id: String,
    pub root_path: PathBuf,
    pub permission_mode: PermissionMode,
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
        })
    }

    #[cfg(test)]
    pub fn test(work_id: &str, run_id: &str) -> Self {
        Self {
            work_id: work_id.into(),
            run_id: run_id.into(),
            root_path: std::env::current_dir().unwrap(),
            permission_mode: PermissionMode::Balanced,
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
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::RunCompleted { .. } | Self::RunFailed { .. })
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::work::PermissionMode;

    use super::EngineRunContext;

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
