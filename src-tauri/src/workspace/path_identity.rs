use std::path::{Path, PathBuf};

use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePathIdentity {
    pub canonical_root: PathBuf,
    pub identity: String,
}

impl WorkspacePathIdentity {
    pub fn resolve(path: &Path) -> Result<Self, AppError> {
        let canonical_root =
            dunce::canonicalize(path).map_err(|source| AppError::WorkspacePathResolution {
                path: path.to_path_buf(),
                source,
            })?;
        if !canonical_root.is_dir() {
            return Err(AppError::invalid_input(
                "rootPath",
                "Workspace root must be a directory",
            ));
        }
        let normalized = canonical_root.to_string_lossy().replace('\\', "/");
        #[cfg(windows)]
        let identity = normalized.to_lowercase();
        #[cfg(not(windows))]
        let identity = normalized;
        Ok(Self {
            canonical_root,
            identity,
        })
    }
}
