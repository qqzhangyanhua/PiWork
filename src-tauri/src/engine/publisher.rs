use async_trait::async_trait;
use tauri::Emitter;
use tokio::sync::mpsc;

use crate::{
    domain::event::WorkEventEnvelope, engine::activity_observer::ActivityObserverHandle,
    error::AppError,
};

#[async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish(&self, envelope: WorkEventEnvelope) -> Result<(), AppError>;
}

pub struct TauriEventPublisher {
    app_handle: tauri::AppHandle,
    observer: ActivityObserverHandle,
}

impl TauriEventPublisher {
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        Self::with_observer(app_handle, ActivityObserverHandle::in_process())
    }

    pub fn with_observer(app_handle: tauri::AppHandle, observer: ActivityObserverHandle) -> Self {
        Self {
            app_handle,
            observer,
        }
    }
}

#[async_trait]
impl EventPublisher for TauriEventPublisher {
    async fn publish(&self, envelope: WorkEventEnvelope) -> Result<(), AppError> {
        self.observer.emit_committed(envelope.clone());
        self.app_handle
            .emit("piwork://work-event", envelope)
            .map_err(|error| AppError::event_publish(error.to_string()))
    }
}

pub struct ChannelEventPublisher {
    sender: mpsc::Sender<WorkEventEnvelope>,
}

impl ChannelEventPublisher {
    pub fn channel(capacity: usize) -> (Self, mpsc::Receiver<WorkEventEnvelope>) {
        let (sender, receiver) = mpsc::channel(capacity);
        (Self { sender }, receiver)
    }
}

#[async_trait]
impl EventPublisher for ChannelEventPublisher {
    async fn publish(&self, envelope: WorkEventEnvelope) -> Result<(), AppError> {
        self.sender
            .send(envelope)
            .await
            .map_err(|error| AppError::event_publish(error.to_string()))
    }
}
