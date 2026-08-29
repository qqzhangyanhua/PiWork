use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use piwork_lib::{
    agent::repository::AgentRepository,
    assignment::{
        event_outbox::AssignmentEventSink,
        repository::{AcceptAssignmentInput, AssignmentRepository},
        scheduler::AssignmentScheduler,
    },
    collaboration::{
        result::{RepairDecision, ResultSubmissionContext, repair_decision, validate_result},
        service::LeadToolService,
    },
    domain::{
        assignment::{AssignmentKind, AssignmentSideEffect, AssignmentStatus},
        collaboration::{DelegateAssignmentInput, ResultEnvelope, ResultStatus},
        event::WorkEventEnvelope,
        work::PermissionMode,
    },
    engine::{
        EngineAdapter, EngineError, EngineEvent, EngineInput, EngineRunContext, EngineSessionRef,
        fake::FakeEngineAdapter,
        harness::{AssignmentExecutionOutcome, AssignmentExecutionRequest, EngineHarness},
        publisher::EventPublisher,
    },
    error::AppError,
    storage::sqlite::Database,
    work::repository::WorkRepository,
};
use serde_json::json;
use tokio::sync::{Notify, mpsc};

#[derive(Clone, Default)]
struct WaitingLoopEngine {
    calls: Arc<AtomicUsize>,
    started_assignments: Arc<Mutex<Vec<String>>>,
    first_started: Arc<Notify>,
    release_first: Arc<Notify>,
    second_started: Arc<Notify>,
    release_second: Arc<Notify>,
}

#[derive(Clone, Default)]
struct CapturingEngine {
    prompts: Arc<Mutex<Vec<String>>>,
}

#[derive(Clone, Default)]
struct ExternallyCompletableEngine {
    started: Arc<Notify>,
    release: Arc<Notify>,
}

#[async_trait]
impl EngineAdapter for ExternallyCompletableEngine {
    fn kind(&self) -> &'static str {
        "externally-completable"
    }

    async fn start(
        &self,
        _context: EngineRunContext,
        _input: EngineInput,
        sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        let started = Arc::clone(&self.started);
        let release = Arc::clone(&self.release);
        tokio::spawn(async move {
            let _ = sink
                .send(EngineEvent::RunStarted {
                    model_label: "Externally completable".into(),
                })
                .await;
            started.notify_one();
            release.notified().await;
            let _ = sink
                .send(EngineEvent::RunCompleted {
                    summary: "engine terminal".into(),
                    artifacts: vec![],
                    validation: vec![],
                    limitations: vec![],
                })
                .await;
        });
        Ok(EngineSessionRef {
            engine_kind: self.kind().into(),
            session_id: "externally-completable-session".into(),
        })
    }
}

#[async_trait]
impl EngineAdapter for CapturingEngine {
    fn kind(&self) -> &'static str {
        "capturing"
    }

    async fn start(
        &self,
        _context: EngineRunContext,
        input: EngineInput,
        sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        self.prompts.lock().unwrap().push(input.message);
        tokio::spawn(async move {
            let _ = sink
                .send(EngineEvent::RunStarted {
                    model_label: "Capturing".into(),
                })
                .await;
            let _ = sink
                .send(EngineEvent::RunCompleted {
                    summary: "completed".into(),
                    artifacts: vec![],
                    validation: vec![],
                    limitations: vec![],
                })
                .await;
        });
        Ok(EngineSessionRef {
            engine_kind: self.kind().into(),
            session_id: "capturing-session".into(),
        })
    }
}

