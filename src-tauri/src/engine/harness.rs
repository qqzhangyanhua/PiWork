//! Assignment-aware engine execution boundary.
//!
//! `EngineHarness::execute` runs exactly one already-claimed Assignment to a
//! terminal engine event. It owns the Agent × Work Session claim, the engine
//! start, the Run/Assignment `running` transition, and the journaling of every
//! engine event with full product identity. It does not create Assignments,
//! build layered context, or decide retry/dead-letter policy — those belong to
//! the Scheduler and the collaboration layer.

use std::{collections::VecDeque, path::PathBuf, sync::Arc, time::Duration};

use chrono::Utc;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{
    assignment::repository::AssignmentRepository,
    domain::{
        agent::AgentInstanceSummary,
        assignment::{AgentSessionSummary, AssignmentSummary},
        event::WorkEventEnvelope,
        work::{PermissionMode, RunSummary, WorkSummary},
    },
    engine::{
        EngineAdapter, EngineEvent, EngineInput, EngineRunContext, EngineRunIdentity,
        EngineSessionRef, publisher::EventPublisher,
    },
    error::AppError,
    work::repository::WorkRepository,
};

const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const STARTUP_EVENT_BUFFER_CAP: usize = 1000;
const RETRY_BASE: Duration = Duration::from_secs(5);
const RETRY_MAX: Duration = Duration::from_secs(300);

/// Everything the Harness needs to execute one Assignment; the Scheduler
/// assembles this after a successful claim + attempt begin.
#[derive(Debug, Clone)]
pub struct AssignmentExecutionRequest {
    pub assignment: AssignmentSummary,
    pub work: WorkSummary,
    pub agent: AgentInstanceSummary,
    pub run: RunSummary,
    pub input: EngineInput,
    pub effective_permission: PermissionMode,
    pub runtime_owner: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssignmentExecutionOutcome {
    Completed {
        result_summary: String,
        artifacts: Vec<String>,
        validation: Vec<String>,
        limitations: Vec<String>,
    },
    Failed {
        message: String,
    },
    Stopped,
}

#[derive(Clone)]
pub struct EngineHarness {
    engine: Arc<dyn EngineAdapter>,
    work_repository: WorkRepository,
    assignment_repository: AssignmentRepository,
    publisher: Arc<dyn EventPublisher>,
    startup_timeout: Duration,
}

impl EngineHarness {
    pub fn new(
        engine: Arc<dyn EngineAdapter>,
        work_repository: WorkRepository,
        assignment_repository: AssignmentRepository,
        publisher: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            engine,
            work_repository,
            assignment_repository,
            publisher,
            startup_timeout: DEFAULT_STARTUP_TIMEOUT,
        }
    }

    #[cfg(test)]
    pub fn with_startup_timeout(mut self, timeout: Duration) -> Self {
        self.startup_timeout = timeout;
        self
    }

