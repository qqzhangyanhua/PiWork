use std::sync::Arc;

use crate::{
    domain::work::{CreateWorkInput, RunSummary, WorkDetail, WorkSummary},
    engine::supervisor::EngineSupervisor,
    error::AppError,
};

use super::repository::WorkRepository;

#[derive(Clone)]
pub struct WorkService {
    repository: WorkRepository,
    execution: Execution,
}

#[derive(Clone)]
enum Execution {
    RepositoryOnly,
    Supervisor(Arc<EngineSupervisor>),
}

impl WorkService {
    pub fn new(repository: WorkRepository) -> Self {
        Self {
            repository,
            execution: Execution::RepositoryOnly,
        }
    }

    pub fn with_supervisor(repository: WorkRepository, supervisor: Arc<EngineSupervisor>) -> Self {
        Self {
            repository,
            execution: Execution::Supervisor(supervisor),
        }
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

    pub async fn start_work(&self, work_id: &str, prompt: &str) -> Result<RunSummary, AppError> {
        match &self.execution {
            Execution::Supervisor(supervisor) => supervisor.start(work_id, prompt).await,
            Execution::RepositoryOnly => Err(AppError::engine(
                "Work execution is not configured for this service",
            )),
        }
    }
}
