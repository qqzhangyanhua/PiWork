use std::{collections::HashSet, path::PathBuf, sync::Arc};

use chrono::Utc;
use tokio::sync::{Mutex, mpsc};

use crate::{
    domain::{
        event::{WorkEventEnvelope, WorkEventPayload},
        work::RunSummary,
    },
    engine::{EngineAdapter, EngineEvent, EngineRunContext},
    error::AppError,
    work::repository::WorkRepository,
};

use super::publisher::EventPublisher;

#[derive(Clone)]
pub struct EngineSupervisor {
    repository: WorkRepository,
    engine: Arc<dyn EngineAdapter>,
    publisher: Arc<dyn EventPublisher>,
    active: Arc<Mutex<HashSet<String>>>,
}

impl EngineSupervisor {
    pub fn new(
        repository: WorkRepository,
        engine: Arc<dyn EngineAdapter>,
        publisher: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            repository,
            engine,
            publisher,
            active: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    pub async fn start(&self, work_id: &str, prompt: &str) -> Result<RunSummary, AppError> {
        {
            let mut active = self.active.lock().await;
            if !active.insert(work_id.to_owned()) {
                return Err(AppError::work_already_running(work_id));
            }
        }

        match self.start_reserved(work_id, prompt).await {
            Ok(run) => Ok(run),
            Err(error) => {
                self.active.lock().await.remove(work_id);
                Err(error)
            }
        }
    }

    async fn start_reserved(&self, work_id: &str, prompt: &str) -> Result<RunSummary, AppError> {
        let work = self
            .repository
            .get(work_id)
            .await?
            .ok_or_else(|| AppError::work_not_found(work_id))?;
        let run = self
            .repository
            .begin_run(work_id, prompt, self.engine.kind())
            .await?;
        let context = match EngineRunContext::new(
            work_id.to_owned(),
            run.id.clone(),
            PathBuf::from(work.summary.root_path),
            work.summary.permission_mode,
        ) {
            Ok(context) => context,
            Err(error) => {
                self.repository.fail_run(&run.id).await?;
                return Err(AppError::engine(error.to_string()));
            }
        };
        let (sender, receiver) = mpsc::channel(16);
        if let Err(error) = self
            .engine
            .start(context, prompt.trim().to_owned(), sender)
            .await
        {
            self.repository.fail_run(&run.id).await?;
            return Err(AppError::engine(error.to_string()));
        }

        let repository = self.repository.clone();
        let publisher = Arc::clone(&self.publisher);
        let active = Arc::clone(&self.active);
        let owned_work_id = work_id.to_owned();
        let run_id = run.id.clone();
        tokio::spawn(async move {
            consume_events(
                repository,
                publisher,
                active,
                owned_work_id,
                run_id,
                receiver,
            )
            .await;
        });

        Ok(run)
    }
}

async fn consume_events(
    repository: WorkRepository,
    publisher: Arc<dyn EventPublisher>,
    active: Arc<Mutex<HashSet<String>>>,
    work_id: String,
    run_id: String,
    mut receiver: mpsc::Receiver<EngineEvent>,
) {
    let mut sequence = 1_u32;
    let mut terminal_seen = false;
    let mut journal_failed = false;
    while let Some(event) = receiver.recv().await {
        let terminal = event.is_terminal();
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
            journal_failed = true;
            break;
        }
        let _ = publisher.publish(envelope).await;
        if terminal {
            terminal_seen = true;
            break;
        }
        let Some(next) = sequence.checked_add(1) else {
            journal_failed = true;
            break;
        };
        sequence = next;
    }
    if !terminal_seen {
        if journal_failed {
            let _ = repository.fail_run(&run_id).await;
        } else {
            let failure = WorkEventEnvelope {
                version: 1,
                work_id: work_id.clone(),
                run_id: run_id.clone(),
                sequence,
                occurred_at: Utc::now(),
                payload: WorkEventPayload::RunFailed {
                    message: "Engine event stream closed before a terminal event".into(),
                },
            };
            if repository
                .append_event_and_transition(&failure)
                .await
                .is_ok()
            {
                let _ = publisher.publish(failure).await;
            } else {
                let _ = repository.fail_run(&run_id).await;
            }
        }
    }
    active.lock().await.remove(&work_id);
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
