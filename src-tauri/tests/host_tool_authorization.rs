//! Characterization: Host Tool calls authorize and audit before dispatch (#19).
//!
//! Seams under test:
//! - `EngineHarness` compiles a Run Capability Snapshot and issues a Host Tool lease
//! - `HostToolServer::bind_with_capability_broker` (the production Host Tool path)
//! - `CapabilityBroker::authorize_and_record` + execution audit tables
//!
//! A `FakeEngineAdapter` (HoldUntilAbort) plus a thin observing wrapper captures
//! the issued lease. No real Pi sidecar or external connector is started.

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
    },
    capability::{CapabilityBroker, RunCapabilityRequest},
    collaboration::{
        tool_bridge::{AuthorizedRunContext, HostToolRegistry},
        tool_server::{HostToolServer, ToolDispatch},
        tools::{TOOL_GET_ASSIGNMENT_STATUS, TOOL_SUBMIT_ASSIGNMENT_RESULT},
    },
    domain::{
        agent::RoleKind,
        assignment::{AssignmentKind, AssignmentSideEffect},
        event::{WorkEventEnvelope, WorkEventPayload},
        work::PermissionMode,
    },
    engine::{
        EngineAdapter, EngineCapabilities, EngineError, EngineEvent, EngineInput, EngineRunContext,
        EngineSessionRef,
        fake::{FakeEngineAdapter, FakeEngineConfig, FakeRunBehavior},
        harness::{AssignmentExecutionRequest, EngineHarness, HostToolBridgeConfig},
        publisher::EventPublisher,
    },
    error::AppError,
    storage::sqlite::Database,
    work::repository::WorkRepository,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

type DecisionAudit = (String, String);
type DecisionAuditLog = Arc<Mutex<Vec<Vec<DecisionAudit>>>>;
type ExecutionStatusLog = Arc<Mutex<Vec<Vec<String>>>>;

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

#[derive(Clone)]
struct CapturedLease {
    run_id: String,
    token_hex: String,
}

#[derive(Clone)]
struct LeaseObservingEngine {
    inner: FakeEngineAdapter,
    captured: Arc<Mutex<Option<CapturedLease>>>,
}

impl LeaseObservingEngine {
    fn holding() -> Self {
        Self {
            inner: FakeEngineAdapter::configured(
                FakeEngineConfig::new(EngineCapabilities {
                    cancel: true,
                    ..EngineCapabilities::default()
                })
                .with_run_behavior(FakeRunBehavior::HoldUntilAbort),
            ),
            captured: Arc::new(Mutex::new(None)),
        }
    }

    fn captured(&self) -> Option<CapturedLease> {
        self.captured.lock().unwrap().clone()
    }
}

#[async_trait]
impl EngineAdapter for LeaseObservingEngine {
    fn kind(&self) -> &'static str {
        self.inner.kind()
    }

    fn capabilities(&self) -> EngineCapabilities {
        self.inner.capabilities()
    }

    async fn start(
        &self,
        context: EngineRunContext,
        input: EngineInput,
        sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        let lease = context
            .host_tool_lease()
            .ok_or_else(|| EngineError::Start("expected a Host Tool lease for this Run".into()))?;
        *self.captured.lock().unwrap() = Some(CapturedLease {
            run_id: context.run_id().to_owned(),
            token_hex: lease.token.to_hex(),
        });
        self.inner.start(context, input, sink).await
    }

    async fn abort(&self, run_id: &str) -> Result<(), EngineError> {
        self.inner.abort(run_id).await
    }
}

