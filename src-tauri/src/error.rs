use std::path::PathBuf;

use serde::{Serialize, Serializer};
use serde_json::{Value, json};

use crate::domain::work::{RunStatus, WorkStatus};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid input for {field}: {message}")]
    InvalidInput { field: String, message: String },

    #[error("work not found: {work_id}")]
    WorkNotFound { work_id: String },

    #[error("run not found: {run_id}")]
    RunNotFound { run_id: String },

    #[error("resource not found: {resource_id}")]
    ResourceNotFound { resource_id: String },

    #[error("resource import failed: {code}")]
    ResourceImport { code: String },

    #[error("resource storage operation failed")]
    ResourceStorage,

    #[error("invalid Work status transition for {work_id}: {from:?} -> {to:?}")]
    InvalidWorkState {
        work_id: String,
        from: WorkStatus,
        to: WorkStatus,
    },

    #[error("invalid Run status transition for {run_id}: {from:?} -> {to:?}")]
    InvalidRunState {
        run_id: String,
        from: RunStatus,
        to: RunStatus,
    },

    #[error("Run was modified concurrently: {run_id}")]
    ConcurrentRunModification { run_id: String },

    #[error("Work was modified concurrently: {work_id}")]
    ConcurrentWorkModification { work_id: String },

    #[error("Work already has an active Run: {work_id}")]
    WorkAlreadyRunning { work_id: String },

    #[error("Work engine lifecycle is faulted: {work_id}")]
    EngineFaulted { work_id: String },

    #[error("Engine failed to start for Work: {work_id}")]
    EngineStartFailed {
        work_id: String,
        reason: Option<String>,
    },

    #[error("engine operation failed: {message}")]
    Engine { message: String },

    #[error("event publication failed: {message}")]
    EventPublish { message: String },

    #[error("model configuration is required")]
    ModelConfigurationRequired,

    #[error("model connection failed: {message}")]
    ModelConnection { message: String },

    #[error("model configuration failed: {message}")]
    ModelConfiguration { message: String },

    #[error("credential operation failed: {message}")]
    Credential { message: String },

    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),

    #[error("database migration failed: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("I/O operation failed for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("workspace path resolution failed for {path}: {source}")]
    WorkspacePathResolution {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("referenced file is unavailable: {path}: {message}")]
    ReferencedFile { path: String, message: String },

    #[error("application path resolution failed: {0}")]
    PathResolution(#[from] tauri::Error),
}

impl AppError {
    pub fn invalid_input(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self::InvalidInput {
            field: field.into(),
            message: message.into(),
        }
    }

    pub fn work_not_found(work_id: impl Into<String>) -> Self {
        Self::WorkNotFound {
            work_id: work_id.into(),
        }
    }

    pub fn run_not_found(run_id: impl Into<String>) -> Self {
        Self::RunNotFound {
            run_id: run_id.into(),
        }
    }

    pub fn invalid_work_state(
        work_id: impl Into<String>,
        from: WorkStatus,
        to: WorkStatus,
    ) -> Self {
        Self::InvalidWorkState {
            work_id: work_id.into(),
            from,
            to,
        }
    }

    pub fn invalid_run_state(run_id: impl Into<String>, from: RunStatus, to: RunStatus) -> Self {
        Self::InvalidRunState {
            run_id: run_id.into(),
            from,
            to,
        }
    }

    pub fn concurrent_run_modification(run_id: impl Into<String>) -> Self {
        Self::ConcurrentRunModification {
            run_id: run_id.into(),
        }
    }

    pub fn concurrent_work_modification(work_id: impl Into<String>) -> Self {
        Self::ConcurrentWorkModification {
            work_id: work_id.into(),
        }
    }

    pub fn work_already_running(work_id: impl Into<String>) -> Self {
        Self::WorkAlreadyRunning {
            work_id: work_id.into(),
        }
    }

    pub fn engine_faulted(work_id: impl Into<String>) -> Self {
        Self::EngineFaulted {
            work_id: work_id.into(),
        }
    }

