// Adapted from block/buzz at 5bf78671f45178f8de02ba18d3d321cbbf19cd1f,
// Apache-2.0. Original: crates/buzz-acp/src/observer.rs.
// PiWork changes: observes only journaled WorkEventEnvelope values; removes
// Relay, Channel, ACP session and agent-slot transport concerns.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use tokio::sync::broadcast;

use crate::domain::event::WorkEventEnvelope;

const ACTIVITY_BUFFER_CAP: usize = 1000;

#[derive(Clone)]
pub struct ActivityObserverHandle {
    inner: Arc<ActivityObserverInner>,
}

struct ActivityObserverInner {
    tx: broadcast::Sender<WorkEventEnvelope>,
    buffer: Mutex<VecDeque<WorkEventEnvelope>>,
    capacity: usize,
}

impl ActivityObserverHandle {
    pub fn in_process() -> Self {
        Self::with_capacity(ACTIVITY_BUFFER_CAP)
    }

    #[cfg(test)]
    fn in_process_with_capacity(capacity: usize) -> Self {
        Self::with_capacity(capacity)
    }

    fn with_capacity(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self {
            inner: Arc::new(ActivityObserverInner {
                tx,
                buffer: Mutex::new(VecDeque::with_capacity(capacity)),
                capacity,
            }),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WorkEventEnvelope> {
        self.inner.tx.subscribe()
    }

    pub fn snapshot(&self) -> Vec<WorkEventEnvelope> {
        let buffer = self
            .inner
            .buffer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        buffer.iter().cloned().collect()
    }

    pub fn emit_committed(&self, envelope: WorkEventEnvelope) {
        let mut buffer = self
            .inner
            .buffer
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if buffer.len() == self.inner.capacity {
            buffer.pop_front();
        }
        buffer.push_back(envelope.clone());
        let _ = self.inner.tx.send(envelope);
        drop(buffer);
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::domain::event::{LivenessState, WorkEventEnvelope, WorkEventPayload};

    use super::ActivityObserverHandle;

    fn committed_event(event_id: &str, sequence: u32) -> WorkEventEnvelope {
        WorkEventEnvelope {
            version: 2,
            event_id: Some(event_id.into()),
            work_id: "work-1".into(),
            run_id: Some("run-1".into()),
            turn_id: None,
            session_id: None,
            agent_id: None,
            assignment_id: None,
            causation_id: None,
            correlation_id: None,
            sequence,
            occurred_at: Utc::now(),
            payload: WorkEventPayload::Liveness {
                state: LivenessState::Alive,
            },
        }
    }

    #[tokio::test]
    async fn observer_replays_committed_events_and_broadcasts_live_events() {
        let observer = ActivityObserverHandle::in_process_with_capacity(2);
        let mut live = observer.subscribe();
        let event_1 = committed_event("event-1", 1);
        let event_2 = committed_event("event-2", 2);
        let event_3 = committed_event("event-3", 3);

        observer.emit_committed(event_1.clone());
        assert_eq!(live.recv().await.unwrap(), event_1);
        observer.emit_committed(event_2.clone());
        assert_eq!(live.recv().await.unwrap(), event_2);
        observer.emit_committed(event_3.clone());
        assert_eq!(live.recv().await.unwrap(), event_3);

        assert_eq!(observer.snapshot(), vec![event_2.clone(), event_3.clone()]);
    }

    #[tokio::test]
    async fn cloned_observers_share_snapshot_and_live_state() {
        let observer = ActivityObserverHandle::in_process_with_capacity(2);
        let observer_clone = observer.clone();
        let mut live = observer_clone.subscribe();
        let event = committed_event("event-1", 1);

        observer.emit_committed(event.clone());

        assert_eq!(observer_clone.snapshot(), vec![event.clone()]);
        assert_eq!(live.recv().await.unwrap(), event);
    }

    #[tokio::test]
    async fn bounded_live_stream_reports_lag_while_replay_keeps_latest_events() {
        let observer = ActivityObserverHandle::in_process_with_capacity(2);
        let mut live = observer.subscribe();
        let event_1 = committed_event("event-1", 1);
        let event_2 = committed_event("event-2", 2);
        let event_3 = committed_event("event-3", 3);

        observer.emit_committed(event_1);
        observer.emit_committed(event_2.clone());
        observer.emit_committed(event_3.clone());

        assert_eq!(
            live.recv().await.unwrap_err(),
            tokio::sync::broadcast::error::RecvError::Lagged(1)
        );
        assert_eq!(observer.snapshot(), vec![event_2, event_3]);
    }
}
