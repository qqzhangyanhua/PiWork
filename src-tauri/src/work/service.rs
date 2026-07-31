use std::{collections::HashSet, path::PathBuf, sync::Arc};

use crate::{
    domain::work::{
        CreateWorkInput, ProjectFileSummary, StartWorkInput, StartWorkOutput, WorkDetail,
        WorkSummary,
    },
    engine::{EngineInput, supervisor::EngineSupervisor},
    error::AppError,
    resource::service::ResourceService,
};

use super::{project_files, repository::WorkRepository};

#[derive(Clone)]
pub struct WorkService {
    repository: WorkRepository,
    execution: Execution,
    resource_service: Option<Arc<ResourceService>>,
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
            resource_service: None,
        }
    }

    pub fn with_supervisor(repository: WorkRepository, supervisor: Arc<EngineSupervisor>) -> Self {
        Self {
            repository,
            execution: Execution::Supervisor(supervisor),
            resource_service: None,
        }
    }

    pub fn with_supervisor_and_resources(
        repository: WorkRepository,
        supervisor: Arc<EngineSupervisor>,
        resource_service: Arc<ResourceService>,
    ) -> Self {
        Self {
            repository,
            execution: Execution::Supervisor(supervisor),
            resource_service: Some(resource_service),
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

    pub async fn start_work(
        &self,
        work_id: &str,
        input: StartWorkInput,
    ) -> Result<StartWorkOutput, AppError> {
        let work = self.get_work(work_id).await?;
        let root_path = PathBuf::from(work.summary.root_path);
        let user_prompt = input.prompt;
        let referenced_files = input.referenced_files;
        let mut seen = HashSet::new();
        let resource_ids = input
            .resource_ids
            .into_iter()
            .filter(|resource_id| seen.insert(resource_id.clone()))
            .collect::<Vec<_>>();
        let prompt_for_context = user_prompt.clone();
        let engine_prompt = tokio::task::spawn_blocking(move || {
            project_files::build_engine_prompt(&root_path, &prompt_for_context, &referenced_files)
        })
        .await
        .map_err(|_| AppError::engine("Referenced file loading task failed"))??;
        let attachments = if resource_ids.is_empty() {
            crate::resource::service::EngineAttachments {
                images: Vec::new(),
                documents: Vec::new(),
            }
        } else {
            self.resource_service
                .as_ref()
                .ok_or_else(|| AppError::engine("Resource execution is not configured"))?
                .engine_attachments(work_id, &resource_ids)
                .await?
        };
        match &self.execution {
            Execution::Supervisor(supervisor) => {
                supervisor
                    .start_with_engine_input(
                        work_id,
                        &user_prompt,
                        resource_ids,
                        EngineInput {
                            message: engine_prompt,
                            images: attachments.images,
                            documents: attachments.documents,
                        },
                    )
                    .await
            }
            Execution::RepositoryOnly => Err(AppError::engine(
                "Work execution is not configured for this service",
            )),
        }
    }
}
