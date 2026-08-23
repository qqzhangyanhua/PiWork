use std::path::PathBuf;

use crate::{
    domain::work::{CreateWorkInput, ProjectFileSummary, WorkDetail, WorkSummary},
    error::AppError,
};

use super::{project_files, repository::WorkRepository};

#[derive(Clone)]
pub struct WorkService {
    repository: WorkRepository,
}

impl WorkService {
    pub fn new(repository: WorkRepository) -> Self {
        Self { repository }
    }

    pub async fn create_work(&self, input: CreateWorkInput) -> Result<WorkDetail, AppError> {
        self.repository.create(input).await
    }

    pub async fn list_works(&self) -> Result<Vec<WorkSummary>, AppError> {
        self.repository.list().await
    }

    pub async fn get_work(&self, work_id: &str) -> Result<WorkDetail, AppError> {
        self.repository
            .get(work_id)
            .await?
            .ok_or_else(|| AppError::work_not_found(work_id))
    }

    pub async fn list_project_files(
        &self,
        root_path: String,
    ) -> Result<Vec<ProjectFileSummary>, AppError> {
        tokio::task::spawn_blocking(move || {
            project_files::list_project_files(PathBuf::from(root_path).as_path())
        })
        .await
        .map_err(|_| AppError::engine("Project file indexing task failed"))?
    }

    pub async fn recover_interrupted_runs(&self) -> Result<u64, AppError> {
        self.repository.recover_interrupted_runs().await
    }

    pub async fn archive_work(&self, work_id: &str) -> Result<WorkDetail, AppError> {
        self.repository
            .set_work_status(work_id, crate::domain::work::WorkStatus::Archived)
            .await?;
        self.get_work(work_id).await
    }

    pub async fn restore_work(&self, work_id: &str) -> Result<WorkDetail, AppError> {
        self.repository
            .set_work_status(work_id, crate::domain::work::WorkStatus::Idle)
            .await?;
        self.get_work(work_id).await
    }
}