async fn seed_work(pool: &sqlx::SqlitePool, work_id: &str, root_path: &str) {
    let now = Utc.with_ymd_and_hms(2026, 8, 30, 1, 0, 0).unwrap();
    sqlx::query(
        "INSERT INTO works (id, title, goal, root_path, permission_mode, status, created_at, updated_at) \
         VALUES (?, 'Host Tool auth', 'Lock the authorization path', ?, 'balanced', 'draft', ?, ?)",
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

fn post_host_tool(
    endpoint: &str,
    run_id: &str,
    token_hex: &str,
    tool: &str,
    arguments: Value,
) -> (u16, String) {
    let body = json!({
        "runId": run_id,
        "token": token_hex,
        "tool": tool,
        "arguments": arguments
    })
    .to_string();
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
    let status = response
        .lines()
        .next()
        .unwrap_or("")
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
    (status, body)
}

async fn post_host_tool_async(
    endpoint: String,
    run_id: String,
    token_hex: String,
    tool: String,
    arguments: Value,
) -> (u16, String) {
    tokio::task::spawn_blocking(move || {
        post_host_tool(&endpoint, &run_id, &token_hex, &tool, arguments)
    })
    .await
    .expect("host tool POST worker")
}

async fn wait_for_lease(engine: &LeaseObservingEngine) -> CapturedLease {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(captured) = engine.captured() {
                return captured;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("harness must issue a Host Tool lease before the FakeEngine starts")
}

async fn decision_rows(pool: &sqlx::SqlitePool) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT decision, operation_json FROM capability_decisions ORDER BY created_at, id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn execution_statuses(pool: &sqlx::SqlitePool) -> Vec<String> {
    sqlx::query_scalar("SELECT status FROM capability_executions ORDER BY started_at, id")
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn work_decision_count(pool: &sqlx::SqlitePool, work_id: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE work_id = ? AND json_extract(payload, '$.type') = 'workDecisionRecorded'",
    )
    .bind(work_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn assignment_count(pool: &sqlx::SqlitePool, work_id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM assignments WHERE work_id = ?")
        .bind(work_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

struct LiveHostToolRun {
    _temp: tempfile::TempDir,
    _database: Database,
    pool: sqlx::SqlitePool,
    broker: CapabilityBroker,
    registry: Arc<HostToolRegistry>,
    server: HostToolServer,
    engine: LeaseObservingEngine,
    execute: tokio::task::JoinHandle<
        Result<piwork_lib::engine::harness::AssignmentExecutionOutcome, AppError>,
    >,
    assignment_id: String,
    work_id: String,
    run_id: String,
    token_hex: String,
    dispatch_calls: Arc<Mutex<Vec<String>>>,
    audits_at_dispatch: DecisionAuditLog,
    executions_at_dispatch: ExecutionStatusLog,
}

impl LiveHostToolRun {
    async fn start(work_id: &str, assignment_id: &str, mutate_on_dispatch: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let database = Database::open_in_memory().await.unwrap();
        let pool = database.pool().clone();
        let root_path = temp.path().to_string_lossy().into_owned();
        seed_work(&pool, work_id, &root_path).await;

        let publisher = Arc::new(RecordingPublisher);
        let repository =
            AssignmentRepository::with_event_sink(pool.clone(), Arc::clone(&publisher) as _);
        let work_repository = WorkRepository::new(pool.clone());
        let agent_repository = AgentRepository::new(pool.clone());

        let assignment = repository
            .accept(AcceptAssignmentInput {
                id: Some(assignment_id.into()),
                work_id: work_id.into(),
                parent_assignment_id: None,
                created_by_agent_id: None,
                assigned_agent_id: "agent-instance:piwork-lead".into(),
                capability_pack_id: None,
                kind: AssignmentKind::Lead,
                side_effect: AssignmentSideEffect::ReadOnly,
                title: "Host Tool auth".into(),
                instruction: "Hold while Host Tools are posted".into(),
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
            .claim(&assignment.id, "host-tool-auth-owner", now)
            .await
            .unwrap();
        let run = repository
            .begin_attempt(&assignment.id, "fake", "Pi")
            .await
            .unwrap();
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

        let broker = CapabilityBroker::new(pool.clone());
        let registry = Arc::new(HostToolRegistry::new());
        let endpoint = Arc::new(OnceLock::new());
        let dispatch_calls = Arc::new(Mutex::new(Vec::new()));
        let audits_at_dispatch = Arc::new(Mutex::new(Vec::new()));
        let executions_at_dispatch = Arc::new(Mutex::new(Vec::new()));
        let runtime = tokio::runtime::Handle::current();
        let seen_calls = Arc::clone(&dispatch_calls);
        let seen_audits = Arc::clone(&audits_at_dispatch);
        let seen_executions = Arc::clone(&executions_at_dispatch);
        let audit_pool = pool.clone();
        let mutation_repository = repository.clone();
        let mutation_assignment_id = assignment.id.clone();
        let dispatch: Arc<ToolDispatch> = Arc::new(move |tool, _context, _args| {
            let decisions = runtime.block_on(decision_rows(&audit_pool));
            let executions = runtime.block_on(execution_statuses(&audit_pool));
            seen_audits.lock().unwrap().push(decisions);
            seen_executions.lock().unwrap().push(executions);
            seen_calls.lock().unwrap().push(tool.to_owned());
            if mutate_on_dispatch {
                runtime
                    .block_on(mutation_repository.emit_collaboration_event(
                        &mutation_assignment_id,
                        WorkEventPayload::WorkDecisionRecorded {
                            decision_id: "leaked-host-tool-side-effect".into(),
                            summary: "dispatch reached the domain".into(),
                            version: 1,
                        },
                    ))
                    .expect("domain mutation probe must persist if dispatch runs");
            }
            Ok(json!({ "ok": true, "tool": tool }))
        });
        let server = HostToolServer::bind_with_capability_broker(
            Arc::clone(&registry),
            dispatch,
            broker.clone(),
        )
        .unwrap();
        endpoint.set(server.endpoint().to_owned()).unwrap();

        let engine = LeaseObservingEngine::holding();
        let harness = Arc::new(
            EngineHarness::new(
                Arc::new(engine.clone()) as _,
                work_repository,
                repository,
                Arc::clone(&publisher) as _,
            )
            .with_capability_broker(broker.clone())
            .with_host_tools(HostToolBridgeConfig {
                registry: Arc::clone(&registry),
                endpoint,
            }),
        );

        let run_id = run.id.clone();
        let execute = tokio::spawn({
            let harness = Arc::clone(&harness);
            async move {
                harness
                    .execute(AssignmentExecutionRequest {
                        assignment,
                        work,
                        agent,
                        run,
                        input: EngineInput {
                            message: "hold for host tools".into(),
                            images: vec![],
                            documents: vec![],
                        },
                        effective_permission: PermissionMode::Balanced,
                        runtime_owner: "host-tool-auth-owner".into(),
                        extension_tool_ids: Vec::new(),
                    })
                    .await
            }
        });

        let captured = wait_for_lease(&engine).await;
        assert_eq!(captured.run_id, run_id);

        Self {
            _temp: temp,
            _database: database,
            pool,
            broker,
            registry,
            server,
            engine,
            execute,
            assignment_id: assignment_id.to_owned(),
            work_id: work_id.to_owned(),
            run_id,
            token_hex: captured.token_hex,
            dispatch_calls,
            audits_at_dispatch,
            executions_at_dispatch,
        }
    }

    fn endpoint(&self) -> String {
        self.server.endpoint().to_owned()
    }

    async fn post(&self, tool: &str, arguments: Value) -> (u16, String) {
        post_host_tool_async(
            self.endpoint(),
            self.run_id.clone(),
            self.token_hex.clone(),
            tool.to_owned(),
            arguments,
        )
        .await
    }

    async fn shutdown(self) {
        let _ = self.engine.abort(&self.run_id).await;
        let _ = self.execute.await;
        drop(self.server);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_tool_call_authorizes_and_records_audit_before_dispatch() {
    let run = LiveHostToolRun::start("work-host-allow", "assignment-host-allow", false).await;

    assert!(
        decision_rows(&run.pool).await.is_empty(),
        "no CapabilityBroker decision should exist before the Host Tool call"
    );
    assert!(
        execution_statuses(&run.pool).await.is_empty(),
        "no capability execution should exist before the Host Tool call"
    );

    let (status, body) = run
        .post(TOOL_GET_ASSIGNMENT_STATUS, json!({"assignmentIds": []}))
        .await;
    assert_eq!(
        status, 200,
        "authorized Host Tool must dispatch, body={body}"
    );
    let parsed: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["tool"], TOOL_GET_ASSIGNMENT_STATUS);

    assert_eq!(
        run.dispatch_calls.lock().unwrap().as_slice(),
        [TOOL_GET_ASSIGNMENT_STATUS],
        "exactly one Host Tool dispatch"
    );
    let audits = run.audits_at_dispatch.lock().unwrap().clone();
    assert_eq!(audits.len(), 1);
    assert_eq!(
        audits[0].len(),
        1,
        "dispatch must observe exactly one already-recorded authorization"
    );
    assert_eq!(audits[0][0].0, "allow");
    assert!(
        audits[0][0].1.contains(TOOL_GET_ASSIGNMENT_STATUS),
        "audit operation must name the Host Tool: {}",
        audits[0][0].1
    );
    let executions = run.executions_at_dispatch.lock().unwrap().clone();
    assert_eq!(
        executions[0].as_slice(),
        ["started"],
        "begin_execution must record an in-flight audit before domain dispatch"
    );

    let after_decisions = decision_rows(&run.pool).await;
    assert_eq!(
        after_decisions.len(),
        1,
        "one Host Tool call records one authorization"
    );
    assert_eq!(after_decisions[0].0, "allow");
    assert_eq!(
        execution_statuses(&run.pool).await.as_slice(),
        ["succeeded"],
        "the pre-dispatch execution audit must complete after dispatch"
    );

    run.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unauthorized_or_out_of_scope_host_tool_returns_stable_error_without_side_effects() {
    let run = LiveHostToolRun::start("work-host-deny", "assignment-host-deny", true).await;
    let assignments_before = assignment_count(&run.pool, &run.work_id).await;
    assert_eq!(work_decision_count(&run.pool, &run.work_id).await, 0);

    for (tool, label) in [
        ("invented_host_tool", "unknown Host Tool"),
        (
            TOOL_SUBMIT_ASSIGNMENT_RESULT,
            "Member Host Tool on a Lead Run",
        ),
    ] {
        let (status, body) = run.post(tool, json!({})).await;
        assert_eq!(status, 403, "{label} status, body={body}");
        let parsed: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            parsed["error"], "tool not authorized for this run",
            "{label} must return the stable out-of-scope error, got {body}"
        );
    }

    let snapshot_id = run
        .registry
        .context(&run.run_id)
        .and_then(|context| context.capability_snapshot_id)
        .expect("harness lease carries a Run Capability Snapshot");
    let snapshot = run.broker.inspect(&snapshot_id).await.unwrap().unwrap();
    let out_of_scope_lease = run.registry.issue(
        AuthorizedRunContext {
            capability_snapshot_id: Some(snapshot.id.clone()),
            run_id: run.run_id.clone(),
            work_id: run.work_id.clone(),
            assignment_id: run.assignment_id.clone(),
            agent_instance_id: "agent-instance:piwork-lead".into(),
            runtime_owner: "host-tool-auth-owner".into(),
            allowed_tools: vec![
                TOOL_GET_ASSIGNMENT_STATUS.into(),
                "invented_host_tool".into(),
            ],
        },
        run.endpoint(),
    );
    let (status, body) = post_host_tool_async(
        run.endpoint(),
        run.run_id.clone(),
        out_of_scope_lease.token.to_hex(),
        "invented_host_tool".into(),
        json!({}),
    )
    .await;
    assert_eq!(status, 403, "broker-denied Host Tool status, body={body}");
    let parsed: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        parsed["error"], "capability snapshot denied the tool",
        "a Host Tool outside the Run Capability Snapshot must be denied by CapabilityBroker"
    );

    assert!(
        run.dispatch_calls.lock().unwrap().is_empty(),
        "unauthorized or out-of-scope Host Tools must not reach domain dispatch"
    );
    assert_eq!(
        work_decision_count(&run.pool, &run.work_id).await,
        0,
        "denied Host Tools must not emit Work decisions"
    );
    assert_eq!(
        assignment_count(&run.pool, &run.work_id).await,
        assignments_before,
        "denied Host Tools must not create Assignments"
    );
    assert!(
        execution_statuses(&run.pool).await.is_empty(),
        "denied Host Tools must not start a capability execution"
    );
    let decisions = decision_rows(&run.pool).await;
    assert_eq!(
        decisions.len(),
        1,
        "only the broker-evaluated out-of-scope call records an audit"
    );
    assert_eq!(decisions[0].0, "deny");
    assert!(
        decisions[0].1.contains("invented_host_tool"),
        "deny audit must name the out-of-scope Host Tool: {}",
        decisions[0].1
    );

    run.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_or_invalid_capability_snapshot_rejects_host_tool_instead_of_allowing() {
    let run = LiveHostToolRun::start("work-host-snapshot", "assignment-host-snapshot", true).await;
    let snapshot_id = run
        .registry
        .context(&run.run_id)
        .and_then(|context| context.capability_snapshot_id)
        .expect("harness lease carries a Run Capability Snapshot");

    let missing = run.registry.issue(
        AuthorizedRunContext {
            capability_snapshot_id: None,
            run_id: "run-missing-snapshot".into(),
            work_id: run.work_id.clone(),
            assignment_id: run.assignment_id.clone(),
            agent_instance_id: "agent-instance:piwork-lead".into(),
            runtime_owner: "host-tool-auth-owner".into(),
            allowed_tools: vec![TOOL_GET_ASSIGNMENT_STATUS.into()],
        },
        run.endpoint(),
    );
    let (status, body) = post_host_tool_async(
        run.endpoint(),
        "run-missing-snapshot".into(),
        missing.token.to_hex(),
        TOOL_GET_ASSIGNMENT_STATUS.into(),
        json!({}),
    )
    .await;
    assert_eq!(status, 403, "missing snapshot status, body={body}");
    let parsed: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        parsed["error"], "capability snapshot is required",
        "a Host Tool lease without a Run Capability Snapshot must be rejected"
    );

    let unknown = run.registry.issue(
        AuthorizedRunContext {
            capability_snapshot_id: Some("snapshot-does-not-exist".into()),
            run_id: "run-unknown-snapshot".into(),
            work_id: run.work_id.clone(),
            assignment_id: run.assignment_id.clone(),
            agent_instance_id: "agent-instance:piwork-lead".into(),
            runtime_owner: "host-tool-auth-owner".into(),
            allowed_tools: vec![TOOL_GET_ASSIGNMENT_STATUS.into()],
        },
        run.endpoint(),
    );
    let (status, body) = post_host_tool_async(
        run.endpoint(),
        "run-unknown-snapshot".into(),
        unknown.token.to_hex(),
        TOOL_GET_ASSIGNMENT_STATUS.into(),
        json!({}),
    )
    .await;
    assert_eq!(status, 403, "unknown snapshot status, body={body}");
    let parsed: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        parsed["error"], "capability snapshot is unavailable",
        "a Host Tool lease pointing at a missing Run Capability Snapshot must be rejected"
    );

    run.broker.revoke(&snapshot_id).await.unwrap();
    let (status, body) = run
        .post(TOOL_GET_ASSIGNMENT_STATUS, json!({"assignmentIds": []}))
        .await;
    assert_eq!(status, 403, "revoked snapshot status, body={body}");
    let parsed: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        parsed["error"], "capability snapshot denied the tool",
        "a revoked Run Capability Snapshot must deny the Host Tool"
    );

    let now = Utc::now();
    sqlx::query(
        "UPDATE run_capability_snapshots SET revoked_at = NULL, created_at = ?, expires_at = ? WHERE id = ?",
    )
    .bind(now - chrono::Duration::seconds(10))
    .bind(now - chrono::Duration::seconds(1))
    .bind(&snapshot_id)
    .execute(&run.pool)
    .await
    .unwrap();
    let (status, body) = run
        .post(TOOL_GET_ASSIGNMENT_STATUS, json!({"assignmentIds": []}))
        .await;
    assert_eq!(status, 403, "expired snapshot status, body={body}");
    let parsed: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        parsed["error"], "capability snapshot denied the tool",
        "an expired Run Capability Snapshot must deny the Host Tool"
    );

    assert!(
        run.dispatch_calls.lock().unwrap().is_empty(),
        "missing or invalid snapshots must not reach domain dispatch"
    );
    assert_eq!(
        work_decision_count(&run.pool, &run.work_id).await,
        0,
        "rejected snapshot Host Tools must not emit Work decisions"
    );
    assert!(
        execution_statuses(&run.pool).await.is_empty(),
        "rejected snapshot Host Tools must not start a capability execution"
    );
    let decisions = decision_rows(&run.pool).await;
    assert!(
        decisions.iter().all(|row| row.0 == "deny"),
        "invalid snapshots may audit a deny, never an allow: {decisions:?}"
    );
    assert!(
        !decisions.is_empty(),
        "revoked/invalid/expired snapshots must record deny audits rather than skip the broker"
    );

    run.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compiled_snapshot_without_host_tool_grant_is_rejected_on_the_broker_path() {
    let root = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    seed_work(&pool, "work-host-grant", &root.path().to_string_lossy()).await;
    sqlx::query(
        "INSERT INTO assignments (id, work_id, assigned_agent_id, kind, side_effect, title, instruction, context_manifest_json, expected_result_schema_json, acceptance_criteria_json, permission_scope_json, priority, status, attempt_count, max_attempts, created_at, updated_at) VALUES ('assignment-host-grant', 'work-host-grant', 'agent-instance:piwork-lead', 'lead', 'unknown', 'Grant', 'Test', '{}', '{}', '[]', '{\"mode\":\"inherit_work\"}', 10, 'running', 1, 3, ?, ?)",
    )
    .bind(Utc::now())
    .bind(Utc::now())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO runs (id, work_id, engine_kind, model_label, status, created_at, updated_at, assignment_id, agent_instance_id, attempt_number) VALUES ('run-host-grant', 'work-host-grant', 'fake', 'fake', 'running', ?, ?, 'assignment-host-grant', 'agent-instance:piwork-lead', 1)",
    )
    .bind(Utc::now())
    .bind(Utc::now())
    .execute(&pool)
    .await
    .unwrap();

    let broker = CapabilityBroker::new(pool.clone());
    let snapshot = broker
        .snapshot(RunCapabilityRequest {
            run_id: "run-host-grant".into(),
            work_id: "work-host-grant".into(),
            assignment_id: "assignment-host-grant".into(),
            agent_instance_id: "agent-instance:piwork-lead".into(),
            role_kind: RoleKind::Lead,
            permission_mode: PermissionMode::Balanced,
            workspace_root: root.path().to_path_buf(),
            expert_pack_ids: vec![],
            host_tool_ids: vec![TOOL_GET_ASSIGNMENT_STATUS.into()],
            extension_tool_ids: vec![],
            expires_at: None,
        })
        .await
        .unwrap();

    let registry = Arc::new(HostToolRegistry::new());
    let lease = registry.issue(
        AuthorizedRunContext {
            capability_snapshot_id: Some(snapshot.id.clone()),
            run_id: "run-host-grant".into(),
            work_id: "work-host-grant".into(),
            assignment_id: "assignment-host-grant".into(),
            agent_instance_id: "agent-instance:piwork-lead".into(),
            runtime_owner: "owner".into(),
            allowed_tools: vec![
                TOOL_GET_ASSIGNMENT_STATUS.into(),
                TOOL_SUBMIT_ASSIGNMENT_RESULT.into(),
            ],
        },
        "http://127.0.0.1:0/tool".into(),
    );
    let dispatch_calls = Arc::new(Mutex::new(0_u32));
    let seen = Arc::clone(&dispatch_calls);
    let dispatch: Arc<ToolDispatch> = Arc::new(move |_tool, _context, _args| {
        *seen.lock().unwrap() += 1;
        Ok(json!({ "ok": true }))
    });
    let server =
        HostToolServer::bind_with_capability_broker(Arc::clone(&registry), dispatch, broker)
            .unwrap();

    let (status, body) = post_host_tool_async(
        server.endpoint().to_owned(),
        "run-host-grant".into(),
        lease.token.to_hex(),
        TOOL_SUBMIT_ASSIGNMENT_RESULT.into(),
        json!({}),
    )
    .await;
    assert_eq!(status, 403, "ungranted Host Tool status, body={body}");
    let parsed: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["error"], "capability snapshot denied the tool");
    assert_eq!(
        *dispatch_calls.lock().unwrap(),
        0,
        "a Host Tool missing from the Run Capability Snapshot must not dispatch"
    );
    let decisions = decision_rows(&pool).await;
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].0, "deny");
    assert!(execution_statuses(&pool).await.is_empty());
    drop(server);
}
