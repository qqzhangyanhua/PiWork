//! Assignment-aware engine execution boundary.
//!
//! `EngineHarness::execute` runs exactly one already-claimed Assignment to a
//! terminal engine event. It owns the Agent × Work Session claim, the engine
//! start, the Run/Assignment `running` transition, and the journaling of every
//! engine event with full product identity. It does not create Assignments,
//! build layered context, or decide retry/dead-letter policy — those belong to
//! the Scheduler and the collaboration layer.

use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::Duration,
};

use chrono::Utc;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::{
    assignment::repository::AssignmentRepository,
    collaboration::{
        tool_bridge::{AuthorizedRunContext, HostToolRegistry},
        tools::role_tool_allowlist,
    },
    domain::{
        agent::AgentInstanceSummary,
        assignment::{AgentSessionSummary, AssignmentStatus, AssignmentSummary},
        event::WorkEventEnvelope,
        work::{PermissionMode, RunSummary, WorkSummary},
    },
    engine::{
        EngineAdapter, EngineEvent, EngineInput, EngineRunContext, EngineRunIdentity,
        EngineSessionRef, WAITING_ON_ASSIGNMENTS_REASON, publisher::EventPublisher,
    },
    error::AppError,
    work::repository::WorkRepository,
};

const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const STARTUP_EVENT_BUFFER_CAP: usize = 1000;
const RETRY_BASE: Duration = Duration::from_secs(5);
const RETRY_MAX: Duration = Duration::from_secs(300);

type TerminalOutcome = (String, Vec<String>, Vec<String>, Vec<String>);

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
    Waiting {
        reason: String,
    },
    Stopped,
}

/// The production Host Tool Bridge pieces the Harness needs to issue a
/// per-Run lease and revoke it when the Run ends. Optional so legacy/test
/// execution paths that never expose host tools keep working unchanged.
///
/// The endpoint is a `OnceLock` because the loopback server binds an ephemeral
/// port during app assembly, after the Scheduler (and thus this config) is
/// constructed; it is set exactly once before any Run can dispatch.
#[derive(Clone)]
pub struct HostToolBridgeConfig {
    pub registry: Arc<HostToolRegistry>,
    pub endpoint: Arc<OnceLock<String>>,
}

/// Revokes a Run's host tool lease on drop, guaranteeing the token is dead once
/// the execution (including its error paths) leaves scope.
struct LeaseGuard {
    registry: Arc<HostToolRegistry>,
    run_id: String,
}

impl Drop for LeaseGuard {
    fn drop(&mut self) {
        self.registry.revoke(&self.run_id);
    }
}