#[async_trait]
impl EngineAdapter for WaitingLoopEngine {
    fn kind(&self) -> &'static str {
        "waiting-loop"
    }

    async fn start(
        &self,
        context: EngineRunContext,
        _input: EngineInput,
        sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let assignment_id = context.assignment_id().to_owned();
        self.started_assignments.lock().unwrap().push(assignment_id);
        let first_started = Arc::clone(&self.first_started);
        let release_first = Arc::clone(&self.release_first);
        let second_started = Arc::clone(&self.second_started);
        let release_second = Arc::clone(&self.release_second);
        tokio::spawn(async move {
            let _ = sink
                .send(EngineEvent::RunStarted {
                    model_label: "Waiting loop".into(),
                })
                .await;
            if call == 0 {
                first_started.notify_one();
                release_first.notified().await;
                let _ = sink
                    .send(EngineEvent::Waiting {
                        reason: "waiting_on_assignments".into(),
                    })
                    .await;
            } else {
                if call == 1 {
                    second_started.notify_one();
                    release_second.notified().await;
                }
                let _ = sink
                    .send(EngineEvent::RunCompleted {
                        summary: "completed".into(),
                        artifacts: vec![],
                        validation: vec![],
                        limitations: vec![],
                    })
                    .await;
            }
        });
        Ok(EngineSessionRef {
            engine_kind: self.kind().into(),
            session_id: format!("waiting-loop-session-{call}"),
        })
    }
}

async fn wait_for_assignment_status(
    repository: &AssignmentRepository,
    assignment_id: &str,
    expected: AssignmentStatus,
) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let actual = repository
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

async fn add_alternate_research_pack(pool: &sqlx::SqlitePool) {
    sqlx::query(
        "INSERT INTO capability_packs (
             id, catalog_capability_id, name, description, instructions,
             input_schema_json, output_schema_json, procedure_json, validation_rubric_json,
             required_tools_json, default_permission_scope, compatible_role_template_ids_json,
             required_engine_capabilities_json, conflicts_with_capability_pack_ids_json,
             version, status, created_at, updated_at
         )
         SELECT 'capability-pack:alternate-research:v1', NULL, 'Alternate research',
                'An alternate research method.', 'ALTERNATE_RESEARCH_INSTRUCTIONS',
                input_schema_json, output_schema_json, procedure_json, validation_rubric_json,
                required_tools_json, default_permission_scope, compatible_role_template_ids_json,
                required_engine_capabilities_json, conflicts_with_capability_pack_ids_json,
                version, status, created_at, updated_at
         FROM capability_packs WHERE id = 'capability-pack:source-research:v1'",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO agent_capability_bindings (
             agent_definition_id, capability_pack_id, installed_at
         ) VALUES (
             'agent-definition:piwork-researcher:v1',
             'capability-pack:alternate-research:v1',
             '2026-08-16T10:00:00Z'
         )",
    )
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
        work_repository.clone(),
        agent_repository.clone(),
        engine,
        Arc::clone(&publisher) as _,
        "lead-tool-owner",
        "Pi",
    );
    let handle = scheduler.spawn();
    let service = LeadToolService::new(
        repository.clone(),
        work_repository,
        agent_repository,
        handle,
    );

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
    assert_eq!(
        child.parent_assignment_id.as_deref(),
        Some(lead_id.as_str())
    );
    assert_eq!(child.assigned_agent_id, "agent-instance:piwork-researcher");
    assert_eq!(
        child.capability_pack_id.as_deref(),
        Some("capability-pack:source-research:v1"),
        "a single bound expert capability should be selected automatically"
    );

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
async fn delegation_requires_an_explicit_pack_when_the_agent_has_multiple() {
    let temp = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let root_path = temp.path().to_string_lossy().into_owned();
    seed_work(&pool, "work-pack-selection", &root_path).await;
    add_alternate_research_pack(&pool).await;

    let publisher = Arc::new(RecordingPublisher::default());
    let repository =
        AssignmentRepository::with_event_sink(pool.clone(), Arc::clone(&publisher) as _);
    let scheduler = AssignmentScheduler::new(
        repository.clone(),
        WorkRepository::new(pool.clone()),
        AgentRepository::new(pool.clone()),
        Arc::new(FakeEngineAdapter::new(Duration::ZERO)),
        Arc::clone(&publisher) as _,
        "pack-selection-owner",
        "Pi",
    );
    let handle = scheduler.spawn();
    let service = LeadToolService::new(
        repository.clone(),
        WorkRepository::new(pool.clone()),
        AgentRepository::new(pool),
        handle.clone(),
    );
    let lead_id = accept_lead(&repository, "work-pack-selection").await;

    let error = service
        .delegate_assignment(
            &lead_id,
            DelegateAssignmentInput {
                assigned_agent_id: "agent-instance:piwork-researcher".into(),
                capability_pack_id: None,
                title: "Investigate with a selected method".into(),
                instruction: "Use the explicitly selected expert method".into(),
                context_manifest: json!({}),
                expected_result_schema: json!({}),
                acceptance_criteria: json!([]),
                permission_scope: json!({"mode": "read_only"}),
                priority: 5,
                max_attempts: 1,
            },
        )
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        AppError::InvalidInput { ref field, ref message }
            if field == "capabilityPackId" && message.contains("multiple capability packs")
    ));
    handle.shutdown().await;
}

