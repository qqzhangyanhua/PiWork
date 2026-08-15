use async_trait::async_trait;
use tauri::Emitter;
use tokio::sync::mpsc;

use crate::{
    assignment::repository::AssignmentEventSink, domain::event::WorkEventEnvelope,
    engine::activity_observer::ActivityObserverHandle, error::AppError,
};

#[async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish(&self, envelope: WorkEventEnvelope) -> Result<(), AppError>;
}

fn publish_if_new<F>(
    observer: &ActivityObserverHandle,
    envelope: WorkEventEnvelope,
    emit: F,
) -> Result<(), AppError>
where
    F: FnOnce(WorkEventEnvelope) -> Result<(), AppError>,
{
    observer.emit_committed_after(envelope, emit).map(|_| ())
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

    fn publish_now(&self, envelope: WorkEventEnvelope) -> Result<(), AppError> {
        publish_if_new(&self.observer, envelope, |event| {
            self.app_handle
                .emit("piwork://work-event", event)
                .map_err(|error| AppError::event_publish(error.to_string()))
        })
    }
}

#[async_trait]
impl EventPublisher for TauriEventPublisher {
    async fn publish(&self, envelope: WorkEventEnvelope) -> Result<(), AppError> {
        self.publish_now(envelope)
    }
}

impl AssignmentEventSink for TauriEventPublisher {
    fn publish(&self, event: WorkEventEnvelope) -> Result<(), AppError> {
        self.publish_now(event)
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

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex, mpsc},
        time::Duration,
    };

    use chrono::Utc;

    use crate::{
        domain::event::{LivenessState, WorkEventEnvelope, WorkEventPayload},
        engine::activity_observer::ActivityObserverHandle,
    };

    use super::publish_if_new;

    fn committed_event() -> WorkEventEnvelope {
        WorkEventEnvelope {
            version: 2,
            event_id: Some("publisher-event-once".into()),
            work_id: "work-1".into(),
            run_id: Some("run-1".into()),
            turn_id: None,
            session_id: None,
            agent_id: None,
            assignment_id: None,
            causation_id: None,
            correlation_id: None,
            sequence: 1,
            occurred_at: Utc::now(),
            payload: WorkEventPayload::Liveness {
                state: LivenessState::Alive,
            },
        }
    }

    #[test]
    fn production_publisher_does_not_emit_a_duplicate_stable_event_id() {
        let observer = ActivityObserverHandle::in_process();
        let emitted = Mutex::new(Vec::new());
        let event = committed_event();

        for candidate in [event.clone(), event] {
            publish_if_new(&observer, candidate, |event| {
                emitted.lock().unwrap().push(event);
                Ok(())
            })
            .unwrap();
        }

        assert_eq!(emitted.lock().unwrap().len(), 1);
    }

    #[test]
    fn production_publisher_retries_the_same_event_id_after_frontend_emit_failure() {
        let observer = ActivityObserverHandle::in_process();
        let attempts = Mutex::new(0usize);
        let event = committed_event();

        let first = publish_if_new(&observer, event.clone(), |_| {
            *attempts.lock().unwrap() += 1;
            Err(crate::error::AppError::event_publish("forced emit failure"))
        });
        assert!(first.is_err());
        publish_if_new(&observer, event, |_| {
            *attempts.lock().unwrap() += 1;
            Ok(())
        })
        .unwrap();

        assert_eq!(*attempts.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn failed_frontend_emit_is_not_broadcast_or_replayed_before_retry_succeeds() {
        let observer = ActivityObserverHandle::in_process();
        let mut live = observer.subscribe();
        let event = committed_event();

        assert!(
            publish_if_new(&observer, event.clone(), |_| {
                Err(crate::error::AppError::event_publish("forced emit failure"))
            })
            .is_err()
        );
        assert!(observer.snapshot().is_empty());
        assert!(matches!(
            live.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));

        publish_if_new(&observer, event.clone(), |_| Ok(())).unwrap();
        assert_eq!(live.recv().await.unwrap(), event.clone());
        assert_eq!(observer.snapshot(), vec![event]);
        assert!(matches!(
            live.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));
    }

    #[test]
    fn failed_frontend_emit_does_not_evict_the_bounded_replay_buffer() {
        let observer = ActivityObserverHandle::in_process_with_capacity(2);
        let mut event_1 = committed_event();
        event_1.event_id = Some("publisher-event-1".into());
        let mut event_2 = committed_event();
        event_2.event_id = Some("publisher-event-2".into());
        let mut event_3 = committed_event();
        event_3.event_id = Some("publisher-event-3".into());

        publish_if_new(&observer, event_1.clone(), |_| Ok(())).unwrap();
        publish_if_new(&observer, event_2.clone(), |_| Ok(())).unwrap();
        assert!(
            publish_if_new(&observer, event_3.clone(), |_| {
                Err(crate::error::AppError::event_publish("forced emit failure"))
            })
            .is_err()
        );
        assert_eq!(observer.snapshot(), vec![event_1, event_2.clone()]);

        publish_if_new(&observer, event_3.clone(), |_| Ok(())).unwrap();
        assert_eq!(observer.snapshot(), vec![event_2, event_3]);
    }

    #[test]
    fn concurrent_duplicate_waits_for_failed_emit_then_retries_itself() {
        let observer = Arc::new(ActivityObserverHandle::in_process());
        let event = committed_event();
        let (first_started_tx, first_started_rx) = mpsc::channel();
        let (release_first_tx, release_first_rx) = mpsc::channel();
        let first_observer = observer.clone();
        let first_event = event.clone();
        let first = std::thread::spawn(move || {
            publish_if_new(&first_observer, first_event, |_| {
                first_started_tx.send(()).unwrap();
                release_first_rx.recv().unwrap();
                Err(crate::error::AppError::event_publish("forced emit failure"))
            })
        });
        first_started_rx.recv().unwrap();

        let (second_finished_tx, second_finished_rx) = mpsc::channel();
        let second_observer = observer.clone();
        let second = std::thread::spawn(move || {
            let result = publish_if_new(&second_observer, event, |_| Ok(()));
            second_finished_tx.send(()).unwrap();
            result
        });
        assert!(
            second_finished_rx
                .recv_timeout(Duration::from_millis(50))
                .is_err()
        );

        release_first_tx.send(()).unwrap();
        assert!(first.join().unwrap().is_err());
        assert!(second.join().unwrap().is_ok());
        assert_eq!(observer.snapshot().len(), 1);
    }
}
