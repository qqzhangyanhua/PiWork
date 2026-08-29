//! Characterization: recovery confirmation and Host Tool lease safety (#13).
//!
//! Seams under test:
//! - `AssignmentScheduler::recover` + `FakeEngineAdapter` observations
//! - `AssignmentRepository::recover_orphans` / `confirm_recovery` + SQLite reopen
//! - `EngineHarness` lease issuance/revocation with `HostToolRegistry`
//! - `HostToolServer` loopback auth failures (stable 401, no product dispatch)

use std::{
    io::{Read, Write},
    sync::{Arc, Mutex, OnceLock},
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
    capability::CapabilityBroker,
    collaboration::{
        tool_bridge::{AuthorizedRunContext, HostToolRegistry},
        tool_server::{HostToolServer, ToolDispatch},
        tools::TOOL_GET_ASSIGNMENT_STATUS,
    },
    domain::{
        assignment::{AssignmentKind, AssignmentSideEffect, AssignmentStatus},
        event::WorkEventEnvelope,
        work::PermissionMode,
    },
    engine::{
        EngineAdapter, EngineError, EngineEvent, EngineInput, EngineRunContext, EngineSessionRef,
        fake::FakeEngineAdapter,
        harness::{AssignmentExecutionRequest, EngineHarness, HostToolBridgeConfig},
        publisher::EventPublisher,
    },
    error::AppError,
    storage::sqlite::Database,
    work::repository::WorkRepository,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

#[derive(Default)]
struct RecordingPublisher;

impl AssignmentEventSink for RecordingPublisher {
    fn publish(&self, _event: WorkEventEnvelope) -> Result<(), AppError> {
        Ok(())
    }
}

#[async_trait]
impl EventPublisher for RecordingPublisher {
    async fn publish(&self, _event: WorkEventEnvelope) -> Result<(), AppError> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct LeaseCapturingEngine {
    captured: Arc<Mutex<Option<(String, String)>>>,
}

#[async_trait]
impl EngineAdapter for LeaseCapturingEngine {
    fn kind(&self) -> &'static str {
        "lease-capturing"
    }

    async fn start(
        &self,
        context: EngineRunContext,
        _input: EngineInput,
        sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        let lease = context
            .host_tool_lease()
            .ok_or_else(|| EngineError::Start("expected a host tool lease for this Run".into()))?;
        *self.captured.lock().unwrap() = Some((context.run_id().to_owned(), lease.token.to_hex()));
        tokio::spawn(async move {
            let _ = sink
                .send(EngineEvent::RunStarted {
                    model_label: "Lease capturing".into(),
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
            session_id: "lease-capturing-session".into(),
        })
    }
}

async fn seed_work(pool: &sqlx::SqlitePool, work_id: &str, root_path: &str) {
    let now = Utc.with_ymd_and_hms(2026, 8, 29, 10, 0, 0).unwrap();
    sqlx::query(
        "INSERT INTO works (id, title, goal, root_path, permission_mode, status, created_at, updated_at) \
         VALUES (?, 'Recovery lease', 'Do not auto-replay', ?, 'balanced', 'draft', ?, ?)",
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

async fn accept_risky_running_orphan(
    repository: &AssignmentRepository,
    work_id: &str,
    assignment_id: &str,
    side_effect: AssignmentSideEffect,
) {
    repository
        .accept(AcceptAssignmentInput {
            id: Some(assignment_id.into()),
            work_id: work_id.into(),
            parent_assignment_id: None,
            created_by_agent_id: None,
            assigned_agent_id: "agent-instance:piwork-lead".into(),
            capability_pack_id: None,
            kind: AssignmentKind::Lead,
            side_effect,
            title: "Unsafe orphan".into(),
            instruction: "Must not auto-replay".into(),
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
    repository
        .claim(assignment_id, "dead-owner", now)
        .await
        .unwrap();
    let run = repository
        .begin_attempt(assignment_id, "fake", "Pi")
        .await
        .unwrap();
    repository
        .mark_running(assignment_id, &run.id, "session-orphan", "dead-owner", now)
        .await
        .unwrap();
}

fn decode_hex_token(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .filter_map(|index| u8::from_str_radix(hex.get(index..index + 2)?, 16).ok())
        .collect()
}

fn post_host_tool(
    endpoint: &str,
    run_id: &str,
    token_hex: &str,
    tool: &str,
) -> (u16, String, String) {
    let body = json!({
        "runId": run_id,
        "token": token_hex,
        "tool": tool,
        "arguments": {}
    });
    let body = body.to_string();
    let request = format!(
        "POST /tool HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let addr = endpoint
        .strip_prefix("http://")
        .and_then(|value| value.strip_suffix("/tool"))
        .expect("host tool endpoint shape");
    let mut stream = std::net::TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let status_line = response.lines().next().unwrap_or("").to_owned();
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let body = response
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or("")
        .trim()
        .to_owned();
    (status, status_line, body)
}

#[tokio::test]
async fn unknown_side_effect_orphan_is_not_auto_replayed_after_scheduler_recover() {
    let temp = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let root_path = temp.path().to_string_lossy().into_owned();
    seed_work(&pool, "work-no-replay", &root_path).await;

    let publisher = Arc::new(RecordingPublisher);
    let repository =
        AssignmentRepository::with_event_sink(pool.clone(), Arc::clone(&publisher) as _);
    let engine = Arc::new(FakeEngineAdapter::new(Duration::ZERO));
    let scheduler = AssignmentScheduler::new(
        repository.clone(),
        WorkRepository::new(pool.clone()),
        AgentRepository::new(pool.clone()),
        Arc::clone(&engine) as _,
        Arc::clone(&publisher) as _,
        "scheduler-recovery-owner",
        "Pi",
    );

    accept_risky_running_orphan(
        &repository,
        "work-no-replay",
        "assignment-unknown-orphan",
        AssignmentSideEffect::Unknown,
    )
    .await;

    scheduler.recover().await.unwrap();
    let stored = repository.list_for_work("work-no-replay").await.unwrap();
    assert_eq!(
        stored[0].status,
        AssignmentStatus::RecoveryConfirmationRequired,
        "unknown started side effects must require explicit confirmation"
    );
    assert!(
        stored[0].recovery_reason.is_some(),
        "recovery confirmation must persist a reason for the operator"
    );

    let handle = scheduler.spawn();
    handle.wake().unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    handle.shutdown().await;

    let observations = engine.observations().await;
    assert!(
        observations.started_run_ids.is_empty(),
        "recovery confirmation must not auto-start the engine; started={:?}",
        observations.started_run_ids
    );
    let still = repository.list_for_work("work-no-replay").await.unwrap();
    assert_eq!(
        still[0].status,
        AssignmentStatus::RecoveryConfirmationRequired
    );
}

#[tokio::test]
async fn recovery_confirmation_survives_reopen_and_still_blocks_auto_replay() {
    let temporary = tempfile::tempdir().unwrap();
    let db_path = temporary.path().join("recovery-confirm.db");
    let root_path = temporary.path().join("workspace");
    std::fs::create_dir_all(&root_path).unwrap();
    let root_path = root_path.to_string_lossy().into_owned();
    let assignment_id = "assignment-persist-confirm";

    {
        let database = Database::open(&db_path).await.unwrap();
        let pool = database.pool().clone();
        seed_work(&pool, "work-persist-confirm", &root_path).await;
        let repository = AssignmentRepository::new(pool);
        accept_risky_running_orphan(
            &repository,
            "work-persist-confirm",
            assignment_id,
            AssignmentSideEffect::Unknown,
        )
        .await;
        let report = repository.recover_orphans(&[]).await.unwrap();
        assert_eq!(report.confirmation_required, vec![assignment_id]);
        let stored = &repository
            .list_for_work("work-persist-confirm")
            .await
            .unwrap()[0];
        assert_eq!(
            stored.status,
            AssignmentStatus::RecoveryConfirmationRequired
        );
        assert_eq!(
            stored.recovery_reason.as_deref(),
            Some("runtime owner disappeared during the active attempt")
        );
    }

    {
        let database = Database::open(&db_path).await.unwrap();
        let pool = database.pool().clone();
        let publisher = Arc::new(RecordingPublisher);
        let repository =
            AssignmentRepository::with_event_sink(pool.clone(), Arc::clone(&publisher) as _);
        let stored = &repository
            .list_for_work("work-persist-confirm")
            .await
            .unwrap()[0];
        assert_eq!(
            stored.status,
            AssignmentStatus::RecoveryConfirmationRequired,
            "confirmation-required must reload from durable storage"
        );
        assert_eq!(
            stored.recovery_reason.as_deref(),
            Some("runtime owner disappeared during the active attempt")
        );

        let engine = Arc::new(FakeEngineAdapter::new(Duration::ZERO));
        let scheduler = AssignmentScheduler::new(
            repository.clone(),
            WorkRepository::new(pool.clone()),
            AgentRepository::new(pool),
            Arc::clone(&engine) as _,
            Arc::clone(&publisher) as _,
            "scheduler-reopen-owner",
            "Pi",
        );
        scheduler.recover().await.unwrap();
        let handle = scheduler.spawn();
        handle.wake().unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        handle.shutdown().await;

        assert!(
            engine.observations().await.started_run_ids.is_empty(),
            "reopened recovery confirmation must still refuse auto-replay"
        );
        assert_eq!(
            repository
                .list_for_work("work-persist-confirm")
                .await
                .unwrap()[0]
                .status,
            AssignmentStatus::RecoveryConfirmationRequired
        );
    }
}

#[tokio::test]
async fn harness_revokes_host_tool_lease_when_run_reaches_terminal() {
    let temp = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let root_path = temp.path().to_string_lossy().into_owned();
    seed_work(&pool, "work-lease-revoke", &root_path).await;

    let publisher = Arc::new(RecordingPublisher);
    let repository =
        AssignmentRepository::with_event_sink(pool.clone(), Arc::clone(&publisher) as _);
    let work_repository = WorkRepository::new(pool.clone());
    let agent_repository = AgentRepository::new(pool.clone());

    let assignment = repository
        .accept(AcceptAssignmentInput {
            id: Some("assignment-lease-revoke".into()),
            work_id: "work-lease-revoke".into(),
            parent_assignment_id: None,
            created_by_agent_id: None,
            assigned_agent_id: "agent-instance:piwork-lead".into(),
            capability_pack_id: None,
            kind: AssignmentKind::Lead,
            side_effect: AssignmentSideEffect::ReadOnly,
            title: "Lease revoke".into(),
            instruction: "Capture then die".into(),
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
    let assignment = repository
        .claim(&assignment.id, "lease-owner", now)
        .await
        .unwrap();
    let run = repository
        .begin_attempt(&assignment.id, "lease-capturing", "Pi")
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

    let registry = Arc::new(HostToolRegistry::new());
    let endpoint = Arc::new(OnceLock::new());
    let dispatch_count = Arc::new(Mutex::new(0_u32));
    let dispatch_seen = Arc::clone(&dispatch_count);
    let dispatch: Arc<ToolDispatch> = Arc::new(move |_tool, _context, _args| {
        *dispatch_seen.lock().unwrap() += 1;
        Ok(json!({ "ok": true }))
    });
    let server = HostToolServer::bind(Arc::clone(&registry), dispatch).unwrap();
    endpoint.set(server.endpoint().to_owned()).unwrap();

    let engine = Arc::new(LeaseCapturingEngine::default());
    let harness = EngineHarness::new(
        Arc::clone(&engine) as _,
        work_repository,
        repository,
        Arc::clone(&publisher) as _,
    )
    .with_capability_broker(CapabilityBroker::new(pool))
    .with_host_tools(HostToolBridgeConfig {
        registry: Arc::clone(&registry),
        endpoint: Arc::clone(&endpoint),
    });

    let outcome = harness
        .execute(AssignmentExecutionRequest {
            assignment,
            work,
            agent,
            run,
            input: EngineInput {
                message: "run once".into(),
                images: vec![],
                documents: vec![],
            },
            effective_permission: PermissionMode::Balanced,
            runtime_owner: "lease-owner".into(),
            extension_tool_ids: Vec::new(),
        })
        .await
        .unwrap();
    assert!(
        matches!(
            outcome,
            piwork_lib::engine::harness::AssignmentExecutionOutcome::Waiting { .. }
        ),
        "lead RunCompleted ends the harness attempt (delivery wait); LeaseGuard must already have revoked"
    );

    let (captured_run_id, token_hex) = engine
        .captured
        .lock()
        .unwrap()
        .clone()
        .expect("engine must observe the issued lease");
    assert_eq!(captured_run_id, run_id);
    assert!(
        registry.context(&run_id).is_none(),
        "terminal Run must revoke the lease so no context remains"
    );
    assert!(
        !registry.authenticate(&run_id, &decode_hex_token(&token_hex)),
        "revoked token must fail authentication"
    );

    let (status, status_line, body) = post_host_tool(
        server.endpoint(),
        &run_id,
        &token_hex,
        TOOL_GET_ASSIGNMENT_STATUS,
    );
    assert_eq!(status, 401, "status_line={status_line} body={body}");
    assert!(
        body.contains("unauthorized"),
        "lease failure must return a stable unauthorized error, got {body}"
    );
    assert_eq!(
        *dispatch_count.lock().unwrap(),
        0,
        "revoked lease must not invoke any product tool"
    );
    drop(server);
}

#[tokio::test]
async fn invalid_host_tool_lease_returns_stable_unauthorized_without_side_effects() {
    let registry = Arc::new(HostToolRegistry::new());
    let lease = registry.issue(
        AuthorizedRunContext {
            capability_snapshot_id: None,
            run_id: "run-invalid-lease".into(),
            work_id: "work-invalid-lease".into(),
            assignment_id: "assignment-invalid-lease".into(),
            agent_instance_id: "agent-instance:piwork-lead".into(),
            runtime_owner: "owner".into(),
            allowed_tools: vec![TOOL_GET_ASSIGNMENT_STATUS.into()],
        },
        "http://127.0.0.1:0/tool".into(),
    );

    let dispatch_count = Arc::new(Mutex::new(0_u32));
    let dispatch_seen = Arc::clone(&dispatch_count);
    let dispatch: Arc<ToolDispatch> = Arc::new(move |_tool, _context, _args| {
        *dispatch_seen.lock().unwrap() += 1;
        Ok(json!({ "ok": true }))
    });
    let server = HostToolServer::bind(Arc::clone(&registry), dispatch).unwrap();

    let forged = "00".repeat(32);
    let (status, _status_line, body) = post_host_tool(
        server.endpoint(),
        "run-invalid-lease",
        &forged,
        TOOL_GET_ASSIGNMENT_STATUS,
    );
    assert_eq!(status, 401);
    let parsed: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["error"], "unauthorized");

    registry.revoke("run-invalid-lease");
    let (status, _status_line, body) = post_host_tool(
        server.endpoint(),
        "run-invalid-lease",
        &lease.token.to_hex(),
        TOOL_GET_ASSIGNMENT_STATUS,
    );
    assert_eq!(status, 401);
    let parsed: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["error"], "unauthorized");
    assert_eq!(
        *dispatch_count.lock().unwrap(),
        0,
        "auth failures must never reach product tool dispatch"
    );
    drop(server);
}
