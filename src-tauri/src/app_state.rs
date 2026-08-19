use std::sync::Arc;

use crate::{
    agent::service::AgentService, assignment::service::AssignmentService, model::ModelService,
    resource::service::ResourceService, work::service::WorkService,
};

pub struct AppState {
    work_service: Arc<WorkService>,
    model_service: Option<Arc<ModelService>>,
    resource_service: Option<Arc<ResourceService>>,
    agent_service: Arc<AgentService>,
    assignment_service: Option<Arc<AssignmentService>>,
}

impl AppState {
    pub fn new(work_service: Arc<WorkService>, agent_service: Arc<AgentService>) -> Self {
        Self {
            work_service,
            model_service: None,
            resource_service: None,
            agent_service,
            assignment_service: None,
        }
    }

    pub fn with_model_service(
        work_service: Arc<WorkService>,
        model_service: Arc<ModelService>,
        agent_service: Arc<AgentService>,
    ) -> Self {
        Self {
            work_service,
            model_service: Some(model_service),
            resource_service: None,
            agent_service,
            assignment_service: None,
        }
    }

    pub fn with_services(
        work_service: Arc<WorkService>,
        model_service: Arc<ModelService>,
        resource_service: Arc<ResourceService>,
        agent_service: Arc<AgentService>,
    ) -> Self {
        Self {
            work_service,
            model_service: Some(model_service),
            resource_service: Some(resource_service),
            agent_service,
            assignment_service: None,
        }
    }

    pub fn with_assignment_service(mut self, assignment_service: Arc<AssignmentService>) -> Self {
        self.assignment_service = Some(assignment_service);
        self
    }

    pub fn work_service(&self) -> &Arc<WorkService> {
        &self.work_service
    }

    pub fn model_service(&self) -> &Arc<ModelService> {
        self.model_service
            .as_ref()
            .expect("production AppState must include ModelService")
    }

    pub fn resource_service(&self) -> &Arc<ResourceService> {
        self.resource_service
            .as_ref()
            .expect("production AppState must include ResourceService")
    }

    pub fn agent_service(&self) -> &Arc<AgentService> {
        &self.agent_service
    }

    pub fn assignment_service(&self) -> &Arc<AssignmentService> {
        self.assignment_service
            .as_ref()
            .expect("production AppState must include AssignmentService")
    }
}
