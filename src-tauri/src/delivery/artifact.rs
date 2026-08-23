use std::path::{Component, Path, PathBuf};

use crate::error::AppError;

pub const MAX_ARTIFACT_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedArtifact {
    pub path: PathBuf,
    pub size_bytes: u64,
}

pub fn admit_artifact(root: &Path, submitted: &str) -> Result<AdmittedArtifact, AppError> {
    if submitted.trim().is_empty() {
        return Err(AppError::invalid_input(
            "artifacts",
            "path must not be empty",
        ));
    }
    let submitted_path = PathBuf::from(submitted);
    if submitted_path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err(AppError::invalid_input(
            "artifacts",
            "path must remain inside the Workspace",
        ));
    }
    let candidate = if submitted_path.is_absolute() {
        submitted_path
    } else {
        root.join(submitted_path)
    };
    let canonical = dunce::canonicalize(&candidate).map_err(|source| AppError::ReferencedFile {
        path: submitted.to_owned(),
        message: source.to_string(),
    })?;
    if !canonical.starts_with(root) {
        return Err(AppError::invalid_input(
            "artifacts",
            "path must remain inside the Workspace",
        ));
    }
    let metadata = canonical.metadata().map_err(|source| AppError::Io {
        path: canonical.clone(),
        source,
    })?;
    if !metadata.is_file() {
        return Err(AppError::invalid_input(
            "artifacts",
            "artifact must be a regular file",
        ));
    }
    if metadata.len() > MAX_ARTIFACT_BYTES {
        return Err(AppError::invalid_input(
            "artifacts",
            "artifact exceeds 100 MiB",
        ));
    }
    Ok(AdmittedArtifact {
        path: canonical,
        size_bytes: metadata.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_must_exist_inside_workspace() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("report.md"), "done").unwrap();
        let root_path = dunce::canonicalize(root.path()).unwrap();
        assert!(admit_artifact(&root_path, "report.md").is_ok());
        assert!(admit_artifact(&root_path, "missing.md").is_err());
        assert!(admit_artifact(&root_path, "../escape.md").is_err());
    }
}
