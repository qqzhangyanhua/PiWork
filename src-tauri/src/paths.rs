use std::path::{Path, PathBuf};

#[cfg(debug_assertions)]
use std::ffi::OsString;

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
        #[cfg(debug_assertions)]
        if let Some(paths) =
            Self::from_debug_override(std::env::var_os("PIWORK_TEST_APP_PATHS_ROOT"))
        {
            return Ok(paths);
        }

        Ok(Self::new(
            resolver.app_data_dir()?,
            resolver.app_local_data_dir()?,
        ))
    }

    #[cfg(debug_assertions)]
    fn from_debug_override(root: Option<OsString>) -> Option<Self> {
        root.filter(|root| !root.is_empty())
            .map(PathBuf::from)
            .map(|root| Self::new(root.join("roaming"), root.join("local")))
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

    pub fn resources_dir(&self) -> PathBuf {
        self.roaming_root.join("resources")
    }

    pub fn resource_cache_dir(&self) -> PathBuf {
        self.local_root.join("resource-cache")
    }
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, path::PathBuf};

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
            paths.resources_dir(),
        ] {
            assert!(path.starts_with(&roaming_root));
        }

        for path in [
            paths.logs_dir(),
            paths.runtime_dir(),
            paths.resource_cache_dir(),
        ] {
            assert!(path.starts_with(&local_root));
        }

        assert_eq!(paths.database_path(), roaming_root.join("piwork.sqlite3"));
        assert_eq!(
            paths.engine_sessions_dir(),
            roaming_root.join("engine-sessions")
        );
        assert_eq!(paths.backups_dir(), roaming_root.join("backups"));
        assert_eq!(paths.resources_dir(), roaming_root.join("resources"));
        assert_eq!(paths.logs_dir(), local_root.join("logs"));
        assert_eq!(paths.runtime_dir(), local_root.join("runtime"));
        assert_eq!(
            paths.resource_cache_dir(),
            local_root.join("resource-cache")
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    fn debug_path_override_is_explicit_and_scoped_to_app_roots() {
        assert!(AppPaths::from_debug_override(None).is_none());

        let root = PathBuf::from("diagnostic-root");
        let paths = AppPaths::from_debug_override(Some(OsString::from(&root))).unwrap();

        assert_eq!(paths.database_path(), root.join("roaming/piwork.sqlite3"));
        assert_eq!(paths.logs_dir(), root.join("local/logs"));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn resolver_checks_the_debug_override_before_tauri_paths() {
        let source = include_str!("paths.rs");
        let override_lookup = source
            .find("from_debug_override(std::env::var_os(\"PIWORK_TEST_APP_PATHS_ROOT\"))")
            .expect("debug builds must read the explicit test path override");
        let tauri_lookup = source
            .find("resolver.app_data_dir()")
            .expect("normal Tauri path resolution is missing");

        assert!(override_lookup < tauri_lookup);
    }
}
