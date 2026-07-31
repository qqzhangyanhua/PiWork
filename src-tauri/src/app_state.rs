use std::sync::Arc;

use crate::{model::ModelService, resource::service::ResourceService, work::service::WorkService};

pub struct AppState {
    work_service: Arc<WorkService>,
    model_service: Option<Arc<ModelService>>,
    resource_service: Option<Arc<ResourceService>>,
}

impl AppState {
    pub fn new(work_service: Arc<WorkService>) -> Self {
        Self {
            work_service,
            model_service: None,
            resource_service: None,
        }
    }

    pub fn with_model_service(
        work_service: Arc<WorkService>,
        model_service: Arc<ModelService>,
    ) -> Self {
        Self {
            work_service,
            model_service: Some(model_service),
            resource_service: None,
        }
    }

    pub fn with_services(
        work_service: Arc<WorkService>,
        model_service: Arc<ModelService>,
        resource_service: Arc<ResourceService>,
    ) -> Self {
        Self {
            work_service,
            model_service: Some(model_service),
            resource_service: Some(resource_service),
        }
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
}
