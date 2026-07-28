use std::path::PathBuf;

use serde::{Serialize, Serializer};
use serde_json::{Value, json};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid input for {field}: {message}")]
    InvalidInput { field: String, message: String },

    #[error("work not found: {work_id}")]
    WorkNotFound { work_id: String },

    #[error("run not found: {run_id}")]
    RunNotFound { run_id: String },

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
                Some(json!({ "path": path.to_string_lossy() })),
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
}
