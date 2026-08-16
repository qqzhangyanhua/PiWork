use std::time::Duration;

use tokio::sync::{mpsc, watch};

use super::{EngineEvent, RunControl, wait_for_abort};

const TERMINAL_DELIVERY_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EventDelivery {
    Aborted,
    ChannelClosed,
    TimedOut,
}

pub(super) async fn publish_started_events(
    sink: &mpsc::Sender<EngineEvent>,
    events: impl IntoIterator<Item = EngineEvent>,
    cancel: &mut watch::Receiver<RunControl>,
) -> Result<(), EventDelivery> {
    for event in events {
        tokio::select! {
            biased;
            _ = wait_for_abort(cancel) => return Err(EventDelivery::Aborted),
            result = sink.send(event) => {
                result.map_err(|_| EventDelivery::ChannelClosed)?;
            }
        }
    }
    Ok(())
}

pub(super) async fn deliver_terminal(
    sink: &mpsc::Sender<EngineEvent>,
    event: EngineEvent,
    cancel: &mut watch::Receiver<RunControl>,
    observe_abort: bool,
) -> Result<(), EventDelivery> {
    tokio::time::timeout(TERMINAL_DELIVERY_TIMEOUT, async {
        if observe_abort {
            tokio::select! {
                biased;
                _ = wait_for_abort(cancel) => Err(EventDelivery::Aborted),
                result = sink.send(event) => result.map_err(|_| EventDelivery::ChannelClosed),
            }
        } else {
            sink.send(event)
                .await
                .map_err(|_| EventDelivery::ChannelClosed)
        }
    })
    .await
    .map_err(|_| EventDelivery::TimedOut)?
}

#[cfg(test)]
mod tests {
    use tokio::sync::{mpsc, watch};

    use super::{EventDelivery, deliver_terminal};
    use crate::engine::{EngineEvent, pi::RunControl};

    #[tokio::test]
    async fn terminal_delivery_has_a_deadline_when_the_sink_stays_full() {
        let (sink, _receiver) = mpsc::channel(1);
        sink.send(EngineEvent::RunStarted {
            model_label: "fixture".into(),
        })
        .await
        .unwrap();
        let (_cancel, mut cancel) = watch::channel(RunControl::Running);

        assert_eq!(
            deliver_terminal(
                &sink,
                EngineEvent::RunFailed {
                    message: "finished".into(),
                },
                &mut cancel,
                false,
            )
            .await,
            Err(EventDelivery::TimedOut)
        );
    }
}