    pub async fn execute(
        &self,
        request: AssignmentExecutionRequest,
    ) -> Result<AssignmentExecutionOutcome, AppError> {
        let AssignmentExecutionRequest {
            assignment,
            work,
            agent,
            run,
            input,
            effective_permission,
            runtime_owner,
        } = request;

        let session = self
            .assignment_repository
            .claim_or_create_session(&agent.id, &work.id, self.engine.kind(), &runtime_owner)
            .await?;

        let identity = EngineRunIdentity::new(
            work.id.clone(),
            run.id.clone(),
            assignment.id.clone(),
            agent.id.clone(),
            session.id.clone(),
            session.generation,
        )
        .map_err(|error| AppError::engine(error.to_string()))?;
        let context = EngineRunContext::new(
            identity,
            PathBuf::from(&work.root_path),
            work.permission_mode,
            agent.model_configuration_override.clone(),
            effective_permission,
        )
        .map_err(|error| AppError::engine(error.to_string()))?;

        let (event_sender, mut event_receiver) = mpsc::channel(64);
        let start = self.engine.start(context, input, event_sender);
        tokio::pin!(start);

        // Buffer engine events emitted before the engine session is attached so
        // the Run is not journaled against a missing or queued identity.
        let mut buffered: VecDeque<EngineEvent> = VecDeque::new();
        let session_ref: EngineSessionRef = loop {
            tokio::select! {
                result = &mut start => {
                    match result {
                        Ok(session_ref) => break session_ref,
                        Err(error) => {
                            let _ = self
                                .assignment_repository
                                .mark_ready(&session.id, &run.id)
                                .await;
                            return Err(AppError::engine_start_failed_with_reason(
                                &work.id,
                                &format!("engine failed to start: {error}"),
                            ));
                        }
                    }
                }
                _ = tokio::time::sleep(self.startup_timeout) => {
                    let _ = self
                        .assignment_repository
                        .mark_ready(&session.id, &run.id)
                        .await;
                    return Err(AppError::engine_start_failed_with_reason(
                        &work.id,
                        "engine startup timed out",
                    ));
                }
                event = event_receiver.recv() => {
                    match event {
                        Some(event) if buffered.len() < STARTUP_EVENT_BUFFER_CAP => {
                            buffered.push_back(event);
                        }
                        Some(_) => {
                            let _ = self
                                .assignment_repository
                                .mark_ready(&session.id, &run.id)
                                .await;
                            return Err(AppError::engine("engine emitted too many events before session attachment"));
                        }
                        None => {
                            let _ = self
                                .assignment_repository
                                .mark_ready(&session.id, &run.id)
                                .await;
                            return Err(AppError::engine_start_failed_with_reason(
                                &work.id,
                                "engine event stream closed before startup",
                            ));
                        }
                    }
                }
            }
        };

        self.assignment_repository
            .attach_engine_reference(&session.id, &session_ref.session_id)
            .await?;
        self.assignment_repository
            .mark_running(
                &assignment.id,
                &run.id,
                &session.id,
                &runtime_owner,
                Utc::now(),
            )
            .await?;

        let mut sequence = self.work_repository.next_run_sequence(&run.id).await?;
        let mut previous_event_id: Option<String> = None;
        let mut terminal: Option<(String, Vec<String>, Vec<String>, Vec<String>)> = None;
        let mut failed: Option<String> = None;

        loop {
            let event = match buffered.pop_front() {
                Some(event) => Some(event),
                None => event_receiver.recv().await,
            };
            let Some(event) = event else {
                // Stream closed without a terminal event.
                break;
            };
            let envelope = self.envelope(
                &work,
                &run,
                &assignment,
                &session,
                &agent,
                &event,
                sequence,
                previous_event_id.clone(),
            );
            previous_event_id = envelope.event_id.clone();
            self.work_repository.journal_engine_event(&envelope).await?;
            let _ = self.publisher.publish(envelope.clone()).await;

            match event {
                EngineEvent::RunCompleted {
                    summary,
                    artifacts,
                    validation,
                    limitations,
                } => {
                    terminal = Some((summary, artifacts, validation, limitations));
                    break;
                }
                EngineEvent::RunFailed { message } => {
                    failed = Some(message);
                    break;
                }
                _ => {}
            }
            let Some(next) = sequence.checked_add(1) else {
                return Err(AppError::engine("engine event sequence limit was reached"));
            };
            sequence = next;
        }

        let now = Utc::now();
        if let Some((summary, artifacts, validation, limitations)) = terminal {
            self.assignment_repository
                .mark_ready(&session.id, &run.id)
                .await?;
            self.assignment_repository
                .complete(
                    &assignment.id,
                    &run.id,
                    &session.id,
                    &runtime_owner,
                    &summary,
                    now,
                )
                .await?;
            Ok(AssignmentExecutionOutcome::Completed {
                result_summary: summary,
                artifacts,
                validation,
                limitations,
            })
        } else {
            let message = failed
                .unwrap_or_else(|| "engine event stream closed before a terminal event".to_owned());
            self.assignment_repository
                .mark_ready(&session.id, &run.id)
                .await?;
            self.finalize_failure(&assignment, &run, &session, &runtime_owner, &message, now)
                .await?;
            Ok(AssignmentExecutionOutcome::Failed { message })
        }
    }

    /// Persists a failed attempt: schedule a retry while attempts remain,
    /// otherwise dead-letter. Both leave the Run failed and the Assignment
    /// terminal or re-queued so the Scheduler can re-hydrate it later.
    async fn finalize_failure(
        &self,
        assignment: &AssignmentSummary,
        run: &RunSummary,
        session: &AgentSessionSummary,
        runtime_owner: &str,
        message: &str,
        now: chrono::DateTime<Utc>,
    ) -> Result<(), AppError> {
        let retried = self
            .assignment_repository
            .fail_and_schedule_retry(
                &assignment.id,
                &run.id,
                &session.id,
                runtime_owner,
                message,
                now,
                RETRY_BASE,
                RETRY_MAX,
            )
            .await;
        if retried.is_err() {
            self.assignment_repository
                .dead_letter(
                    &assignment.id,
                    &run.id,
                    &session.id,
                    runtime_owner,
                    message,
                    now,
                )
                .await?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn envelope(
        &self,
        work: &WorkSummary,
        run: &RunSummary,
        assignment: &AssignmentSummary,
        session: &AgentSessionSummary,
        agent: &AgentInstanceSummary,
        event: &EngineEvent,
        sequence: u32,
        causation_id: Option<String>,
    ) -> WorkEventEnvelope {
        WorkEventEnvelope {
            version: 2,
            event_id: Some(Uuid::new_v4().to_string()),
            work_id: work.id.clone(),
            run_id: Some(run.id.clone()),
            turn_id: Some(run.id.clone()),
            session_id: Some(session.id.clone()),
            agent_id: Some(agent.id.clone()),
            assignment_id: Some(assignment.id.clone()),
            causation_id,
            correlation_id: Some(run.id.clone()),
            sequence,
            occurred_at: Utc::now(),
            payload: event.clone().into(),
        }
    }
}
