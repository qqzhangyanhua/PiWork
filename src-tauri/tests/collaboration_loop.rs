use std::{collections::BTreeMap, sync::Arc, time::Duration};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use piwork_lib::{
    agent::repository::AgentRepository,
    assignment::{
        repository::{AcceptAssignmentInput, AssignmentEventSink, AssignmentRepository},
        scheduler::AssignmentScheduler,
    },
    collaboration::{
        result::{repair_decision, validate_result, RepairDecision, ResultSubmissionContext},
        service::LeadToolService,
    },
    domain::{
        assignment::{AssignmentKind, AssignmentSideEffect, AssignmentStatus},
        collaboration::{DelegateAssignmentInput, ResultEnvelope, ResultStatus},
        event::WorkEventEnvelope,
    },
    engine::{fake::FakeEngineAdapter, publisher::EventPublisher},
    error::AppError,
    storage::sqlite::Database,
    work::repository::WorkRepository,
};
use serde_json::json;

#[derive(Default)]
struct RecordingPublisher(std::sync::Mutex<Vec<WorkEventEnvelope>>);

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

async fn seed_work(pool: &sqlx::SqlitePool, work_id: &str, root_path: &str) {
    let now = Utc.with_ymd_and_hms(2026, 8, 16, 10, 0, 0).unwrap();
    sqlx::query(
        "INSERT INTO works (id, title, goal, root_path, permission_mode, status, created_at, updated_at) \
         VALUES (?, 'Team work', 'Goal', ?, 'balanced', 'draft', ?, ?)",
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
        "INSERT INTO work_agents (work_id, agent_instance_id, role_kind, status, permission_policy, joined_at, updated_at) \
         VALUES (?, 'agent-instance:piwork-researcher', 'researcher', 'joined', 'read_only', ?, ?)",
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
    repository
        .accept(AcceptAssignmentInput {
            id: None,
            work_id: work_id.into(),
            parent_assignment_id: None,
            created_by_agent_id: None,
            assigned_agent_id: "agent-instance:piwork-lead".into(),
            capability_pack_id: None,
            kind: AssignmentKind::Lead,
            side_effect: AssignmentSideEffect::Unknown,
            title: "Lead task".into(),
            instruction: "Coordinate the investigation".into(),
            context_manifest: json!({}),
            expected_result_schema: json!({}),
            acceptance_criteria: json!([]),
            permission_scope: json!({"mode": "inherit_work"}),
            priority: 10,
            max_attempts: 3,
            not_before: None,
        })
        .await
        .unwrap()
        .id
}

fn context() -> ResultSubmissionContext {
    ResultSubmissionContext {
        work_id: "work-1".into(),
        assignment_id: "assignment-1".into(),
        run_id: "run-1".into(),
        author_agent_id: "agent-1".into(),
    }
}

fn minimal_envelope() -> ResultEnvelope {
    ResultEnvelope {
        status: ResultStatus::Completed,
        summary: "Done".into(),
        findings: vec![],
        evidence: vec![],
        artifacts: vec![],
        validation: vec![],
        decisions_recommended: vec![],
        uncertainties: vec![],
        delegation_requests: vec![],
        memory_candidates: vec![],
        limitations: vec![],
        extensions: BTreeMap::new(),
    }
}

#[test]
fn result_validation_and_repair_escalation_are_public() {
    assert!(validate_result(&context(), minimal_envelope()).is_ok());

    let diagnostics = vec!["summary must not be empty".to_owned()];
    match repair_decision(0, diagnostics.clone()) {
        RepairDecision::RequestRepair { diagnostics: d } => assert_eq!(d, diagnostics),
        _ => panic!("first failure must request a repair"),
    }
    match repair_decision(1, diagnostics.clone()) {
        RepairDecision::Reject { diagnostics: d } => assert_eq!(d, diagnostics),
        _ => panic!("second failure must reject"),
    }
}

#[tokio::test]
async fn lead_delegates_a_single_level_member_assignment() {
    let temp = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let root_path = temp.path().to_string_lossy().into_owned();
    seed_work(&pool, "work-delegate", &root_path).await;

    let publisher = Arc::new(RecordingPublisher::default());
    let repository =
        AssignmentRepository::with_event_sink(pool.clone(), Arc::clone(&publisher) as _);
    let work_repository = WorkRepository::new(pool.clone());
    let agent_repository = AgentRepository::new(pool.clone());
    let engine = Arc::new(FakeEngineAdapter::new(Duration::ZERO));

    let scheduler = AssignmentScheduler::new(
        repository.clone(),
        work_repository,
        agent_repository.clone(),
        engine,
        Arc::clone(&publisher) as _,
        "lead-tool-owner",
        "Pi",
    );
    let handle = scheduler.spawn();
    let service = LeadToolService::new(repository.clone(), agent_repository, handle);

    let lead_id = accept_lead(&repository, "work-delegate").await;
    let result = service
        .delegate_assignment(
            &lead_id,
            DelegateAssignmentInput {
                assigned_agent_id: "agent-instance:piwork-researcher".into(),
                capability_pack_id: None,
                title: "Investigate the queue".into(),
                instruction: "Read src/assignment/queue.rs".into(),
                context_manifest: json!({}),
                expected_result_schema: json!({}),
                acceptance_criteria: json!([]),
                permission_scope: json!({"mode": "read_only"}),
                priority: 5,
                max_attempts: 2,
            },
        )
        .await
        .unwrap();

    assert_eq!(result.status, "queued");

    let child = repository
        .get_assignment(&result.assignment_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(child.kind, AssignmentKind::Member);
    assert_eq!(child.parent_assignment_id.as_deref(), Some(lead_id.as_str()));
    assert_eq!(child.assigned_agent_id, "agent-instance:piwork-researcher");

    let dependency: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM assignment_dependencies WHERE assignment_id = ? AND depends_on_assignment_id = ?",
    )
    .bind(&lead_id)
    .bind(&child.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(dependency, 1);

    let delegated: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE assignment_id = ? AND json_extract(payload, '$.type') = 'assignmentDelegated'",
    )
    .bind(&child.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(delegated, 1);
}

#[tokio::test]
async fn lead_cannot_delegate_to_itself_or_outside_the_team() {
    let temp = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let root_path = temp.path().to_string_lossy().into_owned();
    seed_work(&pool, "work-guard", &root_path).await;

    let publisher = Arc::new(RecordingPublisher::default());
    let repository =
        AssignmentRepository::with_event_sink(pool.clone(), Arc::clone(&publisher) as _);
    let work_repository = WorkRepository::new(pool.clone());
    let agent_repository = AgentRepository::new(pool.clone());
    let scheduler = AssignmentScheduler::new(
        repository.clone(),
        work_repository,
        agent_repository.clone(),
        Arc::new(FakeEngineAdapter::new(Duration::ZERO)),
        Arc::clone(&publisher) as _,
        "lead-tool-owner",
        "Pi",
    );
    let handle = scheduler.spawn();
    let service = LeadToolService::new(repository.clone(), agent_repository, handle);

    let lead_id = accept_lead(&repository, "work-guard").await;
    let delegate_to = |assigned: String| {
        let service = service.clone();
        let lead_id = lead_id.clone();
        async move {
            service
                .delegate_assignment(
                    &lead_id,
                    DelegateAssignmentInput {
                        assigned_agent_id: assigned,
                        capability_pack_id: None,
                        title: "T".into(),
                        instruction: "Do".into(),
                        context_manifest: json!({}),
                        expected_result_schema: json!({}),
                        acceptance_criteria: json!([]),
                        permission_scope: json!({}),
                        priority: 5,
                        max_attempts: 1,
                    },
                )
                .await
        }
    };

    assert!(delegate_to("agent-instance:piwork-lead".into()).await.is_err());
    assert!(delegate_to("agent-instance:piwork-reviewer".into()).await.is_err());
}

async fn seed_running_member_assignment(
    repository: &AssignmentRepository,
    pool: &sqlx::SqlitePool,
    work_id: &str,
) -> String {
    let child = repository
        .accept(AcceptAssignmentInput {
            id: None,
            work_id: work_id.into(),
            parent_assignment_id: None,
            created_by_agent_id: None,
            assigned_agent_id: "agent-instance:piwork-researcher".into(),
            capability_pack_id: None,
            kind: AssignmentKind::Member,
            side_effect: AssignmentSideEffect::ReadOnly,
            title: "Investigate".into(),
            instruction: "Inspect the queue".into(),
            context_manifest: json!({}),
            expected_result_schema: json!({}),
            acceptance_criteria: json!([]),
            permission_scope: json!({"mode": "read_only"}),
            priority: 5,
            max_attempts: 2,
            not_before: None,
        })
        .await
        .unwrap();
    let now = Utc::now();
    repository
        .claim(&child.id, "member-owner", now)
        .await
        .unwrap();
    let run = repository
        .begin_attempt(&child.id, "fake", "Pi")
        .await
        .unwrap();
    repository
        .mark_running(&child.id, &run.id, "session-1", "member-owner", now)
        .await
        .unwrap();
    let _ = pool;
    child.id
}

fn valid_result_envelope() -> ResultEnvelope {
    ResultEnvelope {
        status: ResultStatus::Completed,
        summary: "Inspected the queue".into(),
        findings: vec![],
        evidence: vec![piwork_lib::domain::collaboration::ResultEvidence {
            description: "Read the source".into(),
            source_event_id: None,
            source_resource_id: None,
            source_path: Some("src/assignment/queue.rs".into()),
            author_agent_id: "agent-instance:piwork-researcher".into(),
            assignment_id: "".into(),
            occurred_at: None,
        }],
        artifacts: vec![],
        validation: vec![],
        decisions_recommended: vec![],
        uncertainties: vec![],
        delegation_requests: vec![],
        memory_candidates: vec![],
        limitations: vec![],
        extensions: BTreeMap::new(),
    }
}

#[tokio::test]
async fn member_submits_a_valid_result_and_completes() {
    let temp = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let root_path = temp.path().to_string_lossy().into_owned();
    seed_work(&pool, "work-member", &root_path).await;

    let publisher = Arc::new(RecordingPublisher::default());
    let repository =
        AssignmentRepository::with_event_sink(pool.clone(), Arc::clone(&publisher) as _);
    let work_repository = WorkRepository::new(pool.clone());
    let agent_repository = AgentRepository::new(pool.clone());

    // Seed and run the member assignment before the scheduler starts so the two
    // do not race to claim the same queued Assignment.
    let child_id = seed_running_member_assignment(&repository, &pool, "work-member").await;

    let scheduler = AssignmentScheduler::new(
        repository.clone(),
        work_repository,
        agent_repository,
        Arc::new(FakeEngineAdapter::new(Duration::ZERO)),
        Arc::clone(&publisher) as _,
        "member-owner",
        "Pi",
    );
    let handle = scheduler.spawn();
    let service = piwork_lib::collaboration::service::MemberResultService::new(
        repository.clone(),
        handle,
    );

    let mut envelope = valid_result_envelope();
    envelope.evidence[0].assignment_id = child_id.clone();

    let outcome = service
        .submit_assignment_result(piwork_lib::collaboration::service::MemberResultSubmission {
            work_id: "work-member".into(),
            assignment_id: child_id.clone(),
            run_id: "run-1".into(),
            author_agent_id: "agent-instance:piwork-researcher".into(),
            runtime_owner: "member-owner".into(),
            envelope,
        })
        .await
        .unwrap();
    assert!(matches!(outcome, piwork_lib::collaboration::service::SubmitOutcome::Accepted { .. }));

    let child = repository.get_assignment(&child_id).await.unwrap().unwrap();
    assert_eq!(child.status, AssignmentStatus::Completed);

    let result_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM assignment_results WHERE assignment_id = ? AND status = 'valid'",
    )
    .bind(&child_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(result_rows, 1);
}

#[tokio::test]
async fn child_completion_resumes_a_waiting_lead() {
    let temp = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let root_path = temp.path().to_string_lossy().into_owned();
    seed_work(&pool, "work-resume", &root_path).await;

    let publisher = Arc::new(RecordingPublisher::default());
    let repository =
        AssignmentRepository::with_event_sink(pool.clone(), Arc::clone(&publisher) as _);

    // Lead accepts, runs, then delegates and waits.
    let lead_id = accept_lead(&repository, "work-resume").await;
    let now = Utc::now();
    repository.claim(&lead_id, "resume-owner", now).await.unwrap();
    let lead_run = repository.begin_attempt(&lead_id, "fake", "Pi").await.unwrap();
    repository
        .mark_running(&lead_id, &lead_run.id, "lead-session", "resume-owner", now)
        .await
        .unwrap();

    // Delegate directly: accept a child and record the parent→child dependency.
    let child = repository
        .accept(AcceptAssignmentInput {
            id: None,
            work_id: "work-resume".into(),
            parent_assignment_id: Some(lead_id.clone()),
            created_by_agent_id: Some("agent-instance:piwork-lead".into()),
            assigned_agent_id: "agent-instance:piwork-researcher".into(),
            capability_pack_id: None,
            kind: AssignmentKind::Member,
            side_effect: AssignmentSideEffect::ReadOnly,
            title: "Investigate".into(),
            instruction: "Inspect the queue".into(),
            context_manifest: json!({}),
            expected_result_schema: json!({}),
            acceptance_criteria: json!([]),
            permission_scope: json!({"mode": "read_only"}),
            priority: 5,
            max_attempts: 2,
            not_before: None,
        })
        .await
        .unwrap();
    repository.add_dependency(&lead_id, &child.id).await.unwrap();

    // The waiting handshake: the Lead run ends waiting on the child.
    repository
        .mark_waiting(&lead_id, &lead_run.id, "lead-session", "resume-owner", "waiting_on_assignments", now)
        .await
        .unwrap();

    // Child runs and completes (fresh timestamps: the child was created after
    // the captured `now`).
    let child_now = Utc::now();
    repository.claim(&child.id, "resume-owner", child_now).await.unwrap();
    let child_run = repository.begin_attempt(&child.id, "fake", "Pi").await.unwrap();
    repository
        .mark_running(&child.id, &child_run.id, "child-session", "resume-owner", child_now)
        .await
        .unwrap();
    repository
        .complete_by_assignment(&child.id, "Inspected", "resume-owner")
        .await
        .unwrap();

    let lead = repository.get_assignment(&lead_id).await.unwrap().unwrap();
    assert_eq!(lead.status, AssignmentStatus::Queued, "lead should resume after the child terminal");

    let resumed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE assignment_id = ? AND json_extract(payload, '$.type') = 'leadResumed'",
    )
    .bind(&lead_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(resumed, 1);
}