#[tokio::test]
async fn scheduler_injects_only_the_assignment_selected_capability_pack() {
    let temp = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let root_path = temp.path().to_string_lossy().into_owned();
    seed_work(&pool, "work-selected-pack", &root_path).await;
    add_alternate_research_pack(&pool).await;

    let publisher = Arc::new(RecordingPublisher::default());
    let repository =
        AssignmentRepository::with_event_sink(pool.clone(), Arc::clone(&publisher) as _);
    let assignment = repository
        .accept(AcceptAssignmentInput {
            id: None,
            work_id: "work-selected-pack".into(),
            parent_assignment_id: None,
            created_by_agent_id: Some("agent-instance:piwork-lead".into()),
            assigned_agent_id: "agent-instance:piwork-researcher".into(),
            capability_pack_id: Some("capability-pack:source-research:v1".into()),
            kind: AssignmentKind::Member,
            side_effect: AssignmentSideEffect::ReadOnly,
            title: "Use source research".into(),
            instruction: "Run the selected research method".into(),
            context_manifest: json!({}),
            expected_result_schema: json!({}),
            acceptance_criteria: json!([]),
            permission_scope: json!({"mode": "read_only"}),
            priority: 5,
            max_attempts: 1,
            not_before: None,
        })
        .await
        .unwrap();
    let engine = Arc::new(CapturingEngine::default());
    let scheduler = AssignmentScheduler::new(
        repository.clone(),
        WorkRepository::new(pool.clone()),
        AgentRepository::new(pool),
        engine.clone(),
        Arc::clone(&publisher) as _,
        "selected-pack-owner",
        "Pi",
    );
    let handle = scheduler.spawn();
    handle.wake().unwrap();
    wait_for_assignment_status(&repository, &assignment.id, AssignmentStatus::DeadLetter).await;

    {
        let prompts = engine.prompts.lock().unwrap();
        assert_eq!(prompts.len(), 1);
        assert!(prompts[0].contains("优先使用原始可信来源"));
        assert!(!prompts[0].contains("ALTERNATE_RESEARCH_INSTRUCTIONS"));
    }
    handle.shutdown().await;
}

