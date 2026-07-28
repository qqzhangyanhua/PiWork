use std::{collections::HashMap, path::PathBuf, sync::Arc};

use chrono::Utc;
use tokio::{
    sync::{Mutex, mpsc, oneshot, watch},
    task::{AbortHandle, JoinHandle},
};
use uuid::Uuid;

use crate::{
    domain::{
        event::{WorkEventEnvelope, WorkEventPayload},
        work::RunSummary,
    },
    engine::{EngineAdapter, EngineEvent, EngineRunContext, EngineSessionRef},
    error::AppError,
    work::repository::WorkRepository,
};

use super::publisher::EventPublisher;

type ActiveRuns = Arc<Mutex<HashMap<String, ActiveEntry>>>;

#[derive(Debug, Clone)]
struct ActiveEntry {
    generation: Uuid,
    work_id: String,
    run_id: Option<String>,
    session: Option<EngineSessionRef>,
    task: Option<AbortHandle>,
    state: ActiveState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ActiveState {
    Starting,
    Running,
    Faulted { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartSignal {
    Pending,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConsumerOutcome {
    Terminal,
    Failed,
    StartFailed,
    Faulted,
}

#[derive(Clone)]
pub struct EngineSupervisor {
    repository: WorkRepository,
    engine: Arc<dyn EngineAdapter>,
    publisher: Arc<dyn EventPublisher>,
    model_label: String,
    active: ActiveRuns,
}

impl EngineSupervisor {
    pub fn new(
        repository: WorkRepository,
        engine: Arc<dyn EngineAdapter>,
        publisher: Arc<dyn EventPublisher>,
        model_label: impl Into<String>,
    ) -> Self {
        Self {
            repository,
            engine,
            publisher,
            model_label: model_label.into(),
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn start(&self, work_id: &str, prompt: &str) -> Result<RunSummary, AppError> {
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
            prompt.to_owned(),
            self.model_label.clone(),
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
}

#[allow(clippy::too_many_arguments)]
async fn run_lifecycle(
    repository: WorkRepository,
    engine: Arc<dyn EngineAdapter>,
    publisher: Arc<dyn EventPublisher>,
    active: ActiveRuns,
    generation: Uuid,
    work_id: String,
    prompt: String,
    model_label: String,
    result_sender: oneshot::Sender<Result<RunSummary, AppError>>,
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
    let run = match repository
        .begin_run(&work_id, &prompt, engine.kind(), &model_label)
        .await
    {
        Ok(run) => run,
        Err(error) => {
            remove_active(&active, &work_id, generation).await;
            let _ = result_sender.send(Err(error));
            return;
        }
    };
    set_run_id(&active, &work_id, generation, &run.id).await;

    let context = match EngineRunContext::new(
        work_id.clone(),
        run.id.clone(),
        PathBuf::from(work.summary.root_path),
        work.summary.permission_mode,
    ) {
        Ok(context) => context,
        Err(error) => {
            let faulted = finalize_failure(
                &repository,
                &publisher,
                &active,
                &work_id,
                &run.id,
                generation,
                "Engine context initialization failed",
            )
            .await
                == ConsumerOutcome::Faulted;
            let response = if faulted {
                AppError::engine_faulted(&work_id)
            } else {
                AppError::engine(error.to_string())
            };
            let _ = result_sender.send(Err(response));
            return;
        }
    };

    let (event_sender, event_receiver) = mpsc::channel(16);
    let (signal_sender, signal_receiver) = watch::channel(StartSignal::Pending);
    let consumer = tokio::spawn(consume_events(
        repository.clone(),
        Arc::clone(&publisher),
        Arc::clone(&active),
        generation,
        work_id.clone(),
        run.id.clone(),
        event_receiver,
        signal_receiver,
    ));

    let session = match engine
        .start(context, prompt.trim().to_owned(), event_sender)
        .await
    {
        Ok(session) => session,
        Err(error) => {
            let _ = signal_sender.send(StartSignal::Failed);
            let outcome = finalize_failure(
                &repository,
                &publisher,
                &active,
                &work_id,
                &run.id,
                generation,
                "Engine failed to start",
            )
            .await;
            let _ = consumer.await;
            let response = if outcome == ConsumerOutcome::Faulted {
                AppError::engine_faulted(&work_id)
            } else {
                AppError::engine(error.to_string())
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
        Err(error) => {
            let _ = signal_sender.send(StartSignal::Failed);
            let outcome = finalize_failure(
                &repository,
                &publisher,
                &active,
                &work_id,
                &run.id,
                generation,
                "Engine session could not be attached",
            )
            .await;
            let _ = consumer.await;
            let response = if outcome == ConsumerOutcome::Faulted {
                AppError::engine_faulted(&work_id)
            } else {
                error
            };
            let _ = result_sender.send(Err(response));
            return;
        }
    };
    set_running(&active, &work_id, generation, session).await;
    let _ = signal_sender.send(StartSignal::Ready);
    let _ = result_sender.send(Ok(attached));

    if consumer.await.is_err() {
        let _ = finalize_failure(
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
    while let Some(event) = receiver.recv().await {
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
            work_id: work_id.clone(),
            run_id: run_id.clone(),
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
            return finalize_failure(
                &repository,
                &publisher,
                &active,
                &work_id,
                &run_id,
                generation,
                "Engine event could not be journaled",
            )
            .await;
        }
        if terminal {
            remove_active(&active, &work_id, generation).await;
            let _ = publisher.publish(envelope).await;
            return ConsumerOutcome::Terminal;
        }
        let _ = publisher.publish(envelope).await;
        let Some(next) = sequence.checked_add(1) else {
            return finalize_failure(
                &repository,
                &publisher,
                &active,
                &work_id,
                &run_id,
                generation,
                "Engine event sequence limit was reached",
            )
            .await;
        };
        sequence = next;
    }

    match await_start_signal(&mut start_signal).await {
        StartSignal::Ready => {
            finalize_failure(
                &repository,
                &publisher,
                &active,
                &work_id,
                &run_id,
                generation,
                "Engine event stream closed before a terminal event",
            )
            .await
        }
        StartSignal::Failed | StartSignal::Pending => ConsumerOutcome::StartFailed,
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

async fn finalize_failure(
    repository: &WorkRepository,
    publisher: &Arc<dyn EventPublisher>,
    active: &ActiveRuns,
    work_id: &str,
    run_id: &str,
    generation: Uuid,
    reason: &'static str,
) -> ConsumerOutcome {
    match repository
        .finalize_run_failure(run_id, work_id, reason)
        .await
    {
        Ok(envelope) => {
            remove_active(active, work_id, generation).await;
            let _ = publisher.publish(envelope).await;
            ConsumerOutcome::Failed
        }
        Err(_) => {
            set_faulted(active, work_id, generation, reason).await;
            ConsumerOutcome::Faulted
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
    let run_id = {
        let active = active.lock().await;
        active
            .get(&work_id)
            .filter(|entry| entry.generation == generation && entry.work_id == work_id)
            .and_then(|entry| entry.run_id.clone())
    };
    if let Some(run_id) = run_id {
        let _ = finalize_failure(
            &repository,
            &publisher,
            &active,
            &work_id,
            &run_id,
            generation,
            "Engine lifecycle task stopped unexpectedly",
        )
        .await;
    } else {
        remove_active(&active, &work_id, generation).await;
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

async fn set_run_id(active: &ActiveRuns, work_id: &str, generation: Uuid, run_id: &str) {
    let mut active = active.lock().await;
    if let Some(entry) = active
        .get_mut(work_id)
        .filter(|entry| entry.generation == generation && entry.work_id == work_id)
    {
        entry.run_id = Some(run_id.to_owned());
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
