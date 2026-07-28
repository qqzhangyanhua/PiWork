use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
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

    #[error("application path resolution failed: {0}")]
    PathResolution(#[from] tauri::Error),
}
