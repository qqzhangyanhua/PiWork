use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use piwork_lib::{
    agent::repository::AgentRepository,
    assignment::{
        repository::{AcceptAssignmentInput, AssignmentEventSink, AssignmentRepository},
        scheduler::AssignmentScheduler,
        service::AssignmentService,
    },
    domain::{
        assignment::{AssignmentKind, AssignmentSideEffect, AssignmentStatus},
        event::WorkEventEnvelope,
    },
    engine::{fake::FakeEngineAdapter, publisher::EventPublisher},
    error::AppError,
    storage::sqlite::Database,
    work::repository::WorkRepository,
};
use serde_json::json;

#[derive(Default)]
struct RecordingPublisher(Mutex<Vec<WorkEventEnvelope>>);

impl AssignmentEventSink for RecordingPublisher {
    fn publish(&self, event: WorkEventEnvelope) -> Result<(), AppError> {
        self.0.lock().unwrap().push(event);
        Ok(())
    }
}

#[async_trait]
impl EventPublisher for RecordingPublisher {
    async fn publish(&self, envelope: WorkEventEnvelope) -> Result<(), AppError> {
        self.0.lock().unwrap().push(envelope);
        Ok(())
    }
}

async fn seed_schedulable_work(pool: &sqlx::SqlitePool, work_id: &str, root_path: &str) {
    let now = Utc.with_ymd_and_hms(2026, 8, 16, 10, 0, 0).unwrap();
    sqlx::query(
        "INSERT INTO works (id, title, goal, root_path, permission_mode, status, created_at, updated_at) \
         VALUES (?, 'Scheduler test', 'test', ?, 'balanced', 'draft', ?, ?)",
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
        "INSERT INTO work_leads (work_id, agent_instance_id, created_at) VALUES (?, 'agent-instance:piwork-lead', ?)",
    )
    .bind(work_id)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
}

async fn accept_lead(repository: &AssignmentRepository, work_id: &str) -> String {
    let assignment = repository
        .accept(AcceptAssignmentInput {
            id: None,
            work_id: work_id.into(),
            parent_assignment_id: None,
            created_by_agent_id: None,
            assigned_agent_id: "agent-instance:piwork-lead".into(),
            capability_pack_id: None,
            kind: AssignmentKind::Lead,
            side_effect: AssignmentSideEffect::ReadOnly,
            title: "Lead task".into(),
            instruction: "Complete the lead assignment".into(),
            context_manifest: json!({}),
            expected_result_schema: json!({}),
            acceptance_criteria: json!([]),
            permission_scope: json!({"mode": "inherit_work"}),
            priority: 10,
            max_attempts: 3,
            not_before: None,
        })
        .await
        .unwrap();
    assignment.id
}

