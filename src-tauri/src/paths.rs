use std::path::{Path, PathBuf};

use tauri::{Runtime, path::PathResolver};

use crate::error::AppError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppPaths {
    roaming_root: PathBuf,
    local_root: PathBuf,
}

impl AppPaths {
    pub fn new(roaming_root: impl AsRef<Path>, local_root: impl AsRef<Path>) -> Self {
        Self {
            roaming_root: roaming_root.as_ref().to_path_buf(),
            local_root: local_root.as_ref().to_path_buf(),
        }
    }

    pub fn from_resolver<R: Runtime>(resolver: &PathResolver<R>) -> Result<Self, AppError> {
        Ok(Self::new(
            resolver.app_data_dir()?,
            resolver.app_local_data_dir()?,
        ))
    }

    pub fn database_path(&self) -> PathBuf {
        self.roaming_root.join("piwork.sqlite3")
    }

    pub fn engine_sessions_dir(&self) -> PathBuf {
        self.roaming_root.join("engine-sessions")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.local_root.join("logs")
    }

    pub fn runtime_dir(&self) -> PathBuf {
        self.local_root.join("runtime")
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.roaming_root.join("backups")
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::AppPaths;

    #[test]
    fn application_paths_stay_under_their_configured_roots() {
        let roaming_root = PathBuf::from("roaming-root");
        let local_root = PathBuf::from("local-root");
        let paths = AppPaths::new(&roaming_root, &local_root);

        for path in [
            paths.database_path(),
            paths.engine_sessions_dir(),
            paths.backups_dir(),
        ] {
            assert!(path.starts_with(&roaming_root));
        }

        for path in [paths.logs_dir(), paths.runtime_dir()] {
            assert!(path.starts_with(&local_root));
        }

        assert_eq!(paths.database_path(), roaming_root.join("piwork.sqlite3"));
        assert_eq!(
            paths.engine_sessions_dir(),
            roaming_root.join("engine-sessions")
        );
        assert_eq!(paths.backups_dir(), roaming_root.join("backups"));
        assert_eq!(paths.logs_dir(), local_root.join("logs"));
        assert_eq!(paths.runtime_dir(), local_root.join("runtime"));
    }
}