#[tokio::test]
async fn lead_resume_waiting_releases_the_work_slot_and_runs_the_child() {
    let temp = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let root_path = temp.path().to_string_lossy().into_owned();
    seed_work(&pool, "work-waiting-loop", &root_path).await;

    let publisher = Arc::new(RecordingPublisher::default());
    let repository =
        AssignmentRepository::with_event_sink(pool.clone(), Arc::clone(&publisher) as _);
    let work_repository = WorkRepository::new(pool.clone());
    let agent_repository = AgentRepository::new(pool.clone());
    let engine = Arc::new(WaitingLoopEngine::default());
    let scheduler = AssignmentScheduler::new(
        repository.clone(),
        work_repository,
        agent_repository.clone(),
        engine.clone(),
        Arc::clone(&publisher) as _,
        "waiting-loop-owner",
        "Pi",
    );
    let handle = scheduler.spawn();
    let service = LeadToolService::new(
        repository.clone(),
        WorkRepository::new(database.pool().clone()),
        agent_repository,
        handle.clone(),
    );
    let member_service = piwork_lib::collaboration::service::MemberResultService::new(
        repository.clone(),
        handle.clone(),
    );

    let lead_id = accept_lead(&repository, "work-waiting-loop").await;
    handle.wake().unwrap();
    engine.first_started.notified().await;
    wait_for_assignment_status(&repository, &lead_id, AssignmentStatus::Running).await;

    let delegated = service
        .delegate_assignment(
            &lead_id,
            DelegateAssignmentInput {
                assigned_agent_id: "agent-instance:piwork-researcher".into(),
                capability_pack_id: None,
                title: "Inspect independently".into(),
                instruction: "Inspect the relevant implementation".into(),
                context_manifest: json!({}),
                expected_result_schema: json!({}),
                acceptance_criteria: json!([]),
                permission_scope: json!({"mode": "read_only"}),
                priority: 5,
                max_attempts: 1,
            },
        )
        .await
        .unwrap();

    engine.release_first.notify_one();
    engine.second_started.notified().await;
    wait_for_assignment_status(&repository, &lead_id, AssignmentStatus::Waiting).await;
    wait_for_assignment_status(
        &repository,
        &delegated.assignment_id,
        AssignmentStatus::Running,
    )
    .await;
    assert_eq!(
        engine.started_assignments.lock().unwrap().as_slice(),
        [lead_id.as_str(), delegated.assignment_id.as_str()],
        "the child must acquire the same-Work slot after the Lead yields"
    );

    let child_run_id: String = sqlx::query_scalar(
        "SELECT id FROM runs WHERE assignment_id = ? AND status = 'running' ORDER BY attempt_number DESC LIMIT 1",
    )
    .bind(&delegated.assignment_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let mut envelope = valid_result_envelope();
    envelope.evidence[0].assignment_id = delegated.assignment_id.clone();
    member_service
        .submit_assignment_result(piwork_lib::collaboration::service::MemberResultSubmission {
            work_id: "work-waiting-loop".into(),
            assignment_id: delegated.assignment_id.clone(),
            run_id: child_run_id,
            author_agent_id: "agent-instance:piwork-researcher".into(),
            runtime_owner: "waiting-loop-owner".into(),
            envelope,
        })
        .await
        .unwrap();
    engine.release_second.notify_one();
    wait_for_assignment_status(
        &repository,
        &delegated.assignment_id,
        AssignmentStatus::Completed,
    )
    .await;
    handle.wake().unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    while engine.started_assignments.lock().unwrap().len() < 3 {
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    wait_for_assignment_status(&repository, &lead_id, AssignmentStatus::Waiting).await;
    assert_eq!(
        engine.started_assignments.lock().unwrap().as_slice(),
        [
            lead_id.as_str(),
            delegated.assignment_id.as_str(),
            lead_id.as_str(),
        ],
        "the Lead must start a fresh Run after the child becomes terminal"
    );

    handle.shutdown().await;
}

#[tokio::test]
async fn lead_resume_context_includes_validated_dependency_results() {
    let temp = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let root_path = temp.path().to_string_lossy().into_owned();
    seed_work(&pool, "work-result-context", &root_path).await;

    let publisher = Arc::new(RecordingPublisher::default());
    let repository =
        AssignmentRepository::with_event_sink(pool.clone(), Arc::clone(&publisher) as _);
    let lead_id = accept_lead(&repository, "work-result-context").await;
    let child = repository
        .accept(AcceptAssignmentInput {
            id: None,
            work_id: "work-result-context".into(),
            parent_assignment_id: Some(lead_id.clone()),
            created_by_agent_id: Some("agent-instance:piwork-lead".into()),
            assigned_agent_id: "agent-instance:piwork-researcher".into(),
            capability_pack_id: None,
            kind: AssignmentKind::Member,
            side_effect: AssignmentSideEffect::ReadOnly,
            title: "Inspect dependency".into(),
            instruction: "Produce an evidence-backed result".into(),
            context_manifest: json!({}),
            expected_result_schema: json!({}),
            acceptance_criteria: json!([]),
            permission_scope: json!({"mode": "read_only"}),
            priority: 5,
            max_attempts: 1,
            not_before: None,
        })
        .await
        .unwrap();
    repository
        .add_dependency(&lead_id, &child.id)
        .await
        .unwrap();

    let now = Utc::now();
    repository
        .claim(&child.id, "result-context-owner", now)
        .await
        .unwrap();
    let child_run = repository
        .begin_attempt(&child.id, "capturing", "Pi")
        .await
        .unwrap();
    repository
        .mark_running(
            &child.id,
            &child_run.id,
            "result-context-session",
            "result-context-owner",
            now,
        )
        .await
        .unwrap();
    let mut envelope = minimal_envelope();
    envelope.summary = "The dependency found the scheduler invariant".into();
    repository
        .record_result(
            &child.id,
            "agent-instance:piwork-researcher",
            &serde_json::to_string(&envelope).unwrap(),
            "valid",
            0,
        )
        .await
        .unwrap();
    repository
        .complete_by_assignment(&child.id, &envelope.summary, "result-context-owner")
        .await
        .unwrap();

    let engine = Arc::new(CapturingEngine::default());
    let scheduler = AssignmentScheduler::new(
        repository.clone(),
        WorkRepository::new(pool.clone()),
        AgentRepository::new(pool),
        engine.clone(),
        Arc::clone(&publisher) as _,
        "result-context-owner",
        "Pi",
    );
    let handle = scheduler.spawn();
    handle.wake().unwrap();
    wait_for_assignment_status(&repository, &lead_id, AssignmentStatus::Waiting).await;

    {
        let prompts = engine.prompts.lock().unwrap();
        assert_eq!(prompts.len(), 1);
        assert!(
            prompts[0].contains("The dependency found the scheduler invariant"),
            "the resumed Lead must receive validated dependency Result Envelopes"
        );
    }
    handle.shutdown().await;
}

#[tokio::test]
async fn harness_accepts_completion_already_committed_by_a_host_tool() {
    let temp = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let root_path = temp.path().to_string_lossy().into_owned();
    seed_work(&pool, "work-host-completion", &root_path).await;

    let publisher = Arc::new(RecordingPublisher::default());
    let repository =
        AssignmentRepository::with_event_sink(pool.clone(), Arc::clone(&publisher) as _);
    let work_repository = WorkRepository::new(pool.clone());
    let agent_repository = AgentRepository::new(pool);
    let assignment_id = accept_lead(&repository, "work-host-completion").await;
    let assignment = repository
        .claim(&assignment_id, "host-completion-owner", Utc::now())
        .await
        .unwrap();
    let run = repository
        .begin_attempt(&assignment.id, "externally-completable", "Pi")
        .await
        .unwrap();
    let run_id = run.id.clone();
    let work = work_repository
        .get(&assignment.work_id)
        .await
        .unwrap()
        .unwrap()
        .summary;
    let agent = agent_repository
        .get_agent_instance(&assignment.assigned_agent_id)
        .await
        .unwrap()
        .unwrap();
    let engine = Arc::new(ExternallyCompletableEngine::default());
    let harness = Arc::new(EngineHarness::new(
        engine.clone(),
        work_repository,
        repository.clone(),
        Arc::clone(&publisher) as _,
    ));
    let execution_harness = Arc::clone(&harness);
    let execution_assignment = assignment.clone();
    let execution = tokio::spawn(async move {
        execution_harness
            .execute(AssignmentExecutionRequest {
                assignment: execution_assignment,
                work,
                agent,
                run,
                input: EngineInput {
                    message: "Complete through a host tool".into(),
                    images: vec![],
                    documents: vec![],
                },
                effective_permission: PermissionMode::Balanced,
                runtime_owner: "host-completion-owner".into(),
                extension_tool_ids: Vec::new(),
            })
            .await
    });

    engine.started.notified().await;
    wait_for_assignment_status(&repository, &assignment_id, AssignmentStatus::Running).await;
    let event_deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        let started: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM events WHERE run_id = ? AND json_extract(payload, '$.type') = 'runStarted'",
        )
        .bind(&run_id)
        .fetch_one(database.pool())
        .await
        .unwrap();
        if started == 1 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < event_deadline,
            "RunStarted was not journaled before the host tool call"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    repository
        .complete_by_assignment(
            &assignment_id,
            "committed by complete_work_delivery",
            "host-completion-owner",
        )
        .await
        .unwrap();
    engine.release.notify_one();

    let outcome = execution.await.unwrap().unwrap();
    assert!(matches!(
        outcome,
        AssignmentExecutionOutcome::Completed { result_summary, .. }
            if result_summary == "committed by complete_work_delivery"
    ));
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
        work_repository.clone(),
        agent_repository.clone(),
        Arc::new(FakeEngineAdapter::new(Duration::ZERO)),
        Arc::clone(&publisher) as _,
        "lead-tool-owner",
        "Pi",
    );
    let handle = scheduler.spawn();
    let service = LeadToolService::new(
        repository.clone(),
        work_repository,
        agent_repository,
        handle,
    );

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

    assert!(
        delegate_to("agent-instance:piwork-lead".into())
            .await
            .is_err()
    );
    assert!(
        delegate_to("agent-instance:piwork-reviewer".into())
            .await
            .is_err()
    );
}

