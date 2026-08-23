use std::sync::Arc;

use crate::{
    agent::service::AgentService, assignment::service::AssignmentService,
    connectors::ConnectorService, execution::ExecutionCoordinator, extensions::ExtensionService,
    memory::WorkspaceMemoryService, model::ModelService, resource::service::ResourceService,
    work::service::WorkService,
};

pub struct AppState {
    work_service: Arc<WorkService>,
    model_service: Option<Arc<ModelService>>,
    resource_service: Option<Arc<ResourceService>>,
    agent_service: Arc<AgentService>,
    assignment_service: Option<Arc<AssignmentService>>,
    execution_coordinator: Option<Arc<ExecutionCoordinator>>,
    extension_service: Option<Arc<ExtensionService>>,
    connector_service: Option<Arc<ConnectorService>>,
    memory_service: Option<Arc<WorkspaceMemoryService>>,
}

impl AppState {
    pub fn new(work_service: Arc<WorkService>, agent_service: Arc<AgentService>) -> Self {
        Self {
            work_service,
            model_service: None,
            resource_service: None,
            agent_service,
            assignment_service: None,
            execution_coordinator: None,
            extension_service: None,
            connector_service: None,
            memory_service: None,
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
            execution_coordinator: None,
            extension_service: None,
            connector_service: None,
            memory_service: None,
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
            execution_coordinator: None,
            extension_service: None,
            connector_service: None,
            memory_service: None,
        }
    }

    pub fn with_assignment_service(mut self, assignment_service: Arc<AssignmentService>) -> Self {
        self.assignment_service = Some(assignment_service);
        self
    }

    pub fn with_execution_coordinator(
        mut self,
        execution_coordinator: Arc<ExecutionCoordinator>,
    ) -> Self {
        self.execution_coordinator = Some(execution_coordinator);
        self
    }

    pub fn with_extension_service(mut self, extension_service: Arc<ExtensionService>) -> Self {
        self.extension_service = Some(extension_service);
        self
    }

    pub fn with_connector_service(mut self, connector_service: Arc<ConnectorService>) -> Self {
        self.connector_service = Some(connector_service);
        self
    }

    pub fn with_memory_service(mut self, memory_service: Arc<WorkspaceMemoryService>) -> Self {
        self.memory_service = Some(memory_service);
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

    pub fn execution_coordinator(&self) -> &Arc<ExecutionCoordinator> {
        self.execution_coordinator
            .as_ref()
            .expect("production AppState must include ExecutionCoordinator")
    }

    pub fn extension_service(&self) -> &Arc<ExtensionService> {
        self.extension_service
            .as_ref()
            .expect("production AppState must include ExtensionService")
    }

    pub fn connector_service(&self) -> &Arc<ConnectorService> {
        self.connector_service
            .as_ref()
            .expect("production AppState must include ConnectorService")
    }

    pub fn memory_service(&self) -> &Arc<WorkspaceMemoryService> {
        self.memory_service
            .as_ref()
            .expect("production AppState must include WorkspaceMemoryService")
    }
}
