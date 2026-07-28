use crate::{
    domain::work::{CreateWorkInput, WorkDetail, WorkSummary},
    error::AppError,
};

use super::repository::WorkRepository;

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
}
