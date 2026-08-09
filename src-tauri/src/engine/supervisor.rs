use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};

use chrono::Utc;
use tokio::{
    sync::{Mutex, mpsc, oneshot, watch},
    task::{AbortHandle, JoinError, JoinHandle},
};
use uuid::Uuid;

use crate::{
    domain::{
        event::{WorkEventEnvelope, WorkEventPayload},
        work::StartWorkOutput,
    },
    engine::{EngineAdapter, EngineEvent, EngineInput, EngineRunContext, EngineSessionRef},
    error::AppError,
    work::repository::WorkRepository,
};

use super::publisher::EventPublisher;

const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_ABORT_TIMEOUT: Duration = Duration::from_secs(5);

type ActiveRuns = Arc<Mutex<HashMap<String, ActiveEntry>>>;

#[derive(Clone)]
struct ActiveEntry {
    generation: Uuid,
    work_id: String,
    run_id: Option<String>,
    session: Option<EngineSessionRef>,
    task: Option<AbortHandle>,
    termination: Option<Arc<TerminationOwner>>,
    state: ActiveState,
}

#[derive(Clone, PartialEq, Eq)]
enum ActiveState {
    Starting,
    Running,
    Stopping,
    Faulted { reason: String },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StartSignal {
    Pending,
    Ready,
    Failed,
}

enum ConsumerOutcome {
    Terminal,
    Stopped,
    Abnormal(&'static str),
    StartFailed,
}

enum StartPhaseOutcome {
    Returned(Result<EngineSessionRef, crate::engine::EngineError>),
    TimedOut,
    ConsumerFinished(Result<ConsumerOutcome, JoinError>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AbortOutcome {
    Confirmed,
    Unconfirmed,
}

enum AbortState {
    Open,
    Aborting,
    Done(AbortOutcome),
}

struct TerminationOwner {
    engine: Arc<dyn EngineAdapter>,
    run_id: String,
    timeout: Duration,
    state: Mutex<AbortState>,
    completed: watch::Sender<Option<AbortOutcome>>,
}

impl TerminationOwner {
    fn new(engine: Arc<dyn EngineAdapter>, run_id: String, timeout: Duration) -> Arc<Self> {
        let (completed, _) = watch::channel(None);
        Arc::new(Self {
            engine,
            run_id,
            timeout,
            state: Mutex::new(AbortState::Open),
            completed,
        })
    }

    async fn abort_once(self: &Arc<Self>) -> AbortOutcome {
        let mut completed = self.completed.subscribe();
        loop {
            let should_start = {
                let mut state = self.state.lock().await;
                match *state {
                    AbortState::Open => {
                        *state = AbortState::Aborting;
                        true
                    }
                    AbortState::Aborting => false,
                    AbortState::Done(outcome) => return outcome,
                }
            };
            if should_start {
                let owner = Arc::clone(self);
                tokio::spawn(complete_abort(owner));
            }
            let outcome = *completed.borrow();
            if let Some(outcome) = outcome {
                return outcome;
            }
            if completed.changed().await.is_err() {
                return AbortOutcome::Unconfirmed;
            }
        }
    }
}

async fn complete_abort(owner: Arc<TerminationOwner>) {
    let engine = Arc::clone(&owner.engine);
    let run_id = owner.run_id.clone();
    let timeout = owner.timeout;
    let abort =
        tokio::spawn(async move { tokio::time::timeout(timeout, engine.abort(&run_id)).await });
    let outcome = match abort.await {
        Ok(Ok(Ok(()))) | Ok(Ok(Err(crate::engine::EngineError::NotRunning))) => {
            AbortOutcome::Confirmed
        }
        Ok(Ok(Err(_))) | Ok(Err(_)) | Err(_) => AbortOutcome::Unconfirmed,
    };
    *owner.state.lock().await = AbortState::Done(outcome);
    owner.completed.send_replace(Some(outcome));
}

#[derive(Clone)]
pub struct EngineSupervisor {
    repository: WorkRepository,
    engine: Arc<dyn EngineAdapter>,
    publisher: Arc<dyn EventPublisher>,
    model_label: String,
    startup_timeout: Duration,
    abort_timeout: Duration,
    active: ActiveRuns,
}

impl EngineSupervisor {
    pub fn new(
        repository: WorkRepository,
        engine: Arc<dyn EngineAdapter>,
        publisher: Arc<dyn EventPublisher>,
        model_label: impl Into<String>,
    ) -> Self {
        Self::with_timeouts(
            repository,
            engine,
            publisher,
            model_label,
            DEFAULT_STARTUP_TIMEOUT,
            DEFAULT_ABORT_TIMEOUT,
        )
    }

    pub fn with_timeouts(
        repository: WorkRepository,
        engine: Arc<dyn EngineAdapter>,
        publisher: Arc<dyn EventPublisher>,
        model_label: impl Into<String>,
        startup_timeout: Duration,
        abort_timeout: Duration,
    ) -> Self {
        Self {
            repository,
            engine,
            publisher,
            model_label: model_label.into(),
            startup_timeout,
            abort_timeout,
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn start(&self, work_id: &str, prompt: &str) -> Result<StartWorkOutput, AppError> {
        self.start_with_engine_input(
            work_id,
            prompt,
            Vec::new(),
            EngineInput {
                message: prompt.trim().to_owned(),
                images: Vec::new(),
                documents: Vec::new(),
            },
        )
        .await
    }

    pub async fn start_with_engine_input(
        &self,
        work_id: &str,
        user_prompt: &str,
        resource_ids: Vec<String>,
        engine_input: EngineInput,
    ) -> Result<StartWorkOutput, AppError> {
        let generation = Uuid::new_v4();
        {
            let mut active = self.active.lock().await;
            if let Some(entry) = active.get(work_id) {
                return if matches!(entry.state, ActiveState::Faulted { .. }) {
                    Err(AppError::engine_faulted(work_id))
                } else {
                    Err(AppError::work_already_running(work_id))
                };
            }
            active.insert(
                work_id.to_owned(),
                ActiveEntry {
                    generation,
                    work_id: work_id.to_owned(),
                    run_id: None,
                    session: None,
                    task: None,
                    termination: None,
                    state: ActiveState::Starting,
                },
            );
        }

        let (result_sender, result_receiver) = oneshot::channel();
        let task = tokio::spawn(run_lifecycle(
            self.repository.clone(),
            Arc::clone(&self.engine),
            Arc::clone(&self.publisher),
            Arc::clone(&self.active),
            generation,
            work_id.to_owned(),
            user_prompt.to_owned(),
            resource_ids,
            engine_input,
            self.model_label.clone(),
            self.startup_timeout,
            self.abort_timeout,
            result_sender,
        ));
        let abort_handle = task.abort_handle();
        tokio::spawn(monitor_lifecycle(
            task,
            self.repository.clone(),
            Arc::clone(&self.publisher),
            Arc::clone(&self.active),
            work_id.to_owned(),
            generation,
        ));
        let active = Arc::clone(&self.active);
        let owned_work_id = work_id.to_owned();
        tokio::spawn(async move {
            set_task_handle(&active, &owned_work_id, generation, abort_handle).await;
        });

        result_receiver
            .await
            .unwrap_or_else(|_| Err(AppError::engine("Engine startup task ended unexpectedly")))
    }

    pub async fn stop(&self, work_id: &str) -> Result<(), AppError> {
        let (generation, run_id, termination) = {
            let mut active = self.active.lock().await;
            let entry = active
                .get_mut(work_id)
                .ok_or_else(|| AppError::engine("Work does not have an active execution"))?;
            let run_id = entry
                .run_id
                .clone()
                .ok_or_else(|| AppError::engine("Work execution is still starting"))?;
            let termination = entry
                .termination
                .clone()
                .ok_or_else(|| AppError::engine("Work execution cannot be stopped yet"))?;
            entry.state = ActiveState::Stopping;
            (entry.generation, run_id, termination)
        };

        if termination.abort_once().await == AbortOutcome::Unconfirmed {
            set_faulted(
                &self.active,
                work_id,
                generation,
                "Engine stop could not be confirmed",
            )
            .await;
            return Err(AppError::engine_faulted(work_id));
        }

        let stopped = self.repository.stop_run(&run_id, work_id).await;
        remove_active(&self.active, work_id, generation).await;
        stopped
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_lifecycle(
    repository: WorkRepository,
    engine: Arc<dyn EngineAdapter>,
    publisher: Arc<dyn EventPublisher>,
    active: ActiveRuns,
    generation: Uuid,
    work_id: String,
    user_prompt: String,
    resource_ids: Vec<String>,
    engine_input: EngineInput,
    model_label: String,
    startup_timeout: Duration,
    abort_timeout: Duration,
    result_sender: oneshot::Sender<Result<StartWorkOutput, AppError>>,
) {
    let work = match repository.get(&work_id).await {
        Ok(Some(work)) => work,
        Ok(None) => {
            remove_active(&active, &work_id, generation).await;
            let _ = result_sender.send(Err(AppError::work_not_found(&work_id)));
            return;
        }
        Err(error) => {
            remove_active(&active, &work_id, generation).await;
            let _ = result_sender.send(Err(error));
            return;
        }
    };
    let model_label = match engine.model_label(&model_label).await {
        Ok(model_label) => model_label,
        Err(_) => {
            remove_active(&active, &work_id, generation).await;
            let _ = result_sender.send(Err(AppError::engine_start_failed(&work_id)));
            return;
        }
    };
    let started = match repository
        .begin_run(
            &work_id,
            &user_prompt,
            &resource_ids,
            engine.kind(),
            &model_label,
        )
        .await
    {
        Ok(run) => run,
        Err(error) => {
            remove_active(&active, &work_id, generation).await;
            let _ = result_sender.send(Err(error));
            return;
        }
    };
    let run = started.run;
    let user_message = started.user_message;
    set_run_id(&active, &work_id, generation, &run.id).await;

    let context = match EngineRunContext::new(
        work_id.clone(),
        run.id.clone(),
        PathBuf::from(work.summary.root_path),
        work.summary.permission_mode,
    ) {
        Ok(context) => context,
        Err(error) => {
            let outcome = finalize_without_abort(
                &repository,
                &publisher,
                &active,
                &work_id,
                &run.id,
                generation,
                "Engine context initialization failed",
            )
            .await;
            let response = if outcome == AbortOutcome::Unconfirmed {
                AppError::engine_faulted(&work_id)
            } else {
                AppError::engine(error.to_string())
            };
            let _ = result_sender.send(Err(response));
            return;
        }
    };

    let termination = TerminationOwner::new(Arc::clone(&engine), run.id.clone(), abort_timeout);
    set_termination(&active, &work_id, generation, Arc::clone(&termination)).await;
    let (event_sender, event_receiver) = mpsc::channel(16);
    let (signal_sender, signal_receiver) = watch::channel(StartSignal::Pending);
    let mut consumer = tokio::spawn(consume_events(
        repository.clone(),
        Arc::clone(&publisher),
        Arc::clone(&active),
        generation,
        work_id.clone(),
        run.id.clone(),
        event_receiver,
        signal_receiver,
    ));

    let start_phase = {
        let start = engine.start(context, engine_input, event_sender);
        tokio::pin!(start);
        let deadline = tokio::time::sleep(startup_timeout);
        tokio::pin!(deadline);
        tokio::select! {
            result = &mut start => StartPhaseOutcome::Returned(result),
            _ = &mut deadline => StartPhaseOutcome::TimedOut,
            result = &mut consumer => StartPhaseOutcome::ConsumerFinished(result),
        }
    };

    let startup_diagnostic = match &start_phase {
        StartPhaseOutcome::Returned(Err(error)) => Some(error.to_string()),
        StartPhaseOutcome::TimedOut => Some("Engine startup timed out".to_owned()),
        _ => None,
    };
    let session = match start_phase {
        StartPhaseOutcome::Returned(Ok(session)) => session,
        StartPhaseOutcome::Returned(Err(_)) | StartPhaseOutcome::TimedOut => {
            let _ = signal_sender.send(StartSignal::Failed);
            stop_consumer(&mut consumer, abort_timeout).await;
            let outcome = terminate_abnormally(
                &termination,
                &repository,
                &publisher,
                &active,
                &work_id,
                &run.id,
                generation,
                "Engine failed to start",
            )
            .await;
            let response = if outcome == AbortOutcome::Confirmed {
                AppError::engine_start_failed_with_reason(
                    &work_id,
                    startup_diagnostic
                        .as_deref()
                        .unwrap_or("Engine startup failed without a diagnostic"),
                )
            } else {
                AppError::engine_faulted(&work_id)
            };
            let _ = result_sender.send(Err(response));
            return;
        }
        StartPhaseOutcome::ConsumerFinished(result) => {
            let reason = consumer_failure_reason(result);
            let _ = signal_sender.send(StartSignal::Failed);
            let outcome = terminate_abnormally(
                &termination,
                &repository,
                &publisher,
                &active,
                &work_id,
                &run.id,
                generation,
                reason,
            )
            .await;
            let response = if outcome == AbortOutcome::Confirmed {
                AppError::engine_start_failed(&work_id)
            } else {
                AppError::engine_faulted(&work_id)
            };
            let _ = result_sender.send(Err(response));
            return;
        }
    };

    let attached = match repository
        .attach_engine_session(&run.id, &session.engine_kind, &session.session_id)
        .await
    {
        Ok(attached) => attached,
        Err(_) => {
            let _ = signal_sender.send(StartSignal::Failed);
            stop_consumer(&mut consumer, abort_timeout).await;
            let outcome = terminate_abnormally(
                &termination,
                &repository,
                &publisher,
                &active,
                &work_id,
                &run.id,
                generation,
                "Engine session could not be attached",
            )
            .await;
            let response = if outcome == AbortOutcome::Confirmed {
                AppError::engine_start_failed(&work_id)
            } else {
                AppError::engine_faulted(&work_id)
            };
            let _ = result_sender.send(Err(response));
            return;
        }
    };
    set_running(&active, &work_id, generation, session).await;
    let _ = signal_sender.send(StartSignal::Ready);
    let _ = result_sender.send(Ok(StartWorkOutput {
        run: attached,
        user_message,
    }));

    match consumer.await {
        Ok(ConsumerOutcome::Terminal) => {}
        Ok(ConsumerOutcome::Stopped) => {}
        Ok(ConsumerOutcome::Abnormal(reason)) => {
            let _ = terminate_abnormally(
                &termination,
                &repository,
                &publisher,
                &active,
                &work_id,
                &run.id,
                generation,
                reason,
            )
            .await;
        }
        Ok(ConsumerOutcome::StartFailed) | Err(_) => {
            let _ = terminate_abnormally(
                &termination,
                &repository,
                &publisher,
                &active,
                &work_id,
                &run.id,
                generation,
                "Engine event consumer stopped unexpectedly",
            )
            .await;
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn consume_events(
    repository: WorkRepository,
    publisher: Arc<dyn EventPublisher>,
    active: ActiveRuns,
    generation: Uuid,
    work_id: String,
    run_id: String,
    mut receiver: mpsc::Receiver<EngineEvent>,
    mut start_signal: watch::Receiver<StartSignal>,
) -> ConsumerOutcome {
    let mut sequence = 1_u32;
    loop {
        let current_start_signal = *start_signal.borrow();
        let event = match current_start_signal {
            StartSignal::Failed => return ConsumerOutcome::StartFailed,
            StartSignal::Pending => {
                tokio::select! {
                    biased;
                    changed = start_signal.changed() => {
                        if changed.is_err() || *start_signal.borrow() == StartSignal::Failed {
                            return ConsumerOutcome::StartFailed;
                        }
                        continue;
                    }
                    event = receiver.recv() => event,
                }
            }
            StartSignal::Ready => receiver.recv().await,
        };
        let Some(event) = event else {
            if is_stopping(&active, &work_id, generation).await {
                return ConsumerOutcome::Stopped;
            }
            return match await_start_signal(&mut start_signal).await {
                StartSignal::Ready => {
                    ConsumerOutcome::Abnormal("Engine event stream closed before a terminal event")
                }
                StartSignal::Failed | StartSignal::Pending => ConsumerOutcome::StartFailed,
            };
        };
        if is_stopping(&active, &work_id, generation).await {
            return ConsumerOutcome::Stopped;
        }
        let terminal = event.is_terminal();
        if terminal {
            match await_start_signal_while_draining(&mut start_signal, &mut receiver).await {
                StartSignal::Ready => {}
                StartSignal::Failed | StartSignal::Pending => {
                    return ConsumerOutcome::StartFailed;
                }
            }
        }
        let envelope = WorkEventEnvelope {
            version: 1,
            event_id: Some(Uuid::new_v4().to_string()),
            work_id: work_id.clone(),
            run_id: run_id.clone(),
            turn_id: None,
            session_id: None,
            agent_id: None,
            assignment_id: None,
            causation_id: None,
            correlation_id: None,
            sequence,
            occurred_at: Utc::now(),
            payload: event.into(),
        };
        if repository
            .append_event_and_transition(&envelope)
            .await
            .is_err()
        {
            if *start_signal.borrow() == StartSignal::Pending {
                let signal =
                    await_start_signal_while_draining(&mut start_signal, &mut receiver).await;
                if signal != StartSignal::Ready {
                    return ConsumerOutcome::StartFailed;
                }
            }
            return ConsumerOutcome::Abnormal("Engine event could not be journaled");
        }
        if terminal {
            remove_active(&active, &work_id, generation).await;
            let _ = publisher.publish(envelope).await;
            return ConsumerOutcome::Terminal;
        }
        let _ = publisher.publish(envelope).await;
        let Some(next) = sequence.checked_add(1) else {
            return ConsumerOutcome::Abnormal("Engine event sequence limit was reached");
        };
        sequence = next;
    }
}

async fn stop_consumer(consumer: &mut JoinHandle<ConsumerOutcome>, timeout: Duration) {
    if tokio::time::timeout(timeout, &mut *consumer).await.is_err() {
        consumer.abort();
        let _ = consumer.await;
    }
}

fn consumer_failure_reason(result: Result<ConsumerOutcome, JoinError>) -> &'static str {
    match result {
        Ok(ConsumerOutcome::Abnormal(reason)) => reason,
        Ok(ConsumerOutcome::Terminal) => "Engine terminated before its session was attached",
        Ok(ConsumerOutcome::Stopped) => "Engine stopped before its session was attached",
        Ok(ConsumerOutcome::StartFailed) => "Engine startup failed",
        Err(_) => "Engine event consumer stopped unexpectedly",
    }
}

async fn await_start_signal(signal: &mut watch::Receiver<StartSignal>) -> StartSignal {
    loop {
        let current = *signal.borrow();
        if current != StartSignal::Pending {
            return current;
        }
        if signal.changed().await.is_err() {
            return StartSignal::Failed;
        }
    }
}

async fn await_start_signal_while_draining(
    signal: &mut watch::Receiver<StartSignal>,
    receiver: &mut mpsc::Receiver<EngineEvent>,
) -> StartSignal {
    loop {
        let current = *signal.borrow();
        if current != StartSignal::Pending {
            return current;
        }
        tokio::select! {
            changed = signal.changed() => {
                if changed.is_err() {
                    return StartSignal::Failed;
                }
            }
            event = receiver.recv() => {
                if event.is_none() {
                    return await_start_signal(signal).await;
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn terminate_abnormally(
    termination: &Arc<TerminationOwner>,
    repository: &WorkRepository,
    publisher: &Arc<dyn EventPublisher>,
    active: &ActiveRuns,
    work_id: &str,
    run_id: &str,
    generation: Uuid,
    reason: &'static str,
) -> AbortOutcome {
    let abort = termination.abort_once().await;
    let finalized = repository
        .finalize_run_failure(run_id, work_id, reason)
        .await;
    match (abort, finalized) {
        (AbortOutcome::Confirmed, Ok(envelope)) => {
            remove_active(active, work_id, generation).await;
            let _ = publisher.publish(envelope).await;
            AbortOutcome::Confirmed
        }
        (abort, Ok(envelope)) => {
            set_faulted(active, work_id, generation, reason).await;
            let _ = publisher.publish(envelope).await;
            abort
        }
        (_, Err(_)) => {
            set_faulted(active, work_id, generation, reason).await;
            AbortOutcome::Unconfirmed
        }
    }
}

async fn finalize_without_abort(
    repository: &WorkRepository,
    publisher: &Arc<dyn EventPublisher>,
    active: &ActiveRuns,
    work_id: &str,
    run_id: &str,
    generation: Uuid,
    reason: &'static str,
) -> AbortOutcome {
    match repository
        .finalize_run_failure(run_id, work_id, reason)
        .await
    {
        Ok(envelope) => {
            remove_active(active, work_id, generation).await;
            let _ = publisher.publish(envelope).await;
            AbortOutcome::Confirmed
        }
        Err(_) => {
            set_faulted(active, work_id, generation, reason).await;
            AbortOutcome::Unconfirmed
        }
    }
}

async fn monitor_lifecycle(
    task: JoinHandle<()>,
    repository: WorkRepository,
    publisher: Arc<dyn EventPublisher>,
    active: ActiveRuns,
    work_id: String,
    generation: Uuid,
) {
    if task.await.is_ok() {
        return;
    }
    let identity = {
        let active = active.lock().await;
        active
            .get(&work_id)
            .filter(|entry| entry.generation == generation && entry.work_id == work_id)
            .map(|entry| (entry.run_id.clone(), entry.termination.clone()))
    };
    match identity {
        Some((Some(run_id), Some(termination))) => {
            let _ = terminate_abnormally(
                &termination,
                &repository,
                &publisher,
                &active,
                &work_id,
                &run_id,
                generation,
                "Engine lifecycle task stopped unexpectedly",
            )
            .await;
        }
        Some((Some(run_id), None)) => {
            let _ = finalize_without_abort(
                &repository,
                &publisher,
                &active,
                &work_id,
                &run_id,
                generation,
                "Engine lifecycle task stopped unexpectedly",
            )
            .await;
        }
        Some((None, _)) => remove_active(&active, &work_id, generation).await,
        None => {}
    }
}

async fn set_task_handle(active: &ActiveRuns, work_id: &str, generation: Uuid, task: AbortHandle) {
    let mut active = active.lock().await;
    if let Some(entry) = active
        .get_mut(work_id)
        .filter(|entry| entry.generation == generation && entry.work_id == work_id)
    {
        entry.task = Some(task);
    }
}

async fn is_stopping(active: &ActiveRuns, work_id: &str, generation: Uuid) -> bool {
    active.lock().await.get(work_id).is_some_and(|entry| {
        entry.generation == generation && matches!(entry.state, ActiveState::Stopping)
    })
}

async fn set_run_id(active: &ActiveRuns, work_id: &str, generation: Uuid, run_id: &str) {
    let mut active = active.lock().await;
    if let Some(entry) = active
        .get_mut(work_id)
        .filter(|entry| entry.generation == generation && entry.work_id == work_id)
    {
        entry.run_id = Some(run_id.to_owned());
    }
}

async fn set_termination(
    active: &ActiveRuns,
    work_id: &str,
    generation: Uuid,
    termination: Arc<TerminationOwner>,
) {
    let mut active = active.lock().await;
    if let Some(entry) = active
        .get_mut(work_id)
        .filter(|entry| entry.generation == generation && entry.work_id == work_id)
    {
        entry.termination = Some(termination);
    }
}

async fn set_running(
    active: &ActiveRuns,
    work_id: &str,
    generation: Uuid,
    session: EngineSessionRef,
) {
    let mut active = active.lock().await;
    if let Some(entry) = active
        .get_mut(work_id)
        .filter(|entry| entry.generation == generation && entry.work_id == work_id)
    {
        entry.session = Some(session);
        entry.state = ActiveState::Running;
    }
}

async fn set_faulted(active: &ActiveRuns, work_id: &str, generation: Uuid, reason: &str) {
    let mut active = active.lock().await;
    if let Some(entry) = active
        .get_mut(work_id)
        .filter(|entry| entry.generation == generation && entry.work_id == work_id)
    {
        entry.state = ActiveState::Faulted {
            reason: reason.to_owned(),
        };
    }
}

async fn remove_active(active: &ActiveRuns, work_id: &str, generation: Uuid) {
    let mut active = active.lock().await;
    if active
        .get(work_id)
        .is_some_and(|entry| entry.generation == generation && entry.work_id == work_id)
    {
        active.remove(work_id);
    }
}

impl From<EngineEvent> for WorkEventPayload {
    fn from(event: EngineEvent) -> Self {
        match event {
            EngineEvent::RunStarted { model_label } => Self::RunStarted { model_label },
            EngineEvent::AssistantDelta { text } => Self::AssistantDelta { text },
            EngineEvent::ToolStarted {
                tool_call_id,
                tool_name,
                input_summary,
            } => Self::ToolStarted {
                tool_call_id,
                tool_name,
                input_summary,
            },
            EngineEvent::ToolFinished {
                tool_call_id,
                tool_name,
                output_summary,
                success,
            } => Self::ToolFinished {
                tool_call_id,
                tool_name,
                output_summary,
                success,
            },
            EngineEvent::RunCompleted {
                summary,
                artifacts,
                validation,
                limitations,
            } => Self::RunCompleted {
                summary,
                artifacts,
                validation,
                limitations,
            },
            EngineEvent::RunFailed { message } => Self::RunFailed { message },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use async_trait::async_trait;
    use tokio::{sync::mpsc, task::JoinSet};

    use super::{AbortOutcome, TerminationOwner};
    use crate::engine::{
        EngineAdapter, EngineError, EngineEvent, EngineInput, EngineRunContext, EngineSessionRef,
    };

    #[derive(Default)]
    struct InstantAbortEngine {
        abort_calls: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl EngineAdapter for InstantAbortEngine {
        fn kind(&self) -> &'static str {
            "instant-abort"
        }

        async fn start(
            &self,
            _context: EngineRunContext,
            _input: EngineInput,
            _sink: mpsc::Sender<EngineEvent>,
        ) -> Result<EngineSessionRef, EngineError> {
            Err(EngineError::Start("not used by this test".into()))
        }

        async fn abort(&self, _run_id: &str) -> Result<(), EngineError> {
            self.abort_calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn concurrent_abort_waiters_observe_one_instant_abort_result() {
        let engine = Arc::new(InstantAbortEngine::default());

        for iteration in 0..100 {
            let owner = TerminationOwner::new(
                engine.clone(),
                format!("run-{iteration}"),
                Duration::from_secs(1),
            );
            let mut waiters = JoinSet::new();
            for _ in 0..16 {
                let owner = Arc::clone(&owner);
                waiters.spawn(async move { owner.abort_once().await });
            }

            tokio::time::timeout(Duration::from_secs(1), async {
                while let Some(outcome) = waiters.join_next().await {
                    assert!(outcome.unwrap() == AbortOutcome::Confirmed);
                }
            })
            .await
            .expect("concurrent abort waiter missed the completed result");
        }

        assert_eq!(
            engine.abort_calls.load(std::sync::atomic::Ordering::SeqCst),
            100
        );
    }
}
