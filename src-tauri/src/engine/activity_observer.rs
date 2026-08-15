// Adapted from block/buzz at 5bf78671f45178f8de02ba18d3d321cbbf19cd1f,
// Apache-2.0. Original: crates/buzz-acp/src/observer.rs.
// PiWork changes: observes only journaled WorkEventEnvelope values; removes
// Relay, Channel, ACP session and agent-slot transport concerns.

use std::{
    collections::{HashSet, VecDeque},
    sync::{Arc, Mutex},
};

use tokio::sync::broadcast;

use crate::{
    assignment::repository::AssignmentEventSink, domain::event::WorkEventEnvelope, error::AppError,
};

const ACTIVITY_BUFFER_CAP: usize = 1000;

#[derive(Clone)]
pub struct ActivityObserverHandle {
    inner: Arc<ActivityObserverInner>,
}

struct ActivityObserverInner {
    tx: broadcast::Sender<WorkEventEnvelope>,
    state: Mutex<ActivityObserverState>,
    capacity: usize,
}

struct ActivityObserverState {
    buffer: VecDeque<WorkEventEnvelope>,
    seen_event_ids: HashSet<String>,
}

impl ActivityObserverHandle {
    pub fn in_process() -> Self {
        Self::with_capacity(ACTIVITY_BUFFER_CAP)
    }

    #[cfg(test)]
    pub(crate) fn in_process_with_capacity(capacity: usize) -> Self {
        Self::with_capacity(capacity)
    }

    fn with_capacity(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self {
            inner: Arc::new(ActivityObserverInner {
                tx,
                state: Mutex::new(ActivityObserverState {
                    buffer: VecDeque::with_capacity(capacity),
                    seen_event_ids: HashSet::new(),
                }),
                capacity,
            }),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WorkEventEnvelope> {
        self.inner.tx.subscribe()
    }

    pub fn snapshot(&self) -> Vec<WorkEventEnvelope> {
        let state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.buffer.iter().cloned().collect()
    }

    /// Returns `false` when this exact stable event id was already observed.
    pub fn emit_committed(&self, envelope: WorkEventEnvelope) -> bool {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !self.commit_locked(&mut state, &envelope) {
            return false;
        }
        drop(state);
        let _ = self.inner.tx.send(envelope);
        true
    }

    pub(crate) fn emit_committed_after<F>(
        &self,
        envelope: WorkEventEnvelope,
        emit: F,
    ) -> Result<bool, AppError>
    where
        F: FnOnce(WorkEventEnvelope) -> Result<(), AppError>,
    {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if envelope
            .event_id
            .as_ref()
            .is_some_and(|event_id| state.seen_event_ids.contains(event_id))
        {
            return Ok(false);
        }
        emit(envelope.clone())?;
        let committed = self.commit_locked(&mut state, &envelope);
        debug_assert!(
            committed,
            "observer state is locked across frontend emission"
        );
        drop(state);
        let _ = self.inner.tx.send(envelope);
        Ok(true)
    }

    fn commit_locked(
        &self,
        state: &mut ActivityObserverState,
        envelope: &WorkEventEnvelope,
    ) -> bool {
        if let Some(event_id) = envelope.event_id.as_ref()
            && !state.seen_event_ids.insert(event_id.clone())
        {
            return false;
        }
        if state.buffer.len() == self.inner.capacity
            && let Some(evicted) = state.buffer.pop_front()
            && let Some(event_id) = evicted.event_id
        {
            state.seen_event_ids.remove(&event_id);
        }
        state.buffer.push_back(envelope.clone());
        true
    }
}

impl AssignmentEventSink for ActivityObserverHandle {
    fn publish(&self, event: WorkEventEnvelope) -> Result<(), AppError> {
        self.emit_committed(event);
        Ok(())
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
    async fn observer_uses_event_id_as_an_idempotency_key() {
        let observer = ActivityObserverHandle::in_process_with_capacity(2);
        let mut live = observer.subscribe();
        let event = committed_event("event-once", 1);

        observer.emit_committed(event.clone());
        assert_eq!(live.recv().await.unwrap(), event);
        observer.emit_committed(event.clone());

        assert_eq!(observer.snapshot(), vec![event]);
        assert!(matches!(
            live.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));
    }

    #[test]
    fn observer_evicts_seen_ids_with_the_bounded_replay_buffer() {
        let observer = ActivityObserverHandle::in_process_with_capacity(2);
        let event_1 = committed_event("event-1", 1);
        let event_2 = committed_event("event-2", 2);
        let event_3 = committed_event("event-3", 3);

        assert!(observer.emit_committed(event_1.clone()));
        assert!(observer.emit_committed(event_2.clone()));
        assert!(observer.emit_committed(event_3.clone()));
        assert_eq!(observer.inner.state.lock().unwrap().seen_event_ids.len(), 2);
        assert!(!observer.emit_committed(event_2.clone()));
        assert!(observer.emit_committed(event_1.clone()));
        assert_eq!(observer.snapshot(), vec![event_3, event_1]);
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