async fn seed_running_member_assignment(
    repository: &AssignmentRepository,
    pool: &sqlx::SqlitePool,
    work_id: &str,
) -> (String, String) {
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
    (child.id, run.id)
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
    let (child_id, run_id) =
        seed_running_member_assignment(&repository, &pool, "work-member").await;

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
    let service =
        piwork_lib::collaboration::service::MemberResultService::new(repository.clone(), handle);

    let mut envelope = valid_result_envelope();
    envelope.evidence[0].assignment_id = child_id.clone();

    let outcome = service
        .submit_assignment_result(piwork_lib::collaboration::service::MemberResultSubmission {
            work_id: "work-member".into(),
            assignment_id: child_id.clone(),
            run_id,
            author_agent_id: "agent-instance:piwork-researcher".into(),
            runtime_owner: "member-owner".into(),
            envelope,
        })
        .await
        .unwrap();
    assert!(matches!(
        outcome,
        piwork_lib::collaboration::service::SubmitOutcome::Accepted { .. }
    ));

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
    repository
        .claim(&lead_id, "resume-owner", now)
        .await
        .unwrap();
    let lead_run = repository
        .begin_attempt(&lead_id, "fake", "Pi")
        .await
        .unwrap();
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
    repository
        .add_dependency(&lead_id, &child.id)
        .await
        .unwrap();

    // The waiting handshake: the Lead run ends waiting on the child.
    repository
        .mark_waiting(
            &lead_id,
            &lead_run.id,
            "lead-session",
            "resume-owner",
            "waiting_on_assignments",
            now,
        )
        .await
        .unwrap();

    // Child runs and completes (fresh timestamps: the child was created after
    // the captured `now`).
    let child_now = Utc::now();
    repository
        .claim(&child.id, "resume-owner", child_now)
        .await
        .unwrap();
    let child_run = repository
        .begin_attempt(&child.id, "fake", "Pi")
        .await
        .unwrap();
    repository
        .mark_running(
            &child.id,
            &child_run.id,
            "child-session",
            "resume-owner",
            child_now,
        )
        .await
        .unwrap();
    repository
        .complete_by_assignment(&child.id, "Inspected", "resume-owner")
        .await
        .unwrap();

    let lead = repository.get_assignment(&lead_id).await.unwrap().unwrap();
    assert_eq!(
        lead.status,
        AssignmentStatus::Queued,
        "lead should resume after the child terminal"
    );

    let resumed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE assignment_id = ? AND json_extract(payload, '$.type') = 'leadResumed'",
    )
    .bind(&lead_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(resumed, 1);
}