async fn assignment_status(pool: &sqlx::SqlitePool, assignment_id: &str) -> String {
    sqlx::query_scalar::<_, String>("SELECT status FROM assignments WHERE id = ?")
        .bind(assignment_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn scheduler_runs_a_claimed_lead_assignment_to_completion() {
    let temp = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let root_path = temp.path().to_string_lossy().into_owned();
    seed_schedulable_work(&pool, "work-scheduler", &root_path).await;

    let publisher = Arc::new(RecordingPublisher::default());
    let repository =
        AssignmentRepository::with_event_sink(pool.clone(), Arc::clone(&publisher) as _);
    let work_repository = WorkRepository::new(pool.clone());
    let agent_repository = AgentRepository::new(pool.clone());
    let engine = Arc::new(FakeEngineAdapter::new(Duration::ZERO));

    let scheduler = AssignmentScheduler::new(
        repository.clone(),
        work_repository,
        agent_repository,
        engine,
        Arc::clone(&publisher) as _,
        "scheduler-test-owner",
        "Pi",
    );
    let handle = scheduler.spawn();

    let assignment_id = accept_lead(&repository, "work-scheduler").await;
    handle.wake().unwrap();

    // Poll until the assignment reaches a terminal state and the scheduler has
    // reflected the Work's terminal status.
    let mut completed = false;
    for _ in 0..200 {
        let status = assignment_status(&pool, &assignment_id).await;
        let work_status: String =
            sqlx::query_scalar("SELECT status FROM works WHERE id = 'work-scheduler'")
                .fetch_one(&pool)
                .await
                .unwrap();
        if status == "completed" && work_status == "completed" {
            completed = true;
            break;
        }
        if matches!(status.as_str(), "failed" | "dead_letter" | "cancelled") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    handle.shutdown().await;
    assert!(
        completed,
        "lead assignment should complete via the scheduler"
    );

    // The Run must carry real identity.
    let run_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM runs WHERE assignment_id = ?")
        .bind(&assignment_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(run_count, 1);

    // Every engine event must carry the agent and assignment identity.
    let missing_identity: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE run_id IS NOT NULL AND (agent_id IS NULL OR assignment_id IS NULL)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(missing_identity, 0);
}

#[tokio::test]
async fn scheduler_marks_recovery_confirmation_for_unknown_side_effects() {
    let temp = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let root_path = temp.path().to_string_lossy().into_owned();
    seed_schedulable_work(&pool, "work-recovery", &root_path).await;

    let publisher = Arc::new(RecordingPublisher::default());
    let repository =
        AssignmentRepository::with_event_sink(pool.clone(), Arc::clone(&publisher) as _);
    let work_repository = WorkRepository::new(pool.clone());
    let agent_repository = AgentRepository::new(pool.clone());
    let engine = Arc::new(FakeEngineAdapter::new(Duration::ZERO));

    // Simulate a crashed owner holding a running non-idempotent assignment.
    repository
        .accept(AcceptAssignmentInput {
            id: Some("assignment-crash".into()),
            work_id: "work-recovery".into(),
            parent_assignment_id: None,
            created_by_agent_id: None,
            assigned_agent_id: "agent-instance:piwork-lead".into(),
            capability_pack_id: None,
            kind: AssignmentKind::Lead,
            side_effect: AssignmentSideEffect::NonIdempotentWrite,
            title: "Unsafe write".into(),
            instruction: "Mutate state".into(),
            context_manifest: json!({}),
            expected_result_schema: json!({}),
            acceptance_criteria: json!([]),
            permission_scope: json!({"mode": "inherit_work"}),
            priority: 10,
            max_attempts: 3,
            not_before: None,
        })
        .await
        .unwrap();
    let now = Utc::now();
    let claimed = repository
        .claim("assignment-crash", "dead-owner", now)
        .await
        .unwrap();
    assert_eq!(claimed.status, AssignmentStatus::Claimed);
    let run = repository
        .begin_attempt("assignment-crash", "fake", "Pi")
        .await
        .unwrap();
    repository
        .mark_running("assignment-crash", &run.id, "session-1", "dead-owner", now)
        .await
        .unwrap();

    let scheduler = AssignmentScheduler::new(
        repository.clone(),
        work_repository,
        agent_repository,
        engine,
        Arc::clone(&publisher) as _,
        "scheduler-test-owner",
        "Pi",
    );
    scheduler.recover().await.unwrap();

    let status = assignment_status(&pool, "assignment-crash").await;
    assert_eq!(
        status, "recovery_confirmation_required",
        "an uncertain write from a dead owner must require explicit confirmation"
    );
}

#[tokio::test]
async fn start_lead_assignment_persists_then_schedules_to_completion() {
    let temp = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let root_path = temp.path().to_string_lossy().into_owned();
    seed_schedulable_work(&pool, "work-service", &root_path).await;

    let publisher = Arc::new(RecordingPublisher::default());
    let repository =
        AssignmentRepository::with_event_sink(pool.clone(), Arc::clone(&publisher) as _);
    let work_repository = WorkRepository::new(pool.clone());
    let agent_repository = AgentRepository::new(pool.clone());
    let engine = Arc::new(FakeEngineAdapter::new(Duration::ZERO));

    let scheduler = AssignmentScheduler::new(
        repository.clone(),
        work_repository.clone(),
        agent_repository,
        engine,
        Arc::clone(&publisher) as _,
        "scheduler-test-owner",
        "Pi",
    );
    let handle = scheduler.spawn();
    let service = AssignmentService::new(repository.clone(), work_repository, handle.clone());

    let started = service
        .start_lead_assignment("work-service", "Inspect the tree".into(), vec![], vec![])
        .await
        .unwrap();
    assert!(started.run.is_none(), "dispatch is asynchronous");
    assert_eq!(
        started.user_message.assignment_id,
        Some(started.assignment.id.clone())
    );
    assert_eq!(
        started.assignment.assigned_agent_id,
        "agent-instance:piwork-lead"
    );

    let mut completed = false;
    for _ in 0..200 {
        if assignment_status(&pool, &started.assignment.id).await == "completed" {
            completed = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    handle.shutdown().await;
    assert!(
        completed,
        "lead assignment should complete through the scheduler"
    );

    let message_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE work_id = 'work-service'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(message_count, 1);
}

#[tokio::test]
async fn cancel_stops_the_run_and_marks_the_assignment_cancelled() {
    let temp = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let root_path = temp.path().to_string_lossy().into_owned();
    seed_schedulable_work(&pool, "work-cancel", &root_path).await;

    let publisher = Arc::new(RecordingPublisher::default());
    let repository =
        AssignmentRepository::with_event_sink(pool.clone(), Arc::clone(&publisher) as _);
    let assignment_id = accept_lead(&repository, "work-cancel").await;
    let now = Utc::now();
    repository
        .claim(&assignment_id, "owner-1", now)
        .await
        .unwrap();
    let run = repository
        .begin_attempt(&assignment_id, "fake", "Pi")
        .await
        .unwrap();
    repository
        .mark_running(&assignment_id, &run.id, "session-1", "owner-1", now)
        .await
        .unwrap();

    let cancelled = repository
        .cancel(&assignment_id, &run.id, "owner-1", "user interrupt", now)
        .await
        .unwrap();
    assert_eq!(cancelled.status, AssignmentStatus::Cancelled);

    let run_status: String = sqlx::query_scalar("SELECT status FROM runs WHERE id = ?")
        .bind(&run.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(run_status, "stopped");

    let cancelled_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE assignment_id = ? AND json_extract(payload, '$.type') = 'assignmentCancelled'",
    )
    .bind(&assignment_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(cancelled_events, 1);
}
