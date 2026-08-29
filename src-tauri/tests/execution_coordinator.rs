use std::{sync::Arc, time::Duration};

use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use piwork_lib::{
    agent::repository::AgentRepository,
    assignment::{
        event_outbox::AssignmentEventSink,
        repository::{AcceptAssignmentInput, AssignmentRepository},
        scheduler::AssignmentScheduler,
        service::AssignmentService,
    },
    domain::{
        assignment::{AssignmentKind, AssignmentSideEffect, AssignmentStatus},
        event::WorkEventEnvelope,
        work::{RunStatus, WorkStatus},
    },
    engine::{
        EngineCapabilities,
        fake::{FakeEngineAdapter, FakeEngineConfig, FakeRunBehavior},
        publisher::EventPublisher,
    },
    error::AppError,
    execution::{ExecutionCommand, ExecutionCoordinator, ExecutionOutcome, WorkInput},
    storage::sqlite::Database,
    work::repository::WorkRepository,
};
use serde_json::json;

#[derive(Default)]
struct RecordingPublisher;

impl AssignmentEventSink for RecordingPublisher {
    fn publish(&self, _event: WorkEventEnvelope) -> Result<(), AppError> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl EventPublisher for RecordingPublisher {
    async fn publish(&self, _event: WorkEventEnvelope) -> Result<(), AppError> {
        Ok(())
    }
}

async fn seed_work(pool: &sqlx::SqlitePool, work_id: &str, root_path: &str) {
    let now = Utc.with_ymd_and_hms(2026, 8, 22, 10, 0, 0).unwrap();
    sqlx::query(
        "INSERT INTO works (id, title, goal, root_path, permission_mode, status, created_at, updated_at) \
         VALUES (?, 'Execution contract', 'Stop reliably', ?, 'balanced', 'draft', ?, ?)",
    )
    .bind(work_id)
    .bind(root_path)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO work_agents (work_id, agent_instance_id, role_kind, status, permission_policy, joined_at, updated_at) \
         VALUES (?, 'agent-instance:piwork-lead', 'lead', 'joined', 'inherit_work', ?, ?)",
    )
    .bind(work_id)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO work_leads (work_id, agent_instance_id, created_at) \
         VALUES (?, 'agent-instance:piwork-lead', ?)",
    )
    .bind(work_id)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
}

struct CoordinatorHarness {
    assignments: AssignmentRepository,
    works: WorkRepository,
    engine: Arc<FakeEngineAdapter>,
    coordinator: ExecutionCoordinator,
    handle: piwork_lib::assignment::scheduler::AssignmentSchedulerHandle,
}

impl CoordinatorHarness {
    async fn new(work_id: &str, root_path: &str, engine: FakeEngineAdapter) -> Self {
        let database = Database::open_in_memory().await.unwrap();
        let pool = database.pool().clone();
        seed_work(&pool, work_id, root_path).await;

        let publisher = Arc::new(RecordingPublisher);
        let assignments = AssignmentRepository::with_event_sink(pool.clone(), publisher.clone());
        let works = WorkRepository::new(pool.clone());
        let engine = Arc::new(engine);
        let scheduler = AssignmentScheduler::new(
            assignments.clone(),
            works.clone(),
            AgentRepository::new(pool),
            Arc::clone(&engine) as _,
            publisher,
            "execution-contract-owner",
            "fake-model",
        );
        let handle = scheduler.spawn();
        let assignment_service = Arc::new(AssignmentService::new(
            assignments.clone(),
            works.clone(),
            handle.clone(),
        ));
        let coordinator =
            ExecutionCoordinator::new(assignment_service, works.clone(), handle.clone());
        Self {
            assignments,
            works,
            engine,
            coordinator,
            handle,
        }
    }

    async fn shutdown(self) {
        self.handle.shutdown().await;
    }
}

