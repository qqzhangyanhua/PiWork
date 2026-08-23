//! Assignment Scheduler: the single-owner loop that hydrates the Buzz-derived
//! `WorkQueue` from durable Assignment state, claims capacity, and drives each
//! Assignment through the `EngineHarness` to a terminal state.
//!
//! Retry and dead-letter policy lives in `AssignmentRepository` (applied by the
//! Harness); the in-memory queue only enforces single-Work in-flight,
//! global/per-Agent capacity and oldest-head fairness. A failed Assignment is
//! persisted as `queued` with a `next_attempt_at` and re-hydrated on a later
//! cycle.

use std::{collections::BTreeMap, collections::HashMap, sync::Arc, time::Duration};

use chrono::{TimeDelta, Utc};
use tokio::sync::{Mutex, mpsc, oneshot};

use crate::{
    agent::repository::AgentRepository,
    assignment::{
        queue::{
            QueueClaim, QueueCompletion, QueueInput, QueueItem, QueueLimits, QueueOutcome,
            WorkQueue,
        },
        repository::AssignmentRepository,
    },
    collaboration::context::{ContextBuildInput, build_assignment_context},
    domain::{agent::RoleKind, assignment::AssignmentSummary, collaboration::ResultEnvelope},
    engine::{
        EngineAdapter, EngineInput,
        harness::{
            AssignmentExecutionOutcome, AssignmentExecutionRequest, EngineHarness,
            HostToolBridgeConfig,
        },
        publisher::EventPublisher,
    },
    error::AppError,
    extensions::ExtensionService,
    memory::{MemoryCaptureRequest, MemoryRecallRequest, WorkspaceMemoryService},
    work::repository::WorkRepository,
};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const HYDRATE_LIMIT: u32 = 256;

#[derive(Debug)]
pub enum SchedulerCommand {
    Wake,
    Stop {
        work_id: String,
        response: oneshot::Sender<Result<(), AppError>>,
    },
    CancelAssignment {
        work_id: String,
        assignment_id: String,
        response: oneshot::Sender<Result<AssignmentSummary, AppError>>,
    },
    Shutdown,
}

#[derive(Clone)]
pub struct AssignmentSchedulerHandle {
    commands: mpsc::Sender<SchedulerCommand>,
}

impl AssignmentSchedulerHandle {
    pub fn wake(&self) -> Result<(), AppError> {
        self.commands
            .try_send(SchedulerCommand::Wake)
            .map_err(|_| AppError::engine("assignment scheduler is not running"))
    }

    pub async fn stop(&self, work_id: &str) -> Result<(), AppError> {
        let (response, receiver) = oneshot::channel();
        self.commands
            .send(SchedulerCommand::Stop {
                work_id: work_id.to_owned(),
                response,
            })
            .await
            .map_err(|_| AppError::engine("assignment scheduler is not running"))?;
        receiver.await.map_err(|_| {
            AppError::engine("assignment scheduler stopped before control completed")
        })?
    }

    pub async fn cancel_assignment(
        &self,
        work_id: &str,
        assignment_id: &str,
    ) -> Result<AssignmentSummary, AppError> {
        let (response, receiver) = oneshot::channel();
        self.commands
            .send(SchedulerCommand::CancelAssignment {
                work_id: work_id.to_owned(),
                assignment_id: assignment_id.to_owned(),
                response,
            })
            .await
            .map_err(|_| AppError::engine("assignment scheduler is not running"))?;
        receiver.await.map_err(|_| {
            AppError::engine("assignment scheduler stopped before control completed")
        })?
    }

    pub async fn shutdown(&self) {
        let _ = self.commands.send(SchedulerCommand::Shutdown).await;
    }
}

#[derive(Clone)]
pub struct AssignmentScheduler {
    repository: AssignmentRepository,
    work_repository: WorkRepository,
    agent_repository: AgentRepository,
    harness: EngineHarness,
    engine: Arc<dyn EngineAdapter>,
    owner_id: String,
    model_label: String,
    limits: QueueLimits,
    active_runs: Arc<Mutex<HashMap<String, (String, String)>>>,
    memory_service: Option<Arc<WorkspaceMemoryService>>,
    extension_service: Option<Arc<ExtensionService>>,
}

