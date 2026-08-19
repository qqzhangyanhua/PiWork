//! Assignment Scheduler: the single-owner loop that hydrates the Buzz-derived
//! `WorkQueue` from durable Assignment state, claims capacity, and drives each
//! Assignment through the `EngineHarness` to a terminal state.
//!
//! Retry and dead-letter policy lives in `AssignmentRepository` (applied by the
//! Harness); the in-memory queue only enforces single-Work in-flight,
//! global/per-Agent capacity and oldest-head fairness. A failed Assignment is
//! persisted as `queued` with a `next_attempt_at` and re-hydrated on a later
//! cycle.

use std::{collections::BTreeMap, sync::Arc, time::Duration};

use chrono::{TimeDelta, Utc};
use tokio::sync::{mpsc, Mutex};

use crate::{
    agent::repository::AgentRepository,
    assignment::{
        queue::{
            QueueClaim, QueueCompletion, QueueInput, QueueItem, QueueLimits, QueueOutcome,
            WorkQueue,
        },
        repository::AssignmentRepository,
    },
    domain::assignment::AssignmentSummary,
    engine::{
        harness::{AssignmentExecutionOutcome, AssignmentExecutionRequest, EngineHarness},
        publisher::EventPublisher, EngineAdapter, EngineInput,
    },
    error::AppError,
    work::repository::WorkRepository,
};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const HYDRATE_LIMIT: u32 = 256;

#[derive(Debug, Clone)]
pub enum SchedulerCommand {
    Wake,
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
        }
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
        let agent = self
            .agent_repository
            .get_agent_instance(&assignment.assigned_agent_id)
            .await?
            .ok_or_else(|| AppError::invalid_input("assignedAgentId", "agent instance not found"))?;
        let work = self
            .work_repository
            .get(&assignment.work_id)
            .await?
            .ok_or_else(|| AppError::work_not_found(&assignment.work_id))?;

        self.ensure_work_running(&work.summary.id).await;

        let request = AssignmentExecutionRequest {
            assignment: assignment.clone(),
            work: work.summary.clone(),
            agent,
            run: run.clone(),
            input: EngineInput {
                message: assignment.instruction.clone(),
                images: Vec::new(),
                documents: Vec::new(),
            },
            effective_permission: effective_permission(&assignment),
            runtime_owner: self.owner_id.clone(),
        };
        self.harness.execute(request).await
    }

    async fn ensure_work_running(&self, work_id: &str) {
        use crate::domain::work::WorkStatus;
        let Ok(Some(work)) = self.work_repository.get(work_id).await else {
            return;
        };
        match work.summary.status {
            WorkStatus::Running => {}
            WorkStatus::Queued => {
                let _ = self.work_repository.set_work_status(work_id, WorkStatus::Running).await;
            }
            _ => {
                let _ = self.work_repository.set_work_status(work_id, WorkStatus::Queued).await;
                let _ = self.work_repository.set_work_status(work_id, WorkStatus::Running).await;
            }
        }
    }

    async fn release_claim(
        &self,
        queue: &Arc<Mutex<WorkQueue>>,
        claim: &QueueClaim,
        outcome: Result<AssignmentExecutionOutcome, AppError>,
    ) {
        let now = Utc::now();
        let queue_outcome = match &outcome {
            Ok(AssignmentExecutionOutcome::Stopped) => QueueOutcome::Interrupted,
            Ok(AssignmentExecutionOutcome::Completed { .. })
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
        self.reflect_work_terminal(&claim.work_id, &outcome).await;
    }

    async fn reflect_work_terminal(
        &self,
        work_id: &str,
        outcome: &Result<AssignmentExecutionOutcome, AppError>,
    ) {
        use crate::domain::work::WorkStatus;
        let target = match outcome {
            Ok(AssignmentExecutionOutcome::Completed { .. }) => WorkStatus::Completed,
            Ok(AssignmentExecutionOutcome::Stopped) => WorkStatus::Stopped,
            Ok(AssignmentExecutionOutcome::Failed { .. }) | Err(_) => WorkStatus::Failed,
        };
        let _ = self.work_repository.set_work_status(work_id, target).await;
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
