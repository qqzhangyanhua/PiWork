use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use tokio::sync::{Mutex, mpsc, oneshot};
use uuid::Uuid;

#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(test)]
use tokio::sync::Notify;

use super::{EngineAdapter, EngineError, EngineEvent, EngineRunContext, EngineSessionRef};

#[derive(Clone)]
pub struct FakeEngineAdapter {
    delay: Duration,
    active: Arc<Mutex<HashMap<String, ActiveRun>>>,
    #[cfg(test)]
    completion_gate: Option<Arc<FakeCompletionGate>>,
}

struct ActiveRun {
    generation: Uuid,
    state: FakeRunState,
    cancel: Option<oneshot::Sender<()>>,
    completion: Option<oneshot::Receiver<FakeTaskOutcome>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FakeRunState {
    Running,
    Cancelling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FakeTaskOutcome {
    Completed,
    Aborted,
    ChannelClosed,
}

#[cfg(test)]
struct FakeCompletionGate {
    armed: AtomicBool,
    reached: Notify,
    release: Notify,
}

#[cfg(test)]
impl FakeCompletionGate {
    fn new() -> Self {
        Self {
            armed: AtomicBool::new(true),
            reached: Notify::new(),
            release: Notify::new(),
        }
    }

    async fn pause_if_armed(&self) {
        if self.armed.swap(false, Ordering::SeqCst) {
            self.reached.notify_one();
            self.release.notified().await;
        }
    }

    async fn wait_until_reached(&self) {
        tokio::time::timeout(Duration::from_secs(1), self.reached.notified())
            .await
            .expect("fake engine did not reach its completion gate");
    }

    fn release(&self) {
        self.release.notify_one();
    }
}

impl FakeEngineAdapter {
    pub fn new(delay: Duration) -> Self {
        Self {
            delay,
            active: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(test)]
            completion_gate: None,
        }
    }

    #[cfg(test)]
    fn new_with_paused_first_completion(delay: Duration) -> (Self, Arc<FakeCompletionGate>) {
        let completion_gate = Arc::new(FakeCompletionGate::new());
        (
            Self {
                delay,
                active: Arc::new(Mutex::new(HashMap::new())),
                completion_gate: Some(Arc::clone(&completion_gate)),
            },
            completion_gate,
        )
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
                state: FakeRunState::Running,
                cancel: Some(abort_sender),
                completion: Some(completion_receiver),
            },
        );
        drop(active);

        let session = EngineSessionRef {
            engine_kind: self.kind().into(),
            session_id: Uuid::new_v4().to_string(),
        };
        let delay = self.delay;
        let active = Arc::clone(&self.active);
        #[cfg(test)]
        let completion_gate = self.completion_gate.clone();
        tokio::spawn(async move {
            let result = emit_run(context, prompt, sink, delay, abort_receiver).await;
            let outcome = match result {
                Ok(()) => FakeTaskOutcome::Completed,
                Err(EngineError::Aborted) => FakeTaskOutcome::Aborted,
                Err(EngineError::ChannelClosed)
                | Err(EngineError::NotRunning)
                | Err(EngineError::Start(_)) => FakeTaskOutcome::ChannelClosed,
            };
            #[cfg(test)]
            if let Some(completion_gate) = completion_gate {
                completion_gate.pause_if_armed().await;
            }
            let _ = completion_sender.send(outcome);
            let mut active = active.lock().await;
            if active.get(&run_id).is_some_and(|entry| {
                entry.generation == generation && entry.state == FakeRunState::Running
            }) {
                active.remove(&run_id);
            }
        });

        Ok(session)
    }

    async fn abort(&self, run_id: &str) -> Result<(), EngineError> {
        let (generation, cancel, completion) = {
            let mut active_runs = self.active.lock().await;
            let active = active_runs.get_mut(run_id).ok_or(EngineError::NotRunning)?;
            if active.state == FakeRunState::Cancelling {
                return Err(EngineError::Aborted);
            }
            active.state = FakeRunState::Cancelling;
            let cancel = active.cancel.take().ok_or(EngineError::Aborted)?;
            let completion = active.completion.take().ok_or(EngineError::Aborted)?;
            (active.generation, cancel, completion)
        };

        let _ = cancel.send(());
        let outcome = completion.await;
        let mut active_runs = self.active.lock().await;
        if active_runs
            .get(run_id)
            .is_some_and(|active| active.generation == generation)
        {
            active_runs.remove(run_id);
        }
        drop(active_runs);

        match outcome.map_err(|_| EngineError::Aborted)? {
            FakeTaskOutcome::Aborted => Ok(()),
            FakeTaskOutcome::Completed | FakeTaskOutcome::ChannelClosed => {
                Err(EngineError::NotRunning)
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
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if !engine.active.lock().await.contains_key("run-1") {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("naturally completed fake run remained active");
        assert!(matches!(
            engine.abort("run-1").await,
            Err(crate::engine::EngineError::NotRunning)
        ));
    }

    #[tokio::test]
    async fn start_rejects_a_run_while_its_previous_generation_is_cancelling() {
        let (engine, completion_gate) =
            FakeEngineAdapter::new_with_paused_first_completion(Duration::from_secs(60));
        let context = EngineRunContext::test("work-1", "run-1");
        let (first_sender, _first_receiver) = mpsc::channel(16);
        engine
            .start(context.clone(), "First".into(), first_sender)
            .await
            .unwrap();

        let abort_engine = engine.clone();
        let abort = tokio::spawn(async move { abort_engine.abort("run-1").await });
        completion_gate.wait_until_reached().await;

        let (overlap_sender, _overlap_receiver) = mpsc::channel(16);
        let overlap = engine
            .start(context.clone(), "Overlapping".into(), overlap_sender)
            .await;
        assert!(matches!(overlap, Err(crate::engine::EngineError::Start(_))));

        completion_gate.release();
        abort.await.unwrap().unwrap();

        let (second_sender, _second_receiver) = mpsc::channel(16);
        engine
            .start(context, "Second".into(), second_sender)
            .await
            .unwrap();
        engine.abort("run-1").await.unwrap();
    }

    #[tokio::test]
    async fn abort_reports_not_running_when_the_producer_already_completed() {
        let (engine, completion_gate) =
            FakeEngineAdapter::new_with_paused_first_completion(Duration::ZERO);
        let context = EngineRunContext::test("work-1", "run-1");
        let (sender, mut receiver) = mpsc::channel(16);
        engine
            .start(context, "Complete first".into(), sender)
            .await
            .unwrap();
        while let Some(event) = receiver.recv().await {
            if event.is_terminal() {
                break;
            }
        }
        completion_gate.wait_until_reached().await;

        let abort_engine = engine.clone();
        let abort = tokio::spawn(async move { abort_engine.abort("run-1").await });
        completion_gate.release();

        assert!(matches!(
            abort.await.unwrap(),
            Err(crate::engine::EngineError::NotRunning)
        ));
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

        assert!(matches!(
            engine.abort("unknown-run").await,
            Err(crate::engine::EngineError::NotRunning)
        ));
    }
}