impl AssignmentScheduler {
    pub fn new(
        repository: AssignmentRepository,
        work_repository: WorkRepository,
        agent_repository: AgentRepository,
        engine: Arc<dyn EngineAdapter>,
        publisher: Arc<dyn EventPublisher>,
        owner_id: impl Into<String>,
        model_label: impl Into<String>,
    ) -> Self {
        let harness = EngineHarness::new(
            Arc::clone(&engine),
            work_repository.clone(),
            repository.clone(),
            publisher,
        );
        Self {
            repository,
            work_repository,
            agent_repository,
            harness,
            engine,
            owner_id: owner_id.into(),
            model_label: model_label.into(),
            limits: default_limits(),
            active_runs: Arc::new(Mutex::new(HashMap::new())),
            memory_service: None,
            extension_service: None,
        }
    }

    /// Attaches the production host tool bridge so every dispatched Run issues
    /// a role-scoped lease and loads the Pi extension.
    pub fn with_host_tools(mut self, config: HostToolBridgeConfig) -> Self {
        self.harness = self.harness.clone().with_host_tools(config);
        self
    }

    pub fn with_memory_service(mut self, memory_service: Arc<WorkspaceMemoryService>) -> Self {
        self.memory_service = Some(memory_service);
        self
    }

    pub fn with_capability_broker(mut self, broker: crate::capability::CapabilityBroker) -> Self {
        self.harness = self.harness.clone().with_capability_broker(broker);
        self
    }

    pub fn with_extension_service(mut self, extension_service: Arc<ExtensionService>) -> Self {
        self.extension_service = Some(extension_service);
        self
    }

    /// Runs orphan recovery once before the dispatch loop starts. Production
    /// startup awaits this so no non-idempotent assignment is silently re-run.
    pub async fn recover(&self) -> Result<(), AppError> {
        self.repository.recover_orphans(&[]).await?;
        self.repository
            .invalidate_owner(&self.owner_id, "scheduler startup")
            .await?;
        Ok(())
    }

    /// Spawns the loop and returns a handle for wake/shutdown.
    pub fn spawn(self) -> AssignmentSchedulerHandle {
        let (commands_tx, commands_rx) = mpsc::channel(64);
        tokio::spawn(self.run(commands_rx));
        AssignmentSchedulerHandle {
            commands: commands_tx,
        }
    }

    async fn run(self, mut commands: mpsc::Receiver<SchedulerCommand>) {
        let scheduler = Arc::new(self);
        let queue = Arc::new(Mutex::new(
            WorkQueue::hydrate(Vec::<QueueItem>::new(), scheduler.limits.clone())
                .expect("default queue limits are valid"),
        ));
        let (completed_tx, mut completed_rx) = mpsc::channel::<()>(64);

        loop {
            if let Err(error) = scheduler.replenish(&queue).await {
                eprintln!("PiWork scheduler replenish failed: {error}");
            }
            scheduler.dispatch_ready(&queue, &completed_tx).await;

            tokio::select! {
                command = commands.recv() => {
                    match command {
                        Some(SchedulerCommand::Shutdown) | None => {
                            let _ = scheduler.repository.recover_orphans(&[]).await;
                            return;
                        }
                        Some(SchedulerCommand::Wake) => {}
                        Some(SchedulerCommand::Stop { work_id, response }) => {
                            let result = scheduler.stop_work(&work_id).await;
                            let _ = response.send(result);
                        }
                        Some(SchedulerCommand::CancelAssignment { work_id, assignment_id, response }) => {
                            let result = scheduler.cancel_assignment(&work_id, &assignment_id).await;
                            let _ = response.send(result);
                        }
                    }
                }
                _ = completed_rx.recv() => {}
                _ = tokio::time::sleep(POLL_INTERVAL) => {}
            }
        }
    }

    async fn replenish(&self, queue: &Arc<Mutex<WorkQueue>>) -> Result<(), AppError> {
        let now = Utc::now();
        let schedulable = self.repository.load_schedulable(now, HYDRATE_LIMIT).await?;
        let mut queue = queue.lock().await;
        for assignment in schedulable {
            let item = queue_item(&assignment);
            if queue.push(item).is_err() {
                // Duplicate or capped; the assignment is already tracked.
                continue;
            }
        }
        Ok(())
    }

    async fn dispatch_ready(&self, queue: &Arc<Mutex<WorkQueue>>, completed_tx: &mpsc::Sender<()>) {
        let now = Utc::now();
        loop {
            let claim = { queue.lock().await.next_claim(now) };
            let Some(claim) = claim else {
                break;
            };
            let scheduler = Arc::new(self.clone());
            let queue = Arc::clone(queue);
            let completed_tx = completed_tx.clone();
            tokio::spawn(async move {
                let outcome = scheduler.execute_claim(&claim).await;
                scheduler.release_claim(&queue, &claim, outcome).await;
                let _ = completed_tx.send(()).await;
            });
        }
    }