async fn wait_until_work_status(works: &WorkRepository, work_id: &str, expected: WorkStatus) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let actual = works
            .get(work_id)
            .await
            .unwrap()
            .map(|detail| detail.summary.status);
        if actual == Some(expected) {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("work {work_id} did not reach {expected:?}; actual status: {actual:?}");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn wait_until_assignment_status(
    assignments: &AssignmentRepository,
    assignment_id: &str,
    expected: AssignmentStatus,
) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let actual = assignments
            .get_assignment(assignment_id)
            .await
            .unwrap()
            .map(|assignment| assignment.status);
        if actual == Some(expected) {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "assignment {assignment_id} did not reach {expected:?}; actual status: {actual:?}"
            );
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn stopping_queued_work_cancels_assignments_without_starting_the_engine() {
    let root = tempfile::tempdir().unwrap();
    let harness = CoordinatorHarness::new(
        "work-stop-queued",
        &root.path().to_string_lossy(),
        FakeEngineAdapter::new(Duration::from_secs(30)),
    )
    .await;

    let not_before = Utc::now() + ChronoDuration::hours(1);
    let assignment = harness
        .assignments
        .accept(AcceptAssignmentInput {
            id: None,
            work_id: "work-stop-queued".into(),
            parent_assignment_id: None,
            created_by_agent_id: None,
            assigned_agent_id: "agent-instance:piwork-lead".into(),
            capability_pack_id: None,
            kind: AssignmentKind::Lead,
            side_effect: AssignmentSideEffect::Unknown,
            title: "Queued lead".into(),
            instruction: "Stay queued until stopped".into(),
            context_manifest: json!({}),
            expected_result_schema: json!({}),
            acceptance_criteria: json!([]),
            permission_scope: json!({"mode": "inherit_work"}),
            priority: 10,
            max_attempts: 3,
            not_before: Some(not_before),
        })
        .await
        .unwrap();
    harness
        .works
        .reproject_work_status("work-stop-queued")
        .await
        .unwrap();
    assert_eq!(
        harness
            .works
            .get("work-stop-queued")
            .await
            .unwrap()
            .unwrap()
            .summary
            .status,
        WorkStatus::Queued
    );

    let receipt = harness
        .coordinator
        .control("work-stop-queued", ExecutionCommand::Stop)
        .await
        .unwrap();

    assert_eq!(receipt.outcome, ExecutionOutcome::Stopped);
    assert_eq!(receipt.work.summary.status, WorkStatus::Stopped);
    assert_eq!(
        harness
            .assignments
            .get_assignment(&assignment.id)
            .await
            .unwrap()
            .unwrap()
            .status,
        AssignmentStatus::Cancelled
    );
    assert!(
        harness
            .engine
            .observations()
            .await
            .started_run_ids
            .is_empty(),
        "stopping a queued Work must not start the engine"
    );
    assert!(
        harness
            .engine
            .observations()
            .await
            .aborted_run_ids
            .is_empty()
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn stopping_running_work_aborts_the_engine_and_aligns_run_assignment_work() {
    let root = tempfile::tempdir().unwrap();
    let harness = CoordinatorHarness::new(
        "work-stop-running",
        &root.path().to_string_lossy(),
        hold_until_abort_engine(),
    )
    .await;

    let submission = harness
        .coordinator
        .submit(
            "work-stop-running",
            WorkInput {
                instruction: "Keep running until stopped".into(),
                referenced_files: vec![],
                resource_ids: vec![],
            },
        )
        .await
        .unwrap();
    let assignment_id = submission.output.assignment.id.clone();

    wait_until_work_status(&harness.works, "work-stop-running", WorkStatus::Running).await;
    wait_until_assignment_status(
        &harness.assignments,
        &assignment_id,
        AssignmentStatus::Running,
    )
    .await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if !harness
            .engine
            .observations()
            .await
            .started_run_ids
            .is_empty()
        {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            panic!("engine never started the running Work");
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    let receipt = harness
        .coordinator
        .control("work-stop-running", ExecutionCommand::Stop)
        .await
        .unwrap();

    assert_eq!(receipt.outcome, ExecutionOutcome::Stopped);
    assert_eq!(receipt.work.summary.status, WorkStatus::Stopped);
    assert!(
        receipt
            .work
            .runs
            .iter()
            .all(|run| run.status == RunStatus::Stopped),
        "every Run must stop with the Work"
    );
    assert!(
        harness
            .assignments
            .list_for_work("work-stop-running")
            .await
            .unwrap()
            .iter()
            .all(|assignment| assignment.status == AssignmentStatus::Cancelled)
    );
    assert!(
        !harness
            .engine
            .observations()
            .await
            .aborted_run_ids
            .is_empty(),
        "stopping a running Work must abort the active engine run"
    );

    harness.shutdown().await;
}

#[tokio::test]
async fn stopping_waiting_lead_cancels_waiting_and_does_not_auto_resume() {
    let root = tempfile::tempdir().unwrap();
    let harness = CoordinatorHarness::new(
        "work-stop-waiting",
        &root.path().to_string_lossy(),
        FakeEngineAdapter::new(Duration::ZERO),
    )
    .await;

    let submission = harness
        .coordinator
        .submit(
            "work-stop-waiting",
            WorkInput {
                instruction: "Finish and wait for delivery".into(),
                referenced_files: vec![],
                resource_ids: vec![],
            },
        )
        .await
        .unwrap();
    let assignment_id = submission.output.assignment.id.clone();

    wait_until_work_status(&harness.works, "work-stop-waiting", WorkStatus::Waiting).await;
    wait_until_assignment_status(
        &harness.assignments,
        &assignment_id,
        AssignmentStatus::Waiting,
    )
    .await;
    let started_before_stop = harness.engine.observations().await.started_run_ids.len();

    let receipt = harness
        .coordinator
        .control("work-stop-waiting", ExecutionCommand::Stop)
        .await
        .unwrap();

    assert_eq!(receipt.outcome, ExecutionOutcome::Stopped);
    assert_eq!(receipt.work.summary.status, WorkStatus::Stopped);
    assert_eq!(
        harness
            .assignments
            .get_assignment(&assignment_id)
            .await
            .unwrap()
            .unwrap()
            .status,
        AssignmentStatus::Cancelled
    );
    assert!(
        receipt
            .work
            .runs
            .iter()
            .any(|run| run.status == RunStatus::Stopped),
        "stopping a waiting Lead must explicitly stop the waiting Run"
    );
    assert!(
        receipt
            .work
            .runs
            .iter()
            .all(|run| run.status != RunStatus::Waiting),
        "no Run may remain Waiting after Stop"
    );

    harness.handle.wake().unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        harness
            .assignments
            .get_assignment(&assignment_id)
            .await
            .unwrap()
            .unwrap()
            .status,
        AssignmentStatus::Cancelled,
        "a stopped waiting Lead must not auto-resume after scheduler wake"
    );
    assert_eq!(
        harness
            .works
            .get("work-stop-waiting")
            .await
            .unwrap()
            .unwrap()
            .summary
            .status,
        WorkStatus::Stopped
    );
    assert_eq!(
        harness.engine.observations().await.started_run_ids.len(),
        started_before_stop,
        "stopping a waiting Lead must not start another engine run"
    );

    harness.shutdown().await;
}

fn hold_until_abort_engine() -> FakeEngineAdapter {
    FakeEngineAdapter::configured(
        FakeEngineConfig::new(EngineCapabilities {
            cancel: true,
            thought_stream: true,
            plan_updates: true,
            ..EngineCapabilities::default()
        })
        .with_run_behavior(FakeRunBehavior::HoldUntilAbort),
    )
}

#[tokio::test]
async fn stopping_an_already_stopped_work_is_idempotent() {
    let root = tempfile::tempdir().unwrap();
    let harness = CoordinatorHarness::new(
        "work-stop-repeat",
        &root.path().to_string_lossy(),
        hold_until_abort_engine(),
    )
    .await;

    harness
        .coordinator
        .submit(
            "work-stop-repeat",
            WorkInput {
                instruction: "Keep running until stopped".into(),
                referenced_files: vec![],
                resource_ids: vec![],
            },
        )
        .await
        .unwrap();
    wait_until_work_status(&harness.works, "work-stop-repeat", WorkStatus::Running).await;

    let first = harness
        .coordinator
        .control("work-stop-repeat", ExecutionCommand::Stop)
        .await
        .unwrap();
    assert_eq!(first.outcome, ExecutionOutcome::Stopped);
    assert_eq!(first.work.summary.status, WorkStatus::Stopped);
    let aborted_after_first = harness.engine.observations().await.aborted_run_ids.len();
    assert!(aborted_after_first > 0);

    let repeated = harness
        .coordinator
        .control("work-stop-repeat", ExecutionCommand::Stop)
        .await
        .unwrap();
    assert_eq!(repeated.outcome, ExecutionOutcome::Stopped);
    assert_eq!(repeated.work.summary.status, WorkStatus::Stopped);
    assert_eq!(
        harness.engine.observations().await.aborted_run_ids.len(),
        aborted_after_first,
        "a repeated Stop must not abort the engine again"
    );
    assert!(
        harness
            .assignments
            .list_for_work("work-stop-repeat")
            .await
            .unwrap()
            .iter()
            .all(|assignment| assignment.status == AssignmentStatus::Cancelled)
    );

    harness.shutdown().await;
}