#[tokio::test]
async fn memory_candidates_require_confirmation_and_reject_secrets() {
    let temp = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let root_path = temp.path().to_string_lossy().into_owned();
    seed_work(&pool, "work-memory", &root_path).await;

    let service = piwork_lib::collaboration::memory::MemoryService::new(pool.clone());
    let ids = service
        .propose_candidates(
            "work-memory",
            "agent-instance:piwork-researcher",
            "assignment-memory-source",
            vec![
                piwork_lib::domain::collaboration::MemoryCandidateInput {
                    content: "The queue uses a BTreeMap for fair ordering".into(),
                    reason: "discovered during review".into(),
                },
                piwork_lib::domain::collaboration::MemoryCandidateInput {
                    content: "the API key is sk-secret-123".into(),
                    reason: "stored credential".into(),
                },
            ],
        )
        .await
        .unwrap();
    // The secret candidate is never proposed.
    assert_eq!(ids.len(), 1);

    let candidates = service.list_work_candidates("work-memory").await.unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0].status,
        piwork_lib::domain::collaboration::MemoryCandidateStatus::Proposed
    );

    let candidate_id = candidates[0].id.clone();
    let (resolved, source_assignment_id) = service
        .resolve_candidate(&candidate_id, true, "agent-instance:piwork-lead")
        .await
        .unwrap();
    assert_eq!(
        resolved.status,
        piwork_lib::domain::collaboration::MemoryCandidateStatus::Confirmed
    );
    assert_eq!(
        source_assignment_id.as_deref(),
        Some("assignment-memory-source")
    );

    let memory = service
        .list_agent_memory("agent-instance:piwork-researcher", 1024)
        .await
        .unwrap();
    assert_eq!(
        memory,
        vec!["The queue uses a BTreeMap for fair ordering".to_owned()]
    );
}

