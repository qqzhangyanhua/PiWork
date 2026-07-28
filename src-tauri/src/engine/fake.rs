use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use tokio::sync::{Mutex, mpsc, oneshot};
use uuid::Uuid;

use super::{EngineAdapter, EngineError, EngineEvent, EngineRunContext, EngineSessionRef};

#[derive(Clone)]
pub struct FakeEngineAdapter {
    delay: Duration,
    active: Arc<Mutex<HashMap<String, ActiveRun>>>,
}

struct ActiveRun {
    generation: Uuid,
    cancel: oneshot::Sender<()>,
    completion: oneshot::Receiver<FakeTaskOutcome>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FakeTaskOutcome {
    Completed,
    Aborted,
    ChannelClosed,
}

impl FakeEngineAdapter {
    pub fn new(delay: Duration) -> Self {
        Self {
            delay,
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl EngineAdapter for FakeEngineAdapter {
    fn kind(&self) -> &'static str {
        "fake"
    }

    async fn start(
        &self,
        context: EngineRunContext,
        prompt: String,
        sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        let (abort_sender, abort_receiver) = oneshot::channel();
        let (completion_sender, completion_receiver) = oneshot::channel();
        let run_id = context.run_id.clone();
        let generation = Uuid::new_v4();
        let mut active = self.active.lock().await;
        if active.contains_key(&run_id) {
            return Err(EngineError::Start("run is already active".into()));
        }
        active.insert(
            run_id.clone(),
            ActiveRun {
                generation,
                cancel: abort_sender,
                completion: completion_receiver,
            },
        );
        drop(active);

        let session = EngineSessionRef {
            engine_kind: self.kind().into(),
            session_id: Uuid::new_v4().to_string(),
        };
        let delay = self.delay;
        let active = Arc::clone(&self.active);
        tokio::spawn(async move {
            let result = emit_run(context, prompt, sink, delay, abort_receiver).await;
            let outcome = match result {
                Ok(()) => FakeTaskOutcome::Completed,
                Err(EngineError::Aborted) => FakeTaskOutcome::Aborted,
                Err(EngineError::ChannelClosed) | Err(EngineError::Start(_)) => {
                    FakeTaskOutcome::ChannelClosed
                }
            };
            let _ = completion_sender.send(outcome);
            let mut active = active.lock().await;
            if active
                .get(&run_id)
                .is_some_and(|entry| entry.generation == generation)
            {
                active.remove(&run_id);
            }
        });

        Ok(session)
    }

    async fn abort(&self, run_id: &str) -> Result<(), EngineError> {
        let active = self
            .active
            .lock()
            .await
            .remove(run_id)
            .ok_or(EngineError::Aborted)?;
        let _ = active.cancel.send(());
        match active.completion.await.map_err(|_| EngineError::Aborted)? {
            FakeTaskOutcome::Aborted => Ok(()),
            FakeTaskOutcome::Completed | FakeTaskOutcome::ChannelClosed => {
                Err(EngineError::Aborted)
            }
        }
    }
}

async fn emit_run(
    context: EngineRunContext,
    prompt: String,
    sink: mpsc::Sender<EngineEvent>,
    delay: Duration,
    mut abort: oneshot::Receiver<()>,
) -> Result<(), EngineError> {
    let events = [
        EngineEvent::RunStarted {
            model_label: "Fake model".into(),
        },
        EngineEvent::AssistantDelta {
            text: format!("Working on: {prompt}"),
        },
        EngineEvent::ToolStarted {
            tool_call_id: format!("{}-tool-1", context.run_id),
            tool_name: "fake_tool".into(),
            input_summary: "Inspect the workspace".into(),
        },
        EngineEvent::ToolFinished {
            tool_call_id: format!("{}-tool-1", context.run_id),
            tool_name: "fake_tool".into(),
            output_summary: "Workspace inspected".into(),
            success: true,
        },
        EngineEvent::AssistantDelta {
            text: "The requested work is complete.".into(),
        },
        EngineEvent::RunCompleted {
            summary: "Completed by the deterministic fake engine".into(),
            artifacts: Vec::new(),
            validation: vec!["fake validation passed".into()],
            limitations: vec!["fake engine only".into()],
        },
    ];

    for event in events {
        tokio::select! {
            biased;
            _ = &mut abort => return Err(EngineError::Aborted),
            _ = tokio::time::sleep(delay) => {}
        }
        tokio::select! {
            biased;
            _ = &mut abort => return Err(EngineError::Aborted),
            result = sink.send(event) => result.map_err(|_| EngineError::ChannelClosed)?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::mpsc;

    use super::FakeEngineAdapter;
    use crate::engine::{EngineAdapter, EngineRunContext};

    #[tokio::test]
    async fn fake_engine_emits_a_complete_ordered_run() {
        let (sender, mut receiver) = mpsc::channel(16);
        let engine = FakeEngineAdapter::new(Duration::ZERO);
        let context = EngineRunContext::test("work-1", "run-1");

        let session = engine
            .start(context, "Build it".into(), sender)
            .await
            .unwrap();

        assert_eq!(session.engine_kind, "fake");
        let mut kinds = Vec::new();
        while let Some(event) = receiver.recv().await {
            kinds.push(event.kind());
            if event.is_terminal() {
                break;
            }
        }

        assert_eq!(
            kinds,
            [
                "run_started",
                "assistant_delta",
                "tool_started",
                "tool_finished",
                "assistant_delta",
                "run_completed",
            ]
        );
    }

    #[tokio::test]
    async fn abort_waits_for_stop_and_old_cleanup_cannot_remove_a_restarted_run() {
        let engine = FakeEngineAdapter::new(Duration::from_millis(40));
        let context = EngineRunContext::test("work-1", "run-1");
        let (first_sender, mut first_receiver) = mpsc::channel(16);
        engine
            .start(context.clone(), "First".into(), first_sender)
            .await
            .unwrap();
        assert_eq!(first_receiver.recv().await.unwrap().kind(), "run_started");

        engine.abort("run-1").await.unwrap();
        assert!(first_receiver.is_closed());
        while first_receiver.try_recv().is_ok() {}
        tokio::time::sleep(Duration::from_millis(90)).await;
        assert!(first_receiver.try_recv().is_err());

        let (second_sender, _second_receiver) = mpsc::channel(16);
        engine
            .start(context, "Second".into(), second_sender)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        engine.abort("run-1").await.unwrap();

        assert!(engine.abort("unknown-run").await.is_err());
    }
}
