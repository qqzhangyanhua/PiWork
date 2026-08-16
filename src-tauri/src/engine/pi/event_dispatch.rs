use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};

use super::{CleanupBudget, EngineEvent, MAX_PRE_ACCEPTANCE_EVENTS, RunControl, wait_for_abort};

const INTERNAL_EVENT_CAPACITY: usize = MAX_PRE_ACCEPTANCE_EVENTS + 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EventDelivery {
    Aborted,
    CapacityExceeded,
    ChannelClosed,
    TimedOut,
}

pub(super) struct EventDispatcher {
    events: Option<mpsc::Sender<EngineEvent>>,
    forwarder: Option<JoinHandle<Result<(), EventDelivery>>>,
}

impl EventDispatcher {
    pub(super) fn new(sink: mpsc::Sender<EngineEvent>) -> Self {
        let (events, mut staged) = mpsc::channel(INTERNAL_EVENT_CAPACITY);
        let forwarder = tokio::spawn(async move {
            while let Some(event) = staged.recv().await {
                sink.send(event)
                    .await
                    .map_err(|_| EventDelivery::ChannelClosed)?;
            }
            Ok(())
        });
        Self {
            events: Some(events),
            forwarder: Some(forwarder),
        }
    }

    pub(super) fn stage_initial(
        &self,
        events: impl IntoIterator<Item = EngineEvent>,
    ) -> Result<(), EventDelivery> {
        let sender = self.events.as_ref().ok_or(EventDelivery::ChannelClosed)?;
        for event in events {
            sender.try_send(event).map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => EventDelivery::CapacityExceeded,
                mpsc::error::TrySendError::Closed(_) => EventDelivery::ChannelClosed,
            })?;
        }
        Ok(())
    }

    pub(super) async fn send(
        &self,
        event: EngineEvent,
        cancel: &mut watch::Receiver<RunControl>,
    ) -> Result<(), EventDelivery> {
        let sender = self.events.as_ref().ok_or(EventDelivery::ChannelClosed)?;
        tokio::select! {
            biased;
            _ = wait_for_abort(cancel) => Err(EventDelivery::Aborted),
            result = sender.send(event) => result.map_err(|_| EventDelivery::ChannelClosed),
        }
    }

    pub(super) async fn deliver_terminal(
        &mut self,
        event: EngineEvent,
        cancel: &mut watch::Receiver<RunControl>,
        observe_abort: bool,
        cleanup: CleanupBudget,
    ) -> Result<(), EventDelivery> {
        let sender = self.events.as_ref().ok_or(EventDelivery::ChannelClosed)?;
        let delivery = tokio::time::timeout_at(cleanup.deadline(), async {
            if observe_abort {
                tokio::select! {
                    biased;
                    _ = wait_for_abort(cancel) => Err(EventDelivery::Aborted),
                    result = sender.send(event) => {
                        result.map_err(|_| EventDelivery::ChannelClosed)
                    }
                }
            } else {
                sender
                    .send(event)
                    .await
                    .map_err(|_| EventDelivery::ChannelClosed)
            }
        })
        .await
        .map_err(|_| EventDelivery::TimedOut)?;

        if delivery == Err(EventDelivery::Aborted) {
            return delivery;
        }
        self.events.take();
        delivery?;
        self.wait_for_forwarder(cleanup).await
    }

    async fn wait_for_forwarder(&mut self, cleanup: CleanupBudget) -> Result<(), EventDelivery> {
        let Some(mut forwarder) = self.forwarder.take() else {
            return Err(EventDelivery::ChannelClosed);
        };
        match tokio::time::timeout_at(cleanup.deadline(), &mut forwarder).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(EventDelivery::ChannelClosed),
            Err(_) => {
                forwarder.abort();
                let _ = forwarder.await;
                Err(EventDelivery::TimedOut)
            }
        }
    }
}

impl Drop for EventDispatcher {
    fn drop(&mut self) {
        if let Some(forwarder) = self.forwarder.take() {
            forwarder.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::{mpsc, watch};

    use super::{EventDelivery, EventDispatcher};
    use crate::engine::{
        EngineEvent,
        pi::{CLEANUP_TIMEOUT, CleanupBudget, RunControl},
    };

    #[tokio::test]
    async fn terminal_delivery_uses_the_shared_deadline_when_the_sink_stays_full() {
        let (sink, _receiver) = mpsc::channel(1);
        sink.send(EngineEvent::RunStarted {
            model_label: "fixture".into(),
        })
        .await
        .unwrap();
        let mut dispatcher = EventDispatcher::new(sink);
        let (_cancel, mut cancel) = watch::channel(RunControl::Running);
        let cleanup = CleanupBudget::from_start(tokio::time::Instant::now() - CLEANUP_TIMEOUT);

        assert_eq!(
            dispatcher
                .deliver_terminal(
                    EngineEvent::RunFailed {
                        message: "finished".into(),
                    },
                    &mut cancel,
                    false,
                    cleanup,
                )
                .await,
            Err(EventDelivery::TimedOut)
        );
        assert!(cleanup.deadline() <= tokio::time::Instant::now());
        assert_eq!(CLEANUP_TIMEOUT, Duration::from_secs(4));
    }
}