#[tokio::test]
async fn lead_records_decisions_plan_and_delivery() {
    let temp = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let root_path = temp.path().to_string_lossy().into_owned();
    seed_work(&pool, "work-delivery", &root_path).await;

    let publisher = Arc::new(RecordingPublisher::default());
    let repository =
        AssignmentRepository::with_event_sink(pool.clone(), Arc::clone(&publisher) as _);
    let work_repository = WorkRepository::new(pool.clone());
    let agent_repository = AgentRepository::new(pool.clone());

    // Seed and run the lead before the scheduler starts so they do not race.
    let lead_id = accept_lead(&repository, "work-delivery").await;
    let now = Utc::now();
    repository
        .claim(&lead_id, "delivery-owner", now)
        .await
        .unwrap();
    let run = repository
        .begin_attempt(&lead_id, "fake", "Pi")
        .await
        .unwrap();
    repository
        .mark_running(&lead_id, &run.id, "lead-session", "delivery-owner", now)
        .await
        .unwrap();
    // Advance the Work so delivery can complete it.
    let _ = work_repository
        .set_work_status(
            "work-delivery",
            piwork_lib::domain::work::WorkStatus::Queued,
        )
        .await;
    let _ = work_repository
        .set_work_status(
            "work-delivery",
            piwork_lib::domain::work::WorkStatus::Running,
        )
        .await;

    let scheduler = AssignmentScheduler::new(
        repository.clone(),
        work_repository.clone(),
        agent_repository.clone(),
        Arc::new(FakeEngineAdapter::new(Duration::ZERO)),
        Arc::clone(&publisher) as _,
        "delivery-owner",
        "Pi",
    );
    let handle = scheduler.spawn();
    let service = LeadToolService::new(
        repository.clone(),
        work_repository,
        agent_repository,
        handle,
    );

    service
        .record_work_decision(
            &lead_id,
            piwork_lib::domain::collaboration::RecordWorkDecisionInput {
                summary: "Adopt the queue".into(),
            },
        )
        .await
        .unwrap();
    service
        .update_work_plan(
            &lead_id,
            piwork_lib::domain::collaboration::UpdateWorkPlanInput {
                plan: vec![piwork_lib::domain::collaboration::LedgerPlanStep {
                    id: "step-1".into(),
                    title: "Investigate".into(),
                    status: piwork_lib::domain::collaboration::LedgerPlanStepStatus::InProgress,
                }],
            },
        )
        .await
        .unwrap();
    std::fs::write(temp.path().join("notes.md"), "Delivered notes").unwrap();
    let completed = service
        .complete_work_delivery(
            &lead_id,
            "delivery-owner",
            piwork_lib::domain::collaboration::CompleteWorkDeliveryInput {
                summary: "Delivered".into(),
                artifacts: vec!["notes.md".into()],
                validation: vec!["cargo test".into()],
                limitations: vec![],
            },
        )
        .await
        .unwrap();
    assert_eq!(completed.status, AssignmentStatus::Completed);

    let work_status: String =
        sqlx::query_scalar("SELECT status FROM works WHERE id = 'work-delivery'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(work_status, "completed");

    for event_type in [
        "workDecisionRecorded",
        "workPlanUpdated",
        "workDeliveryCompleted",
    ] {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM events WHERE work_id = 'work-delivery' AND json_extract(payload, '$.type') = ?",
        )
        .bind(event_type)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1, "missing {event_type}");
    }
}