    async fn execute_claim(
        &self,
        claim: &QueueClaim,
    ) -> Result<AssignmentExecutionOutcome, AppError> {
        let now = Utc::now();
        let assignment = self
            .repository
            .claim(&claim.item.id, &self.owner_id, now)
            .await?;
        let model_label = self
            .engine
            .model_label(&self.model_label)
            .await
            .map_err(|error| AppError::engine(error.to_string()))?;
        let run = self
            .repository
            .begin_attempt(&assignment.id, self.engine.kind(), &model_label)
            .await?;
        self.active_runs.lock().await.insert(
            assignment.work_id.clone(),
            (assignment.id.clone(), run.id.clone()),
        );
        let agent = self
            .agent_repository
            .get_agent_instance(&assignment.assigned_agent_id)
            .await?
            .ok_or_else(|| {
                AppError::invalid_input("assignedAgentId", "agent instance not found")
            })?;
        let work = self
            .work_repository
            .get(&assignment.work_id)
            .await?
            .ok_or_else(|| AppError::work_not_found(&assignment.work_id))?;

        self.ensure_work_running(&work.summary.id).await;

        let dependency_results = self
            .repository
            .validated_dependency_results(&assignment.id)
            .await?;
        let agent_memory = match self.memory_service.as_ref() {
            Some(memory) => memory
                .recall_for_assignment(MemoryRecallRequest {
                    root_path: work.summary.root_path.clone(),
                    agent_id: agent.id.clone(),
                    query: assignment.instruction.clone(),
                })
                .await
                .unwrap_or_else(|error| {
                    eprintln!("CoDo workspace memory context degraded: {error}");
                    Vec::new()
                }),
            None => Vec::new(),
        };
        let message = build_engine_prompt(
            &agent,
            &work.summary,
            &assignment,
            dependency_results,
            agent_memory,
        );

        let extension_tool_ids = match &self.extension_service {
            Some(service) => service
                .runtime_snapshot(&agent.id, &work.summary.id)
                .await
                .map(|snapshot| snapshot.tool_ids)
                .unwrap_or_else(|error| {
                    eprintln!("CoDo extension capability snapshot degraded: {error}");
                    Vec::new()
                }),
            None => Vec::new(),
        };
        let request = AssignmentExecutionRequest {
            assignment: assignment.clone(),
            work: work.summary.clone(),
            agent,
            run: run.clone(),
            input: EngineInput {
                message,
                images: Vec::new(),
                documents: Vec::new(),
            },
            effective_permission: effective_permission(&assignment),
            runtime_owner: self.owner_id.clone(),
            extension_tool_ids,
        };
        let outcome = self.harness.execute(request).await;
        if let (Some(memory), Ok(AssignmentExecutionOutcome::Completed { result_summary, .. })) =
            (self.memory_service.as_ref(), &outcome)
        {
            let capture = MemoryCaptureRequest {
                root_path: work.summary.root_path.clone(),
                work_id: work.summary.id.clone(),
                assignment_id: assignment.id.clone(),
                run_id: run.id.clone(),
                agent_id: assignment.assigned_agent_id.clone(),
                user_message: assignment.instruction.clone(),
                assistant_message: result_summary.clone(),
            };
            if let Err(error) = memory.queue_capture(capture).await {
                eprintln!("CoDo workspace memory capture was not queued: {error}");
            }
        }
        outcome
    }

    async fn ensure_work_running(&self, work_id: &str) {
        let _ = self.work_repository.reproject_work_status(work_id).await;
    }

    async fn release_claim(
        &self,
        queue: &Arc<Mutex<WorkQueue>>,
        claim: &QueueClaim,
        outcome: Result<AssignmentExecutionOutcome, AppError>,
    ) {
        let now = Utc::now();
        {
            let mut active = self.active_runs.lock().await;
            if active
                .get(&claim.work_id)
                .is_some_and(|(assignment_id, _)| assignment_id == &claim.item.id)
            {
                active.remove(&claim.work_id);
            }
        }
        let queue_outcome = match &outcome {
            Ok(AssignmentExecutionOutcome::Stopped) => QueueOutcome::Interrupted,
            Ok(AssignmentExecutionOutcome::Completed { .. })
            | Ok(AssignmentExecutionOutcome::Waiting { .. })
            | Ok(AssignmentExecutionOutcome::Failed { .. })
            | Err(_) => QueueOutcome::Completed,
        };
        let completion = QueueCompletion {
            claim_id: claim.id.clone(),
            completed_at: now,
            observed_at: now,
            outcome: queue_outcome,
        };
        let _ = queue.lock().await.release(completion);
        let _ = self
            .work_repository
            .reproject_work_status(&claim.work_id)
            .await;
    }

