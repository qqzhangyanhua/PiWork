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
        // Must match the SQLite workspace contract: lower(replace(path, '\', '/')).
        // Windows already folded case here; Unix tempfile names are mixed-case, so
        // skipping lower() lets the autofill trigger create a second workspace.
        let identity = canonical_root
            .to_string_lossy()
            .replace('\\', "/")
            .to_lowercase();
        Ok(Self {
            canonical_root,
            identity,
        })
    }
}
