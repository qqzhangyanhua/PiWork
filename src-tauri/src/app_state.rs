use std::sync::Arc;

use crate::work::service::WorkService;

pub struct AppState {
    work_service: Arc<WorkService>,
}

impl AppState {
    pub fn new(work_service: Arc<WorkService>) -> Self {
        Self { work_service }
    }

    pub fn work_service(&self) -> &Arc<WorkService> {
        &self.work_service
    }
}