#[derive(Clone)]
pub struct EngineHarness {
    engine: Arc<dyn EngineAdapter>,
    work_repository: WorkRepository,
    assignment_repository: AssignmentRepository,
    publisher: Arc<dyn EventPublisher>,
    startup_timeout: Duration,
    host_tools: Option<HostToolBridgeConfig>,
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
            host_tools: None,
        }
    }

    /// Attaches the production host tool bridge so each executed Run receives a
    /// role-scoped lease. Without this the Pi extension is never loaded.
    pub fn with_host_tools(mut self, config: HostToolBridgeConfig) -> Self {
        self.host_tools = Some(config);
        self
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

        // Issue a role-scoped host tool lease before starting the engine. The
        // guard revokes it on every exit path (including startup failure and
        // the `?` early returns below), so a token never outlives its Run.
        let (lease, _lease_guard) = match self.host_tools.as_ref().and_then(|config| {
            config
                .endpoint
                .get()
                .cloned()
                .map(|endpoint| (config, endpoint))
        }) {
            Some((config, endpoint)) => {
                let lease = config.registry.issue(
                    AuthorizedRunContext {
                        run_id: run.id.clone(),
                        work_id: work.id.clone(),
                        assignment_id: assignment.id.clone(),
                        agent_instance_id: agent.id.clone(),
                        runtime_owner: runtime_owner.clone(),
                        allowed_tools: role_tool_allowlist(agent.definition.role_kind),
                    },
                    endpoint,
                );
                let guard = LeaseGuard {
                    registry: Arc::clone(&config.registry),
                    run_id: run.id.clone(),
                };
                (Some(lease), Some(guard))
            }
            None => (None, None),
        };
        let context = match lease {
            Some(lease) => context.with_host_tool_lease(lease),
            None => context,
        };

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
                            self.finalize_startup_failure(
                                &session,
                                &run,
                                &assignment,
                                &runtime_owner,
                                &format!("engine failed to start: {error}"),
                            )
                            .await;
                            return Err(AppError::engine_start_failed_with_reason(
                                &work.id,
                                format!("engine failed to start: {error}"),
                            ));
                        }
                    }
                }
                _ = tokio::time::sleep(self.startup_timeout) => {
                    self.finalize_startup_failure(
                        &session,
                        &run,
                        &assignment,
                        &runtime_owner,
                        "engine startup timed out",
                    )
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
                            self.finalize_startup_failure(
                                &session,
                                &run,
                                &assignment,
                                &runtime_owner,
                                "engine emitted too many events before session attachment",
                            )
                            .await;
                            return Err(AppError::engine("engine emitted too many events before session attachment"));
                        }
                        None => {
                            self.finalize_startup_failure(
                                &session,
                                &run,
                                &assignment,
                                &runtime_owner,
                                "engine event stream closed before startup",
                            )
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
        let mut terminal: Option<TerminalOutcome> = None;
        let mut failed: Option<String> = None;
        let mut waiting: Option<String> = None;
        let mut completed_by_host_tool = false;

        loop {
            let event = match buffered.pop_front() {
                Some(event) => Some(event),
                None => event_receiver.recv().await,
            };
            let Some(event) = event else {
                // Stream closed without a terminal event.
                break;
            };
            if matches!(
                event,
                EngineEvent::RunCompleted { .. } | EngineEvent::RunFailed { .. }
            ) && let Some(current) = self
                .assignment_repository
                .get_assignment(&assignment.id)
                .await?
                && current.status == AssignmentStatus::Completed
            {
                let (artifacts, validation, limitations) = match event {
                    EngineEvent::RunCompleted {
                        artifacts,
                        validation,
                        limitations,
                        ..
                    } => (artifacts, validation, limitations),
                    EngineEvent::RunFailed { .. } => (Vec::new(), Vec::new(), Vec::new()),
                    _ => unreachable!(),
                };
                terminal = Some((
                    current.result_summary.unwrap_or_else(|| "Completed".into()),
                    artifacts,
                    validation,
                    limitations,
                ));
                completed_by_host_tool = true;
                break;
            }
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
                EngineEvent::Waiting { reason } if reason == WAITING_ON_ASSIGNMENTS_REASON => {
                    waiting = Some(reason);
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
            if !completed_by_host_tool {
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
            }
            Ok(AssignmentExecutionOutcome::Completed {
                result_summary: summary,
                artifacts,
                validation,
                limitations,
            })
        } else if let Some(reason) = waiting {
            self.assignment_repository
                .mark_waiting(
                    &assignment.id,
                    &run.id,
                    &session.id,
                    &runtime_owner,
                    &reason,
                    now,
                )
                .await?;
            self.assignment_repository
                .mark_ready(&session.id, &run.id)
                .await?;
            Ok(AssignmentExecutionOutcome::Waiting { reason })
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

    /// Marks the claimed session ready and finalizes a startup failure through
    /// the retry/dead-letter policy so the Assignment never stays claimed.
    async fn finalize_startup_failure(
        &self,
        session: &AgentSessionSummary,
        run: &RunSummary,
        assignment: &AssignmentSummary,
        runtime_owner: &str,
        message: &str,
    ) {
        let _ = self
            .assignment_repository
            .mark_ready(&session.id, &run.id)
            .await;
        let _ = self
            .assignment_repository
            .fail_startup_attempt(
                &assignment.id,
                &run.id,
                &session.id,
                runtime_owner,
                message,
                Utc::now(),
                RETRY_BASE,
                RETRY_MAX,
            )
            .await;
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