    pub fn engine_start_failed(work_id: impl Into<String>) -> Self {
        Self::EngineStartFailed {
            work_id: work_id.into(),
            reason: None,
        }
    }

    pub fn resource_not_found(resource_id: impl Into<String>) -> Self {
        Self::ResourceNotFound {
            resource_id: resource_id.into(),
        }
    }

    pub fn resource_import(code: impl Into<String>) -> Self {
        Self::ResourceImport { code: code.into() }
    }

    pub fn engine_start_failed_with_reason(
        work_id: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::EngineStartFailed {
            work_id: work_id.into(),
            reason: Some(reason.into()),
        }
    }

    pub fn engine(message: impl Into<String>) -> Self {
        Self::Engine {
            message: message.into(),
        }
    }

    pub fn referenced_file(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ReferencedFile {
            path: path.into(),
            message: message.into(),
        }
    }

    pub fn event_publish(message: impl Into<String>) -> Self {
        Self::EventPublish {
            message: message.into(),
        }
    }

    fn wire_parts(&self) -> (&'static str, &str, Option<Value>) {
        match self {
            Self::InvalidInput { field, message } => {
                ("invalid_input", message, Some(json!({ "field": field })))
            }
            Self::WorkNotFound { work_id } => (
                "not_found",
                "Work not found",
                Some(json!({ "workId": work_id })),
            ),
            Self::RunNotFound { run_id } => (
                "not_found",
                "Run not found",
                Some(json!({ "runId": run_id })),
            ),
            Self::InvalidWorkState { work_id, from, to } => (
                "invalid_work_state",
                "Work status transition is invalid",
                Some(json!({ "workId": work_id, "from": from, "to": to })),
            ),
            Self::InvalidRunState { run_id, from, to } => (
                "invalid_work_state",
                "Run status transition is invalid",
                Some(json!({ "runId": run_id, "from": from, "to": to })),
            ),
            Self::ConcurrentRunModification { run_id } => (
                "concurrent_modification",
                "Run was modified concurrently",
                Some(json!({ "runId": run_id })),
            ),
            Self::ConcurrentWorkModification { work_id } => (
                "concurrent_modification",
                "Work was modified concurrently",
                Some(json!({ "workId": work_id })),
            ),
            Self::WorkAlreadyRunning { work_id } => (
                "work_already_running",
                "Work already has an active Run",
                Some(json!({ "workId": work_id })),
            ),
            Self::EngineFaulted { work_id } => (
                "engine_faulted",
                "Work engine lifecycle is faulted",
                Some(json!({ "workId": work_id })),
            ),
            Self::EngineStartFailed { work_id, reason } => {
                let mut details = json!({ "workId": work_id });
                if let Some(reason) = reason {
                    details["reason"] = Value::String(reason.clone());
                }
                (
                    "engine_start_failed",
                    "Engine failed to start",
                    Some(details),
                )
            }
            Self::Engine { .. } => ("engine_error", "Engine operation failed", None),
            Self::EventPublish { .. } => (
                "event_publish_error",
                "Work event could not be published",
                None,
            ),
            Self::ResourceNotFound { resource_id } => (
                "resource_not_found",
                "Resource not found",
                Some(json!({ "resourceId": resource_id })),
            ),
            Self::ResourceImport { code } => (
                "resource_import",
                "Resource import failed",
                Some(json!({ "reason": code })),
            ),
            Self::ResourceStorage => (
                "resource_storage",
                "Resource storage operation failed",
                None,
            ),
            Self::ModelConfigurationRequired => (
                "model_configuration_required",
                "A verified model configuration is required",
                None,
            ),
            Self::ModelConnection { .. } => (
                "model_connection_error",
                "The model provider could not be verified",
                None,
            ),
            Self::ModelConfiguration { .. } => (
                "model_configuration_error",
                "The model configuration is unavailable",
                None,
            ),
            Self::Credential { .. } => (
                "credential_error",
                "The model credential could not be stored",
                None,
            ),
            Self::Database(_) => ("database_error", "Database operation failed", None),
            Self::Migration(_) => ("migration_error", "Database migration failed", None),
            Self::Io { path, .. } => (
                "io_error",
                "I/O operation failed",
                Some(json!({ "path": path.to_string_lossy() })),
            ),
            Self::WorkspacePathResolution { path, .. } => (
                "path_resolution_error",
                "Workspace path could not be resolved",
                Some(json!({
                    "field": "rootPath",
                    "path": path.to_string_lossy()
                })),
            ),
            Self::ReferencedFile { path, .. } => (
                "referenced_file_error",
                "A referenced project file is unavailable",
                Some(json!({ "path": path })),
            ),
            Self::PathResolution(_) => (
                "path_resolution_error",
                "Application path could not be resolved",
                None,
            ),
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct WireError<'a> {
            code: &'a str,
            message: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            details: Option<Value>,
        }

        let (code, message, details) = self.wire_parts();
        WireError {
            code,
            message,
            details,
        }
        .serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::domain::work::{RunStatus, WorkStatus};

    use super::AppError;

    #[test]
    fn invalid_input_has_a_stable_serialized_shape() {
        let error = AppError::invalid_input("title", "title must not be empty");

        assert_eq!(
            serde_json::to_value(error).unwrap(),
            json!({
                "code": "invalid_input",
                "message": "title must not be empty",
                "details": { "field": "title" }
            })
        );
    }

    #[test]
    fn database_error_serialization_hides_internal_details() {
        let error = AppError::Database(sqlx::Error::Protocol("secret database diagnostic".into()));

        assert_eq!(
            serde_json::to_value(error).unwrap(),
            json!({
                "code": "database_error",
                "message": "Database operation failed"
            })
        );
    }

    #[test]
    fn engine_start_failure_exposes_only_the_explicit_safe_diagnostic() {
        let error = AppError::engine_start_failed_with_reason(
            "work-id",
            "pi_rpc_stdout_closed: sidecar exited with code 1",
        );

        assert_eq!(
            serde_json::to_value(error).unwrap(),
            json!({
                "code": "engine_start_failed",
                "message": "Engine failed to start",
                "details": {
                    "workId": "work-id",
                    "reason": "pi_rpc_stdout_closed: sidecar exited with code 1"
                }
            })
        );
    }

    #[test]
    fn workspace_path_error_identifies_the_root_path_field_without_raw_io_text() {
        let error = AppError::WorkspacePathResolution {
            path: "C:/missing/private-workspace".into(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "raw operating system path diagnostic",
            ),
        };
        let serialized = serde_json::to_value(error).unwrap();

        assert_eq!(serialized["code"], "path_resolution_error");
        assert_eq!(serialized["details"]["field"], "rootPath");
        assert_eq!(
            serialized["details"]["path"],
            "C:/missing/private-workspace"
        );
        assert!(
            !serialized
                .to_string()
                .contains("raw operating system path diagnostic")
        );
    }

    #[test]
    fn invalid_state_constructor_has_a_stable_serialized_shape() {
        let error =
            AppError::invalid_work_state("work-id", WorkStatus::Archived, WorkStatus::Running);

        assert_eq!(
            serde_json::to_value(error).unwrap(),
            json!({
                "code": "invalid_work_state",
                "message": "Work status transition is invalid",
                "details": {
                    "workId": "work-id",
                    "from": "archived",
                    "to": "running"
                }
            })
        );
    }

    #[test]
    fn concurrent_modification_constructor_has_a_stable_serialized_shape() {
        let error = AppError::concurrent_run_modification("run-id");

        assert_eq!(
            serde_json::to_value(error).unwrap(),
            json!({
                "code": "concurrent_modification",
                "message": "Run was modified concurrently",
                "details": { "runId": "run-id" }
            })
        );

        let invalid_run =
            AppError::invalid_run_state("run-id", RunStatus::Completed, RunStatus::Running);
        assert_eq!(
            serde_json::to_value(invalid_run).unwrap()["code"],
            "invalid_work_state"
        );
    }
}
