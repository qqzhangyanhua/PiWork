use std::{collections::HashMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use tokio::sync::{Mutex, Notify, Semaphore, mpsc, oneshot};
use uuid::Uuid;

use super::{
    EngineAdapter, EngineCapabilities, EngineError, EngineEvent, EngineInput, EngineRunContext,
    EngineSessionRef,
};
use crate::domain::event::{LivenessState, PermissionOutcome, SessionTransition};
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FakeRunBehavior {
    Complete,
    HoldUntilAbort,
    Crash,
    LivenessTimeout,
    DuplicateTerminal,
}

#[derive(Clone)]
pub struct FakeStartBarrier {
    entered: Arc<Semaphore>,
    release: Arc<Semaphore>,
}

impl Default for FakeStartBarrier {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeStartBarrier {
    pub fn new() -> Self {
        Self {
            entered: Arc::new(Semaphore::new(0)),
            release: Arc::new(Semaphore::new(0)),
        }
    }

    async fn pause(&self) {
        self.entered.add_permits(1);
        self.release
            .acquire()
            .await
            .expect("fake start barrier was closed")
            .forget();
    }

    pub async fn wait_until_entered(&self) {
        self.entered
            .acquire()
            .await
            .expect("fake start barrier was closed")
            .forget();
    }

    pub fn release(&self) {
        self.release.add_permits(1);
    }
}

#[derive(Clone)]
pub struct FakeEngineConfig {
    capabilities: EngineCapabilities,
    session_id: Option<String>,
    run_behavior: FakeRunBehavior,
    start_barrier: Option<Arc<FakeStartBarrier>>,
}

impl FakeEngineConfig {
    pub fn new(capabilities: EngineCapabilities) -> Self {
        Self {
            capabilities,
            session_id: None,
            run_behavior: FakeRunBehavior::Complete,
            start_barrier: None,
        }
    }

    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    pub fn with_run_behavior(mut self, run_behavior: FakeRunBehavior) -> Self {
        self.run_behavior = run_behavior;
        self
    }

    pub fn with_start_barrier(mut self, start_barrier: Arc<FakeStartBarrier>) -> Self {
        self.start_barrier = Some(start_barrier);
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FakeEngineObservations {
    pub started_run_ids: Vec<String>,
    pub resumed_run_ids: Vec<String>,
    pub rotations: Vec<(String, String)>,
    pub steers: Vec<(String, String)>,
    pub aborted_run_ids: Vec<String>,
}

#[derive(Clone)]
pub struct FakeEngineAdapter {
    delay: Duration,
    active: Arc<Mutex<HashMap<String, ActiveRun>>>,
    capabilities: EngineCapabilities,
    run_behavior: FakeRunBehavior,
    start_barrier: Option<Arc<FakeStartBarrier>>,
    session_id: Option<String>,
    observations: Arc<Mutex<FakeEngineObservations>>,
    #[cfg(test)]
    completion_gate: Option<Arc<FakeCompletionGate>>,
}

struct ActiveRun {
    generation: Uuid,
    state: FakeRunState,
    cancel: Option<oneshot::Sender<()>>,
    completion: Arc<FakeRunCompletion>,
}

struct FakeRunRequest {
    context: EngineRunContext,
    prompt: String,
    capabilities: EngineCapabilities,
    behavior: FakeRunBehavior,
    transition: Option<SessionTransition>,
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

struct FakeRunCompletion {
    outcome: std::sync::Mutex<Option<FakeTaskOutcome>>,
    changed: Notify,
}

impl FakeRunCompletion {
    fn new() -> Self {
        Self {
            outcome: std::sync::Mutex::new(None),
            changed: Notify::new(),
        }
    }

    fn complete(&self, outcome: FakeTaskOutcome) {
        let mut current = self.outcome.lock().unwrap();
        if current.is_none() {
            *current = Some(outcome);
            drop(current);
            self.changed.notify_waiters();
        }
    }

    async fn wait(&self) -> FakeTaskOutcome {
        loop {
            let changed = self.changed.notified();
            if let Some(outcome) = *self.outcome.lock().unwrap() {
                return outcome;
            }
            changed.await;
        }
    }
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
        let capabilities = EngineCapabilities {
            cancel: true,
            thought_stream: true,
            plan_updates: true,
            ..EngineCapabilities::default()
        };
        Self {
            delay,
            active: Arc::new(Mutex::new(HashMap::new())),
            capabilities,
            run_behavior: FakeRunBehavior::Complete,
            start_barrier: None,
            session_id: None,
            observations: Arc::new(Mutex::new(FakeEngineObservations::default())),
            #[cfg(test)]
            completion_gate: None,
        }
    }

    pub fn configured(config: FakeEngineConfig) -> Self {
        Self {
            delay: Duration::ZERO,
            active: Arc::new(Mutex::new(HashMap::new())),
            capabilities: config.capabilities,
            run_behavior: config.run_behavior,
            start_barrier: config.start_barrier,
            session_id: config.session_id,
            observations: Arc::new(Mutex::new(FakeEngineObservations::default())),
            #[cfg(test)]
            completion_gate: None,
        }
    }

    pub async fn observations(&self) -> FakeEngineObservations {
        self.observations.lock().await.clone()
    }

    #[cfg(test)]
    pub(crate) fn new_with_session(delay: Duration, session_id: &str) -> Self {
        let mut adapter = Self::new(delay);
        adapter.session_id = Some(session_id.to_owned());
        adapter
    }

    #[cfg(test)]
    fn new_with_paused_first_completion(delay: Duration) -> (Self, Arc<FakeCompletionGate>) {
        let completion_gate = Arc::new(FakeCompletionGate::new());
        (
            Self {
                delay,
                active: Arc::new(Mutex::new(HashMap::new())),
                capabilities: EngineCapabilities {
                    cancel: true,
                    thought_stream: true,
                    plan_updates: true,
                    ..EngineCapabilities::default()
                },
                run_behavior: FakeRunBehavior::Complete,
                start_barrier: None,
                session_id: None,
                observations: Arc::new(Mutex::new(FakeEngineObservations::default())),
                completion_gate: Some(Arc::clone(&completion_gate)),
            },
            completion_gate,
        )
    }

    async fn begin(
        &self,
        context: EngineRunContext,
        input: EngineInput,
        sink: mpsc::Sender<EngineEvent>,
        transition: Option<SessionTransition>,
    ) -> Result<EngineSessionRef, EngineError> {
        if let Some(start_barrier) = &self.start_barrier {
            start_barrier.pause().await;
        }

        let (abort_sender, abort_receiver) = oneshot::channel();
        let completion = Arc::new(FakeRunCompletion::new());
        let run_id = context.run_id().to_owned();
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
                completion: Arc::clone(&completion),
            },
        );
        drop(active);

        let session = EngineSessionRef {
            engine_kind: self.kind().into(),
            session_id: self
                .session_id
                .clone()
                .unwrap_or_else(|| Uuid::new_v4().to_string()),
        };
        {
            let mut observations = self.observations.lock().await;
            if transition == Some(SessionTransition::Resumed) {
                observations.resumed_run_ids.push(run_id.clone());
            } else {
                observations.started_run_ids.push(run_id.clone());
            }
        }

        let delay = self.delay;
        let active = Arc::clone(&self.active);
        let capabilities = self.capabilities;
        let run_behavior = self.run_behavior;
        #[cfg(test)]
        let completion_gate = self.completion_gate.clone();
        tokio::spawn(async move {
            let result = emit_run(
                FakeRunRequest {
                    context,
                    prompt: input.message,
                    capabilities,
                    behavior: run_behavior,
                    transition,
                },
                sink,
                delay,
                abort_receiver,
            )
            .await;
            let outcome = match result {
                Ok(()) => FakeTaskOutcome::Completed,
                Err(EngineError::Aborted) => FakeTaskOutcome::Aborted,
                Err(EngineError::ChannelClosed)
                | Err(EngineError::NotRunning)
                | Err(EngineError::Start(_))
                | Err(EngineError::Unsupported(_)) => FakeTaskOutcome::ChannelClosed,
            };
            #[cfg(test)]
            if let Some(completion_gate) = completion_gate {
                completion_gate.pause_if_armed().await;
            }
            let mut active = active.lock().await;
            if active
                .get(&run_id)
                .is_some_and(|entry| entry.generation == generation)
            {
                active.remove(&run_id);
            }
            drop(active);
            completion.complete(outcome);
        });

        Ok(session)
    }
}

#[async_trait]
impl EngineAdapter for FakeEngineAdapter {
    fn kind(&self) -> &'static str {
        "fake"
    }

    fn capabilities(&self) -> EngineCapabilities {
        self.capabilities
    }

    async fn start(
        &self,
        context: EngineRunContext,
        input: EngineInput,
        sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        self.begin(context, input, sink, None).await
    }

    async fn resume(
        &self,
        context: EngineRunContext,
        input: EngineInput,
        sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        if !self.capabilities.session_resume {
            return Err(EngineError::Unsupported("session_resume"));
        }
        self.begin(context, input, sink, Some(SessionTransition::Resumed))
            .await
    }

    async fn rotate(
        &self,
        context: EngineRunContext,
        reason: &str,
    ) -> Result<EngineSessionRef, EngineError> {
        if !self.capabilities.session_rotate {
            return Err(EngineError::Unsupported("session_rotate"));
        }
        self.observations
            .lock()
            .await
            .rotations
            .push((context.run_id().to_owned(), reason.to_owned()));
        Ok(EngineSessionRef {
            engine_kind: self.kind().into(),
            session_id: format!(
                "{}-generation-{}",
                self.session_id.as_deref().unwrap_or("fake-session"),
                context.session_generation().saturating_add(1)
            ),
        })
    }

    async fn steer(&self, run_id: &str, input: EngineInput) -> Result<(), EngineError> {
        if !self.capabilities.native_steer {
            return Err(EngineError::Unsupported("native_steer"));
        }
        if !self.active.lock().await.contains_key(run_id) {
            return Err(EngineError::NotRunning);
        }
        self.observations
            .lock()
            .await
            .steers
            .push((run_id.to_owned(), input.message));
        Ok(())
    }

    async fn abort(&self, run_id: &str) -> Result<(), EngineError> {
        if !self.capabilities.cancel {
            return Err(EngineError::Unsupported("cancel"));
        }
        let (cancel, completion) = {
            let mut active_runs = self.active.lock().await;
            let active = active_runs.get_mut(run_id).ok_or(EngineError::NotRunning)?;
            if active.state == FakeRunState::Cancelling {
                return Err(EngineError::Aborted);
            }
            active.state = FakeRunState::Cancelling;
            let cancel = active.cancel.take().ok_or(EngineError::Aborted)?;
            (cancel, Arc::clone(&active.completion))
        };
        self.observations
            .lock()
            .await
            .aborted_run_ids
            .push(run_id.to_owned());

        let _ = cancel.send(());
        let outcome = completion.wait().await;

        match outcome {
            FakeTaskOutcome::Aborted => Ok(()),
            FakeTaskOutcome::Completed | FakeTaskOutcome::ChannelClosed => {
                Err(EngineError::NotRunning)
            }
        }
    }
}

async fn emit_run(
    request: FakeRunRequest,
    sink: mpsc::Sender<EngineEvent>,
    delay: Duration,
    mut abort: oneshot::Receiver<()>,
) -> Result<(), EngineError> {
    let mut events = Vec::new();
    if let Some(transition) = request.transition {
        events.push(EngineEvent::SessionChanged {
            transition,
            reason: None,
        });
    }
    events.push(EngineEvent::RunStarted {
        model_label: "Fake model".into(),
    });

    match request.behavior {
        FakeRunBehavior::Crash => {
            events.push(EngineEvent::RunFailed {
                message: "fake engine process crashed".into(),
            });
        }
        FakeRunBehavior::LivenessTimeout => {
            events.push(EngineEvent::Liveness {
                state: LivenessState::Stalled,
            });
            events.push(EngineEvent::RunFailed {
                message: "fake engine liveness timed out".into(),
            });
        }
        FakeRunBehavior::HoldUntilAbort => {
            emit_events(&sink, events, delay, &mut abort).await?;
            let _ = abort.await;
            sink.send(EngineEvent::RunFailed {
                message: "fake engine run was aborted".into(),
            })
            .await
            .map_err(|_| EngineError::ChannelClosed)?;
            return Err(EngineError::Aborted);
        }
        FakeRunBehavior::DuplicateTerminal => {
            events.push(EngineEvent::RunCompleted {
                summary: "Completed by the deterministic fake engine".into(),
                artifacts: Vec::new(),
                validation: Vec::new(),
                limitations: Vec::new(),
            });
            events.push(EngineEvent::RunFailed {
                message: "duplicate terminal must not escape the adapter".into(),
            });
        }
        FakeRunBehavior::Complete => {
            if request.capabilities.thought_stream {
                events.push(EngineEvent::ThoughtDelta {
                    text: "Inspecting the Work".into(),
                });
            }
            if request.capabilities.plan_updates {
                events.push(EngineEvent::PlanChanged {
                    plan_id: "default".into(),
                    revision: 1,
                    text: "- inspect\n- execute\n- validate".into(),
                });
            }
            events.push(EngineEvent::AssistantDelta {
                text: format!("Working on: {}", request.prompt),
            });
            events.push(EngineEvent::ToolStarted {
                tool_call_id: format!("{}-tool-1", request.context.run_id()),
                tool_name: "fake_tool".into(),
                input_summary: "Inspect the workspace".into(),
            });
            if request.capabilities.parallel_tool_calls {
                events.push(EngineEvent::ToolStarted {
                    tool_call_id: format!("{}-tool-2", request.context.run_id()),
                    tool_name: "fake_parallel_tool".into(),
                    input_summary: "Inspect another file".into(),
                });
            }
            if request.capabilities.tool_progress {
                events.push(EngineEvent::ToolProgress {
                    tool_call_id: format!("{}-tool-1", request.context.run_id()),
                    tool_name: "fake_tool".into(),
                    output_summary: "Halfway complete".into(),
                });
            }
            if request.capabilities.permission_requests {
                events.push(EngineEvent::PermissionRequested {
                    request_id: format!("{}-permission-1", request.context.run_id()),
                    tool_call_id: Some(format!("{}-tool-1", request.context.run_id())),
                    title: "Allow fake tool".into(),
                    detail: "The conformance fake requests a deterministic permission".into(),
                });
                events.push(EngineEvent::PermissionResolved {
                    request_id: format!("{}-permission-1", request.context.run_id()),
                    outcome: PermissionOutcome::AllowedOnce,
                });
            }
            events.push(EngineEvent::ToolFinished {
                tool_call_id: format!("{}-tool-1", request.context.run_id()),
                tool_name: "fake_tool".into(),
                output_summary: "Workspace inspected".into(),
                success: true,
            });
            if request.capabilities.parallel_tool_calls {
                events.push(EngineEvent::ToolFinished {
                    tool_call_id: format!("{}-tool-2", request.context.run_id()),
                    tool_name: "fake_parallel_tool".into(),
                    output_summary: "Another file inspected".into(),
                    success: true,
                });
            }
            if request.capabilities.usage_reporting {
                events.push(EngineEvent::UsageUpdated {
                    input_tokens: 10,
                    output_tokens: 20,
                    cache_read_tokens: 3,
                    cache_write_tokens: 4,
                    total_tokens: 37,
                });
            }
            events.push(EngineEvent::AssistantDelta {
                text: "The requested work is complete.".into(),
            });
            events.push(EngineEvent::RunCompleted {
                summary: "Completed by the deterministic fake engine".into(),
                artifacts: Vec::new(),
                validation: vec!["fake validation passed".into()],
                limitations: vec!["fake engine only".into()],
            });
        }
    }

    match emit_events(&sink, events, delay, &mut abort).await {
        Err(EngineError::Aborted) => {
            sink.send(EngineEvent::RunFailed {
                message: "fake engine run was aborted".into(),
            })
            .await
            .map_err(|_| EngineError::ChannelClosed)?;
            Err(EngineError::Aborted)
        }
        outcome => outcome,
    }
}

async fn emit_events(
    sink: &mpsc::Sender<EngineEvent>,
    events: Vec<EngineEvent>,
    delay: Duration,
    abort: &mut oneshot::Receiver<()>,
) -> Result<(), EngineError> {
    for event in events {
        let terminal = event.is_terminal();
        tokio::select! {
            biased;
            _ = &mut *abort => return Err(EngineError::Aborted),
            _ = tokio::time::sleep(delay) => {}
        }
        tokio::select! {
            biased;
            _ = &mut *abort => return Err(EngineError::Aborted),
            result = sink.send(event) => result.map_err(|_| EngineError::ChannelClosed)?,
        }
        if terminal {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::sync::mpsc;

    use super::{FakeEngineAdapter, FakeEngineConfig, FakeRunBehavior, FakeRunState};
    use crate::engine::{
        EngineAdapter, EngineCapabilities, EngineEvent, EngineImage, EngineInput, EngineRunContext,
    };

    fn text_input(message: &str) -> EngineInput {
        EngineInput {
            message: message.into(),
            images: Vec::new(),
            documents: Vec::new(),
        }
    }

    #[tokio::test]
    async fn fake_engine_accepts_typed_image_input() {
        let (sender, mut receiver) = mpsc::channel(16);
        let engine = FakeEngineAdapter::new(Duration::ZERO);
        let context = EngineRunContext::test("work-1", "run-images");

        engine
            .start(
                context,
                EngineInput {
                    message: "Inspect".into(),
                    images: vec![EngineImage {
                        media_type: "image/png".into(),
                        data: vec![1, 2, 3],
                    }],
                    documents: Vec::new(),
                },
                sender,
            )
            .await
            .unwrap();

        let mut assistant = None;
        while let Some(event) = receiver.recv().await {
            if let crate::engine::EngineEvent::AssistantDelta { text } = event {
                assistant = Some(text);
                break;
            }
        }
        assert_eq!(assistant.as_deref(), Some("Working on: Inspect"));
    }

    #[tokio::test]
    async fn fake_engine_emits_a_complete_ordered_run() {
        let (sender, mut receiver) = mpsc::channel(16);
        let engine = FakeEngineAdapter::new(Duration::ZERO);
        let context = EngineRunContext::test("work-1", "run-1");

        let session = engine
            .start(context, text_input("Build it"), sender)
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
                "thought_delta",
                "plan_changed",
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
    async fn fake_engine_emits_initial_thought_and_default_plan() {
        let (sender, mut receiver) = mpsc::channel(16);
        let engine = FakeEngineAdapter::new(Duration::ZERO);
        let context = EngineRunContext::test("work-1", "run-activity");

        engine
            .start(context, text_input("Build it"), sender)
            .await
            .unwrap();

        assert!(matches!(
            receiver.recv().await.unwrap(),
            crate::engine::EngineEvent::RunStarted { .. }
        ));
        assert_eq!(
            receiver.recv().await.unwrap(),
            crate::engine::EngineEvent::ThoughtDelta {
                text: "Inspecting the Work".into(),
            }
        );
        assert_eq!(
            receiver.recv().await.unwrap(),
            crate::engine::EngineEvent::PlanChanged {
                plan_id: "default".into(),
                revision: 1,
                text: "- inspect\n- execute\n- validate".into(),
            }
        );
    }

    #[tokio::test]
    async fn start_rejects_a_run_while_its_previous_generation_is_cancelling() {
        let (engine, completion_gate) =
            FakeEngineAdapter::new_with_paused_first_completion(Duration::from_secs(60));
        let context = EngineRunContext::test("work-1", "run-1");
        let (first_sender, _first_receiver) = mpsc::channel(16);
        engine
            .start(context.clone(), text_input("First"), first_sender)
            .await
            .unwrap();

        let abort_engine = engine.clone();
        let abort = tokio::spawn(async move { abort_engine.abort("run-1").await });
        completion_gate.wait_until_reached().await;

        let (overlap_sender, _overlap_receiver) = mpsc::channel(16);
        let overlap = engine
            .start(context.clone(), text_input("Overlapping"), overlap_sender)
            .await;
        assert!(matches!(overlap, Err(crate::engine::EngineError::Start(_))));

        completion_gate.release();
        abort.await.unwrap().unwrap();

        let (second_sender, _second_receiver) = mpsc::channel(16);
        engine
            .start(context, text_input("Second"), second_sender)
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
            .start(context, text_input("Complete first"), sender)
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
            .start(context.clone(), text_input("First"), first_sender)
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
            .start(context, text_input("Second"), second_sender)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        engine.abort("run-1").await.unwrap();

        assert!(matches!(
            engine.abort("unknown-run").await,
            Err(crate::engine::EngineError::NotRunning)
        ));
    }

    #[tokio::test]
    async fn cancelled_abort_caller_cannot_strand_a_blocked_fake_terminal() {
        let engine = FakeEngineAdapter::configured(
            FakeEngineConfig::new(EngineCapabilities {
                cancel: true,
                ..EngineCapabilities::default()
            })
            .with_run_behavior(FakeRunBehavior::HoldUntilAbort),
        );
        let context = EngineRunContext::test("work-1", "run-cancelled-abort");
        let (sender, mut receiver) = mpsc::channel(1);
        engine
            .start(context.clone(), text_input("First"), sender)
            .await
            .unwrap();
        while receiver.is_empty() {
            tokio::task::yield_now().await;
        }

        let abort_engine = engine.clone();
        let abort = tokio::spawn(async move { abort_engine.abort("run-cancelled-abort").await });
        loop {
            let cancelling = engine
                .active
                .lock()
                .await
                .get("run-cancelled-abort")
                .is_some_and(|run| run.state == FakeRunState::Cancelling);
            if cancelling {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(!abort.is_finished());
        abort.abort();
        assert!(abort.await.unwrap_err().is_cancelled());

        assert!(matches!(
            receiver.recv().await,
            Some(EngineEvent::RunStarted { .. })
        ));
        assert!(matches!(
            receiver.recv().await,
            Some(EngineEvent::RunFailed { .. })
        ));
        assert!(receiver.recv().await.is_none());
        tokio::time::timeout(Duration::from_millis(200), async {
            while engine
                .active
                .lock()
                .await
                .contains_key("run-cancelled-abort")
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("cancelled abort caller stranded the Fake active map");

        let (restart_sender, mut restart_receiver) = mpsc::channel(2);
        engine
            .start(context, text_input("Second"), restart_sender)
            .await
            .unwrap();
        assert!(matches!(
            restart_receiver.recv().await,
            Some(EngineEvent::RunStarted { .. })
        ));
        engine.abort("run-cancelled-abort").await.unwrap();
        assert!(matches!(
            restart_receiver.recv().await,
            Some(EngineEvent::RunFailed { .. })
        ));
        assert!(restart_receiver.recv().await.is_none());
    }
}
