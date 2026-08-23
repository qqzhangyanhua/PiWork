use std::{sync::Arc, time::Duration};

use chrono::{TimeZone, Utc};
use piwork_lib::{
    agent::repository::AgentRepository,
    assignment::{
        repository::{AssignmentEventSink, AssignmentRepository},
        scheduler::AssignmentScheduler,
        service::AssignmentService,
    },
    domain::{assignment::AssignmentStatus, event::WorkEventEnvelope, work::WorkStatus},
    engine::{fake::FakeEngineAdapter, publisher::EventPublisher},
    error::AppError,
    execution::{ExecutionCommand, ExecutionCoordinator, WorkInput},
    storage::sqlite::Database,
    work::repository::WorkRepository,
};

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

#[tokio::test]
async fn submitted_work_can_be_stopped_once_or_repeatedly() {
    let root = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    seed_work(&pool, "work-stop", &root.path().to_string_lossy()).await;

    let publisher = Arc::new(RecordingPublisher);
    let assignments = AssignmentRepository::with_event_sink(pool.clone(), publisher.clone());
    let works = WorkRepository::new(pool.clone());
    let scheduler = AssignmentScheduler::new(
        assignments.clone(),
        works.clone(),
        AgentRepository::new(pool),
        Arc::new(FakeEngineAdapter::new(Duration::from_secs(30))),
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
    let coordinator = ExecutionCoordinator::new(assignment_service, works.clone(), handle.clone());

    coordinator
        .submit(
            "work-stop",
            WorkInput {
                instruction: "Keep running until stopped".into(),
                referenced_files: vec![],
                resource_ids: vec![],
            },
        )
        .await
        .unwrap();

    let first = coordinator
        .control("work-stop", ExecutionCommand::Stop)
        .await
        .unwrap();
    assert_eq!(first.work.summary.status, WorkStatus::Stopped);
    assert!(
        assignments
            .list_for_work("work-stop")
            .await
            .unwrap()
            .iter()
            .all(|assignment| assignment.status == AssignmentStatus::Cancelled)
    );

    let repeated = coordinator
        .control("work-stop", ExecutionCommand::Stop)
        .await
        .unwrap();
    assert_eq!(repeated.work.summary.status, WorkStatus::Stopped);
    handle.shutdown().await;
}