    async fn stop_work(&self, work_id: &str) -> Result<(), AppError> {
        let active = self.active_runs.lock().await.get(work_id).cloned();
        if let Some((_assignment_id, run_id)) = active {
            match self.engine.abort(&run_id).await {
                Ok(()) | Err(crate::engine::EngineError::NotRunning) => {}
                Err(crate::engine::EngineError::CleanupUnconfirmed) => {
                    self.repository
                        .interrupt_work(
                            work_id,
                            "engine cleanup could not be confirmed",
                            Utc::now(),
                        )
                        .await?;
                    return Err(AppError::engine("engine cleanup could not be confirmed"));
                }
                Err(error) => return Err(AppError::engine(error.to_string())),
            }
        }
        self.repository
            .stop_work(work_id, "stopped by user", Utc::now())
            .await?;
        self.active_runs.lock().await.remove(work_id);
        self.work_repository.reproject_work_status(work_id).await?;
        Ok(())
    }

    async fn cancel_assignment(
        &self,
        work_id: &str,
        assignment_id: &str,
    ) -> Result<AssignmentSummary, AppError> {
        let active = self.active_runs.lock().await.get(work_id).cloned();
        let Some((active_assignment_id, run_id)) = active else {
            return Err(AppError::invalid_input(
                "assignmentId",
                "assignment is not actively running",
            ));
        };
        if active_assignment_id != assignment_id {
            return Err(AppError::invalid_input(
                "assignmentId",
                "assignment is not the active Work assignment",
            ));
        }
        match self.engine.abort(&run_id).await {
            Ok(()) | Err(crate::engine::EngineError::NotRunning) => {}
            Err(error) => return Err(AppError::engine(error.to_string())),
        }
        let assignment = self
            .repository
            .cancel(
                assignment_id,
                &run_id,
                &self.owner_id,
                "cancelled by parent Lead",
                Utc::now(),
            )
            .await?;
        self.active_runs.lock().await.remove(work_id);
        self.work_repository.reproject_work_status(work_id).await?;
        Ok(assignment)
    }
}

fn queue_item(assignment: &AssignmentSummary) -> QueueItem {
    QueueItem {
        id: assignment.id.clone(),
        work_id: assignment.work_id.clone(),
        agent_id: assignment.assigned_agent_id.clone(),
        created_at: assignment.created_at,
        not_before: assignment.not_before.unwrap_or(assignment.created_at),
        retry_count: assignment.attempt_count,
        inputs: vec![QueueInput {
            id: assignment.id.clone(),
            created_at: assignment.created_at,
        }],
    }
}

/// Builds the fixed eight-layer context for the Assignment and returns the
/// rendered, budget-bounded engine prompt.
fn build_engine_prompt(
    agent: &crate::domain::agent::AgentInstanceSummary,
    work: &crate::domain::work::WorkSummary,
    assignment: &AssignmentSummary,
    dependency_results: Vec<ResultEnvelope>,
    agent_memory: Vec<String>,
) -> String {
    let capability_packs = assignment
        .capability_pack_id
        .as_ref()
        .map(|selected| {
            agent
                .definition
                .capability_packs
                .iter()
                .filter(|pack| &pack.id == selected)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let input = ContextBuildInput {
        is_lead: agent.definition.role_kind == RoleKind::Lead,
        agent_definition: agent.definition.clone(),
        capability_packs,
        work: work.clone(),
        assignment: assignment.clone(),
        dependency_results,
        agent_memory,
        ..ContextBuildInput::default()
    };
    build_assignment_context(input).rendered_prompt
}

fn effective_permission(assignment: &AssignmentSummary) -> crate::domain::work::PermissionMode {
    match assignment
        .permission_scope
        .get("mode")
        .and_then(|mode| mode.as_str())
    {
        Some("ask_every_step") => crate::domain::work::PermissionMode::AskEveryStep,
        Some("auto_execute") => crate::domain::work::PermissionMode::AutoExecute,
        _ => crate::domain::work::PermissionMode::Balanced,
    }
}

fn default_limits() -> QueueLimits {
    QueueLimits {
        max_pending_per_work: 128,
        max_total_items: 1024,
        max_works: 128,
        max_batch_size: 64,
        global_parallelism: 4,
        default_agent_parallelism: 1,
        agent_parallelism: BTreeMap::new(),
        in_flight_timeout: TimeDelta::hours(24),
        max_retries: 16,
        retry_base: TimeDelta::seconds(5),
        retry_max: TimeDelta::minutes(5),
    }
}
