use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

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
    engine::{
        EngineAdapter, EngineEvent, EngineInput, EngineRunContext, EngineRunIdentity,
        EngineSessionRef,
    },
    error::AppError,
    work::repository::WorkRepository,
};

use super::publisher::EventPublisher;

const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_ABORT_TIMEOUT: Duration = Duration::from_secs(5);
const STARTUP_EVENT_BUFFER_CAP: usize = 1000;

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

#[derive(Clone, PartialEq, Eq)]
enum StartSignal {
    Pending,
    Ready { session_id: String },
    Failed,
}

#[derive(Debug)]
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
    let assignment = started.assignment;
    let run = match started.run {
        Some(run) => run,
        None => {
            remove_active(&active, &work_id, generation).await;
            let _ = result_sender.send(Err(AppError::engine_start_failed_with_reason(
                &work_id,
                "run was not created for immediate execution",
            )));
            return;
        }
    };
    let user_message = started.user_message;
    set_run_id(&active, &work_id, generation, &run.id).await;

    // C5 keeps the legacy one-session-per-Work mapping. Repository-backed
    // agent sessions and generation rotation belong to the later C6 boundary.
    let context = match EngineRunIdentity::new(
        work_id.clone(),
        run.id.clone(),
        assignment.id.clone(),
        assignment.assigned_agent_id.clone(),
        work_id.clone(),
        0,
    )
    .and_then(|identity| {
        EngineRunContext::new(
            identity,
            PathBuf::from(&work.summary.root_path),
            work.summary.permission_mode,
            None,
            work.summary.permission_mode,
        )
    }) {
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
    let (readiness_sender, readiness_receiver) = oneshot::channel();
    let mut consumer = tokio::spawn(consume_events(
        repository.clone(),
        Arc::clone(&publisher),
        Arc::clone(&active),
        generation,
        work_id.clone(),
        run.id.clone(),
        event_receiver,
        signal_receiver,
        readiness_sender,
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
            signal_sender.send_replace(StartSignal::Failed);
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
            signal_sender.send_replace(StartSignal::Failed);
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
            signal_sender.send_replace(StartSignal::Failed);
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
    let session_id = session.session_id.clone();
    set_running(&active, &work_id, generation, session).await;
    signal_sender.send_replace(StartSignal::Ready { session_id });
    let consumer_before_readiness = tokio::select! {
        biased;
        result = &mut consumer => Some(result),
        readiness = readiness_receiver => {
            match readiness {
                Ok(()) => None,
                Err(_) => Some((&mut consumer).await),
            }
        }
    };
    if let Some(result) = consumer_before_readiness {
        let reason = consumer_failure_reason(result);
        signal_sender.send_replace(StartSignal::Failed);
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
    let _ = result_sender.send(Ok(StartWorkOutput {
        assignment,
        run: Some(attached),
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
    readiness_sender: oneshot::Sender<()>,
) -> ConsumerOutcome {
    let (session_id, mut startup_events) =
        match wait_for_engine_session(&mut receiver, &mut start_signal).await {
            Ok(ready) => ready,
            Err(outcome) => return outcome,
        };
    let _ = readiness_sender.send(());
    let mut sequence = 1_u32;
    let mut previous_event_id = None;
    loop {
        let event = match startup_events.pop_front() {
            Some(event) => Some(event),
            None => receiver.recv().await,
        };
        let Some(event) = event else {
            if is_stopping(&active, &work_id, generation).await {
                return ConsumerOutcome::Stopped;
            }
            return ConsumerOutcome::Abnormal("Engine event stream closed before a terminal event");
        };
        if is_stopping(&active, &work_id, generation).await {
            return ConsumerOutcome::Stopped;
        }
        let terminal = event.is_terminal();
        let event_id = Uuid::new_v4().to_string();
        let envelope = WorkEventEnvelope {
            version: 2,
            event_id: Some(event_id.clone()),
            work_id: work_id.clone(),
            run_id: Some(run_id.clone()),
            turn_id: Some(run_id.clone()),
            session_id: Some(session_id.clone()),
            agent_id: None,
            assignment_id: None,
            causation_id: previous_event_id.clone(),
            correlation_id: Some(run_id.clone()),
            sequence,
            occurred_at: Utc::now(),
            payload: event.into(),
        };
        if repository
            .append_event_and_transition(&envelope)
            .await
            .is_err()
        {
            return ConsumerOutcome::Abnormal("Engine event could not be journaled");
        }
        previous_event_id = Some(event_id);
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

async fn wait_for_engine_session(
    receiver: &mut mpsc::Receiver<EngineEvent>,
    signal: &mut watch::Receiver<StartSignal>,
) -> Result<(String, VecDeque<EngineEvent>), ConsumerOutcome> {
    let mut buffered = VecDeque::new();
    let mut receiver_closed = false;
    loop {
        match signal.borrow().clone() {
            StartSignal::Pending => {}
            StartSignal::Ready { session_id } => {
                if receiver_closed && !buffered.back().is_some_and(EngineEvent::is_terminal) {
                    return Err(ConsumerOutcome::Abnormal(
                        "Engine event stream closed before a terminal event",
                    ));
                }
                return Ok((session_id, buffered));
            }
            StartSignal::Failed => return Err(ConsumerOutcome::StartFailed),
        }

        if receiver_closed {
            if signal.changed().await.is_err() {
                return Err(ConsumerOutcome::StartFailed);
            }
            continue;
        }

        tokio::select! {
            biased;
            changed = signal.changed() => {
                if changed.is_err() {
                    return Err(ConsumerOutcome::StartFailed);
                }
            }
            event = receiver.recv() => {
                match event {
                    Some(event) if buffered.len() < STARTUP_EVENT_BUFFER_CAP => {
                        buffered.push_back(event);
                    }
                    Some(_) => {
                        return Err(ConsumerOutcome::Abnormal(
                            "Engine emitted too many events before session attachment",
                        ));
                    }
                    None => receiver_closed = true,
                }
            }
        }
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
            publish_failure_without_panicking(publisher, envelope).await;
            AbortOutcome::Confirmed
        }
        (abort, Ok(envelope)) => {
            set_faulted(active, work_id, generation, reason).await;
            publish_failure_without_panicking(publisher, envelope).await;
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
            publish_failure_without_panicking(publisher, envelope).await;
            AbortOutcome::Confirmed
        }
        Err(_) => {
            set_faulted(active, work_id, generation, reason).await;
            AbortOutcome::Unconfirmed
        }
    }
}

async fn publish_failure_without_panicking(
    publisher: &Arc<dyn EventPublisher>,
    envelope: WorkEventEnvelope,
) {
    let publisher = Arc::clone(publisher);
    let _ = tokio::spawn(async move { publisher.publish(envelope).await }).await;
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
            EngineEvent::ThoughtDelta { text } => Self::ThoughtDelta { text },
            EngineEvent::PlanChanged {
                plan_id,
                revision,
                text,
            } => Self::PlanChanged {
                plan_id,
                revision,
                text,
            },
            EngineEvent::ToolPending {
                tool_call_id,
                tool_name,
                input_summary,
            } => Self::ToolPending {
                tool_call_id,
                tool_name,
                input_summary,
            },
            EngineEvent::ToolProgress {
                tool_call_id,
                tool_name,
                output_summary,
            } => Self::ToolProgress {
                tool_call_id,
                tool_name,
                output_summary,
            },
            EngineEvent::PermissionRequested {
                request_id,
                tool_call_id,
                title,
                detail,
            } => Self::PermissionRequested {
                request_id,
                tool_call_id,
                title,
                detail,
            },
            EngineEvent::PermissionResolved {
                request_id,
                outcome,
            } => Self::PermissionResolved {
                request_id,
                outcome,
            },
            EngineEvent::Waiting { reason } => Self::Waiting { reason },
            EngineEvent::Liveness { state } => Self::Liveness { state },
            EngineEvent::SessionChanged { transition, reason } => {
                Self::SessionChanged { transition, reason }
            }
            EngineEvent::ArtifactProduced { path } => Self::ArtifactProduced { path },
            EngineEvent::ValidationProduced {
                command,
                success,
                summary,
            } => Self::ValidationProduced {
                command,
                success,
                summary,
            },
            EngineEvent::UsageUpdated {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_write_tokens,
                total_tokens,
            } => Self::UsageUpdated {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_write_tokens,
                total_tokens,
            },
            EngineEvent::RawEngineEvent { kind, payload_json } => {
                Self::RawEngineEvent { kind, payload_json }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use async_trait::async_trait;
    use tokio::{
        sync::{mpsc, watch},
        task::JoinSet,
    };

    use super::{
        AbortOutcome, ConsumerOutcome, EngineSupervisor, STARTUP_EVENT_BUFFER_CAP, StartSignal,
        TerminationOwner, wait_for_engine_session,
    };
    use crate::{
        domain::work::{CreateWorkInput, PermissionMode},
        engine::{
            EngineAdapter, EngineError, EngineEvent, EngineInput, EngineRunContext,
            EngineSessionRef, fake::FakeEngineAdapter, publisher::ChannelEventPublisher,
        },
        storage::sqlite::Database,
        work::repository::WorkRepository,
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
    async fn startup_events_are_buffered_until_the_engine_session_is_ready() {
        let (event_sender, mut event_receiver) = mpsc::channel(1);
        let (signal_sender, mut signal_receiver) = watch::channel(StartSignal::Pending);
        event_sender
            .send(EngineEvent::RunStarted {
                model_label: "Fake model".into(),
            })
            .await
            .unwrap();
        let waiter = tokio::spawn(async move {
            wait_for_engine_session(&mut event_receiver, &mut signal_receiver).await
        });

        event_sender
            .send(EngineEvent::AssistantDelta {
                text: "queued after the buffered event".into(),
            })
            .await
            .unwrap();
        signal_sender.send_replace(StartSignal::Ready {
            session_id: "fake-session".into(),
        });

        let (session_id, buffered) = waiter.await.unwrap().unwrap();
        assert_eq!(session_id, "fake-session");
        assert!(matches!(
            buffered.front(),
            Some(EngineEvent::RunStarted { model_label }) if model_label == "Fake model"
        ));
    }

    #[tokio::test]
    async fn startup_event_buffer_rejects_more_than_its_capacity() {
        let (event_sender, mut event_receiver) = mpsc::channel(STARTUP_EVENT_BUFFER_CAP + 1);
        let (_signal_sender, mut signal_receiver) = watch::channel(StartSignal::Pending);
        for index in 0..=STARTUP_EVENT_BUFFER_CAP {
            event_sender
                .send(EngineEvent::AssistantDelta {
                    text: format!("chunk-{index}"),
                })
                .await
                .unwrap();
        }

        let outcome = wait_for_engine_session(&mut event_receiver, &mut signal_receiver).await;

        assert!(matches!(
            outcome,
            Err(ConsumerOutcome::Abnormal(
                "Engine emitted too many events before session attachment"
            ))
        ));
    }

    #[tokio::test]
    async fn startup_stream_closure_without_terminal_fails_after_ready() {
        let (event_sender, mut event_receiver) = mpsc::channel(1);
        let (signal_sender, mut signal_receiver) = watch::channel(StartSignal::Pending);
        drop(event_sender);
        let waiter = tokio::spawn(async move {
            wait_for_engine_session(&mut event_receiver, &mut signal_receiver).await
        });
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }

        signal_sender.send_replace(StartSignal::Ready {
            session_id: "fake-session".into(),
        });
        let outcome = waiter.await.unwrap();

        assert!(matches!(
            outcome,
            Err(ConsumerOutcome::Abnormal(
                "Engine event stream closed before a terminal event"
            ))
        ));
    }

    #[tokio::test]
    async fn published_events_include_session_and_causal_context() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let workspace = temporary_directory.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let database = Database::open_in_memory().await.unwrap();
        let repository = WorkRepository::new(database.pool().clone());
        let work = repository
            .create(CreateWorkInput {
                title: "Causal context".into(),
                goal: "Publish connected activity events".into(),
                root_path: workspace.to_string_lossy().into_owned(),
                permission_mode: PermissionMode::Balanced,
                resource_draft_id: None,
            })
            .await
            .unwrap();
        let engine = FakeEngineAdapter::new_with_session(Duration::ZERO, "fake-session");
        let (publisher, mut published) = ChannelEventPublisher::channel(16);
        let supervisor = EngineSupervisor::new(
            repository,
            Arc::new(engine),
            Arc::new(publisher),
            "Fake model",
        );

        let started = supervisor
            .start(&work.summary.id, "Inspect it")
            .await
            .unwrap();
        let first = published.recv().await.unwrap();
        let second = published.recv().await.unwrap();

        assert_eq!(first.version, 2);
        assert!(first.event_id.is_some());
        assert_eq!(
            first.turn_id.as_deref(),
            Some(started.run.as_ref().expect("immediate run").id.as_str())
        );
        assert_eq!(first.session_id.as_deref(), Some("fake-session"));
        assert_eq!(
            first.correlation_id.as_deref(),
            Some(started.run.as_ref().expect("immediate run").id.as_str())
        );
        assert_eq!(first.causation_id, None);
        assert_eq!(second.version, 2);
        assert!(second.event_id.is_some());
        assert_eq!(
            second.turn_id.as_deref(),
            Some(started.run.as_ref().expect("immediate run").id.as_str())
        );
        assert_eq!(second.session_id.as_deref(), Some("fake-session"));
        assert_eq!(
            second.correlation_id.as_deref(),
            Some(started.run.as_ref().expect("immediate run").id.as_str())
        );
        assert_eq!(second.causation_id, first.event_id);
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
