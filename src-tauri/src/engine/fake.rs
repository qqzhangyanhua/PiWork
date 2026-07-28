use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use tokio::sync::{Mutex, mpsc, oneshot};
use uuid::Uuid;

use super::{EngineAdapter, EngineError, EngineEvent, EngineRunContext, EngineSessionRef};

#[derive(Clone)]
pub struct FakeEngineAdapter {
    delay: Duration,
    active: Arc<Mutex<HashMap<String, oneshot::Sender<()>>>>,
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
        let run_id = context.run_id.clone();
        let mut active = self.active.lock().await;
        if active.contains_key(&run_id) {
            return Err(EngineError::Start("run is already active".into()));
        }
        active.insert(run_id.clone(), abort_sender);
        drop(active);

        let session = EngineSessionRef {
            engine_kind: self.kind().into(),
            session_id: Uuid::new_v4().to_string(),
        };
        let delay = self.delay;
        let active = Arc::clone(&self.active);
        tokio::spawn(async move {
            let result = emit_run(context, prompt, sink, delay, abort_receiver).await;
            active.lock().await.remove(&run_id);
            result
        });

        Ok(session)
    }

    async fn abort(&self, run_id: &str) -> Result<(), EngineError> {
        let sender = self
            .active
            .lock()
            .await
            .remove(run_id)
            .ok_or(EngineError::Aborted)?;
        sender.send(()).map_err(|_| EngineError::Aborted)
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
            model_label: "fake".into(),
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
            _ = &mut abort => return Err(EngineError::Aborted),
            _ = tokio::time::sleep(delay) => {}
        }
        tokio::select! {
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
}
