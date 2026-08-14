use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use piwork_lib::{
    agent::repository::AgentRepository,
    app_state::AppState,
    domain::{
        event::{WorkEventEnvelope, WorkEventPayload},
        work::{
            CreateWorkInput, MessageRole, PermissionMode, RunStatus, StartWorkInput, WorkDetail,
            WorkStatus,
        },
    },
    engine::{
        EngineAdapter, EngineError, EngineEvent, EngineInput, EngineRunContext, EngineSessionRef,
        fake::FakeEngineAdapter,
        publisher::{ChannelEventPublisher, EventPublisher},
        supervisor::EngineSupervisor,
    },
    storage::sqlite::Database,
    work::{repository::WorkRepository, service::WorkService},
};
use tokio::sync::mpsc;
use uuid::Uuid;

struct TestHarness {
    _temporary_directory: tempfile::TempDir,
    pool: sqlx::SqlitePool,
    repository: WorkRepository,
    workspace_path: std::path::PathBuf,
}

struct StartCountingEngine {
    start_count: std::sync::atomic::AtomicUsize,
}

struct PostReturnOverflowEngine {
    started: Arc<tokio::sync::Semaphore>,
    release_start: Arc<tokio::sync::Semaphore>,
    overflowed: Arc<tokio::sync::Semaphore>,
    abort_calls: std::sync::atomic::AtomicUsize,
}

#[derive(Default)]
struct PromptRecordingEngine {
    prompt: Mutex<Option<String>>,
}

#[async_trait]
impl EngineAdapter for PromptRecordingEngine {
    fn kind(&self) -> &'static str {
        "prompt-recording"
    }

    async fn start(
        &self,
        _context: EngineRunContext,
        input: EngineInput,
        sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        *self.prompt.lock().unwrap() = Some(input.message);
        sink.send(EngineEvent::RunStarted {
            model_label: "Recording model".into(),
        })
        .await
        .map_err(|_| EngineError::ChannelClosed)?;
        sink.send(EngineEvent::RunCompleted {
            summary: "prompt recorded".into(),
            artifacts: Vec::new(),
            validation: Vec::new(),
            limitations: Vec::new(),
        })
        .await
        .map_err(|_| EngineError::ChannelClosed)?;
        Ok(EngineSessionRef {
            engine_kind: self.kind().into(),
            session_id: "recording-session".into(),
        })
    }

    async fn abort(&self, _run_id: &str) -> Result<(), EngineError> {
        Err(EngineError::NotRunning)
    }
}

impl StartCountingEngine {
    fn new() -> Self {
        Self {
            start_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn start_count(&self) -> usize {
        self.start_count.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl PostReturnOverflowEngine {
    fn new() -> Self {
        Self {
            started: Arc::new(tokio::sync::Semaphore::new(0)),
            release_start: Arc::new(tokio::sync::Semaphore::new(0)),
            overflowed: Arc::new(tokio::sync::Semaphore::new(0)),
            abort_calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    async fn wait_started(&self) {
        self.started.acquire().await.unwrap().forget();
    }

    fn release_start(&self) {
        self.release_start.add_permits(1);
    }

    async fn wait_overflowed(&self) {
        tokio::time::timeout(std::time::Duration::from_secs(1), self.overflowed.acquire())
            .await
            .expect("engine did not overflow the startup event buffer")
            .unwrap()
            .forget();
    }

    fn abort_calls(&self) -> usize {
        self.abort_calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait]
impl EngineAdapter for StartCountingEngine {
    fn kind(&self) -> &'static str {
        "start-counting"
    }

    async fn start(
        &self,
        _context: EngineRunContext,
        _input: EngineInput,
        _sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        self.start_count
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(EngineError::Start("unexpected recovery start".into()))
    }

    async fn abort(&self, _run_id: &str) -> Result<(), EngineError> {
        Err(EngineError::NotRunning)
    }
}

#[async_trait]
impl EngineAdapter for PostReturnOverflowEngine {
    fn kind(&self) -> &'static str {
        "post-return-overflow"
    }

    async fn start(
        &self,
        _context: EngineRunContext,
        _input: EngineInput,
        sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        self.started.add_permits(1);
        self.release_start
            .acquire()
            .await
            .map_err(|_| EngineError::Start("start gate closed".into()))?
            .forget();
        let overflowed = Arc::clone(&self.overflowed);
        tokio::spawn(async move {
            for index in 0..=1_000 {
                if sink
                    .send(EngineEvent::AssistantDelta {
                        text: format!("startup-chunk-{index}"),
                    })
                    .await
                    .is_err()
                {
                    break;
                }
            }
            overflowed.add_permits(1);
        });
        Ok(EngineSessionRef {
            engine_kind: self.kind().into(),
            session_id: "overflow-session".into(),
        })
    }

    async fn abort(&self, _run_id: &str) -> Result<(), EngineError> {
        self.abort_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}

impl TestHarness {
    async fn new() -> Self {
        let temporary_directory = tempfile::tempdir().unwrap();
        let workspace_path = temporary_directory.path().join("workspace");
        std::fs::create_dir(&workspace_path).unwrap();
        let database = Database::open_in_memory().await.unwrap();
        let repository = WorkRepository::new(database.pool().clone());

        Self {
            _temporary_directory: temporary_directory,
            pool: database.pool().clone(),
            repository,
            workspace_path,
        }
    }

    async fn create_work(&self, title: &str) -> piwork_lib::domain::work::WorkDetail {
        self.repository
            .create(CreateWorkInput {
                title: title.into(),
                goal: "Exercise the engine pipeline".into(),
                root_path: self.workspace_path.to_string_lossy().into_owned(),
                permission_mode: PermissionMode::Balanced,
                resource_draft_id: None,
            })
            .await
            .unwrap()
    }
}

#[tokio::test]
async fn creating_a_work_atomically_assigns_the_builtin_lead() {
    let harness = TestHarness::new().await;

    let work = harness.create_work("Atomic lead").await;
    let memberships: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT agent_instance_id, role_kind, status FROM work_agents WHERE work_id = ?",
    )
    .bind(&work.summary.id)
    .fetch_all(&harness.pool)
    .await
    .unwrap();
    let lead: (String,) =
        sqlx::query_as("SELECT agent_instance_id FROM work_leads WHERE work_id = ?")
            .bind(&work.summary.id)
            .fetch_one(&harness.pool)
            .await
            .unwrap();

    assert_eq!(
        memberships,
        vec![(
            "agent-instance:piwork-lead".into(),
            "lead".into(),
            "joined".into(),
        )]
    );
    assert_eq!(lead.0, "agent-instance:piwork-lead");
}

#[tokio::test]
async fn failed_lead_binding_rolls_back_the_new_work() {
    let harness = TestHarness::new().await;
    sqlx::query("DELETE FROM agent_instances WHERE id = 'agent-instance:piwork-lead'")
        .execute(&harness.pool)
        .await
        .unwrap();

    let result = harness
        .repository
        .create(CreateWorkInput {
            title: "Must roll back".into(),
            goal: "Prove atomicity".into(),
            root_path: harness.workspace_path.to_string_lossy().into_owned(),
            permission_mode: PermissionMode::Balanced,
            resource_draft_id: None,
        })
        .await;

    assert!(matches!(
        result,
        Err(piwork_lib::error::AppError::Database(
            sqlx::Error::Database(_)
        ))
    ));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM works WHERE title = ?")
        .bind("Must roll back")
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn replacing_a_work_lead_keeps_exactly_one_lead_and_both_memberships_auditable() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Replace lead").await;
    sqlx::query(
        "INSERT INTO agent_instances \
         (id, definition_id, display_name, engine_override, model_configuration_override, \
          permission_policy_override, parallelism_override, builtin, status, created_at, updated_at) \
         SELECT 'agent-instance:alternate-lead', definition_id, 'Alternate lead', NULL, NULL, \
                NULL, NULL, 0, 'active', created_at, updated_at \
         FROM agent_instances WHERE id = 'agent-instance:piwork-lead'",
    )
    .execute(&harness.pool)
    .await
    .unwrap();
    let repository = AgentRepository::new(harness.pool.clone());

    let added = repository
        .add_work_member(&work.summary.id, "agent-instance:alternate-lead")
        .await
        .unwrap();
    assert_eq!(added.members.len(), 2);
    let team = repository
        .set_work_lead(&work.summary.id, "agent-instance:alternate-lead")
        .await
        .unwrap();

    assert_eq!(team.lead.instance.id, "agent-instance:alternate-lead");
    assert_eq!(team.members.len(), 2);
    assert!(
        team.members
            .iter()
            .all(|member| member.status == piwork_lib::domain::agent::WorkAgentStatus::Joined)
    );
    assert!(team.members.iter().any(|member| {
        member.instance.id == "agent-instance:piwork-lead"
            && member.role_kind == piwork_lib::domain::agent::RoleKind::Lead
    }));
    let lead_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM work_leads WHERE work_id = ?")
        .bind(&work.summary.id)
        .fetch_one(&harness.pool)
        .await
        .unwrap();
    let member_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM work_agents WHERE work_id = ?")
            .bind(&work.summary.id)
            .fetch_one(&harness.pool)
            .await
            .unwrap();
    assert_eq!(lead_count, 1);
    assert_eq!(member_count, 2);
}

#[tokio::test]
async fn work_team_reads_fail_closed_for_missing_leads_and_invalid_lead_instances() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Fail closed team").await;
    let repository = AgentRepository::new(harness.pool.clone());

    let missing = repository
        .set_work_lead(&work.summary.id, "agent-instance:missing")
        .await
        .unwrap_err();
    assert!(matches!(
        missing,
        piwork_lib::error::AppError::InvalidInput { ref field, .. } if field == "instanceId"
    ));

    let non_lead = repository
        .set_work_lead(&work.summary.id, "agent-instance:piwork-engineer")
        .await
        .unwrap_err();
    assert!(matches!(
        non_lead,
        piwork_lib::error::AppError::InvalidInput { ref field, .. } if field == "instanceId"
    ));
    let persisted_lead: String =
        sqlx::query_scalar("SELECT agent_instance_id FROM work_leads WHERE work_id = ?")
            .bind(&work.summary.id)
            .fetch_one(&harness.pool)
            .await
            .unwrap();
    assert_eq!(persisted_lead, "agent-instance:piwork-lead");

    sqlx::query("DELETE FROM work_leads WHERE work_id = ?")
        .bind(&work.summary.id)
        .execute(&harness.pool)
        .await
        .unwrap();
    assert!(matches!(
        repository.get_work_team(&work.summary.id).await,
        Err(piwork_lib::error::AppError::Database(sqlx::Error::Decode(
            _
        )))
    ));
    assert!(
        repository
            .get_work_team("missing-work")
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn work_team_read_keeps_one_snapshot_during_concurrent_work_deletion() {
    let temporary_directory = tempfile::tempdir().unwrap();
    let database_path = temporary_directory.path().join("team-snapshot.sqlite3");
    let workspace_path = temporary_directory.path().join("workspace");
    std::fs::create_dir(&workspace_path).unwrap();
    let database = Database::open(&database_path).await.unwrap();
    let work_repository = WorkRepository::new(database.pool().clone());
    let work = work_repository
        .create(CreateWorkInput {
            title: "Team snapshot".into(),
            goal: "Read a consistent team".into(),
            root_path: workspace_path.to_string_lossy().into_owned(),
            permission_mode: PermissionMode::Balanced,
            resource_draft_id: None,
        })
        .await
        .unwrap();
    let repository = AgentRepository::new(database.pool().clone());
    let writer_pool = database.pool().clone();
    let writer_work_id = work.summary.id.clone();
    let (start_writer, writer_started) = tokio::sync::oneshot::channel();
    let (writer_finished, wait_for_writer) = std::sync::mpsc::sync_channel(0);
    let writer = tokio::spawn(async move {
        writer_started.await.unwrap();
        sqlx::query("DELETE FROM works WHERE id = ?")
            .bind(&writer_work_id)
            .execute(&writer_pool)
            .await
            .unwrap();
        writer_finished.send(()).unwrap();
    });

    let team = repository
        .get_work_team_after_work_loaded(&work.summary.id, move || {
            start_writer.send(()).unwrap();
            wait_for_writer
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("concurrent Work deletion did not finish");
        })
        .await
        .unwrap()
        .unwrap();
    writer.await.unwrap();

    assert_eq!(team.work_id, work.summary.id);
    assert_eq!(team.lead.instance.id, "agent-instance:piwork-lead");
    assert_eq!(team.members.len(), 1);
    assert!(
        repository
            .get_work_team(&work.summary.id)
            .await
            .unwrap()
            .is_none()
    );
}

fn assert_authoritative_prompt(detail: &WorkDetail, prompt: &str, expected_run_status: RunStatus) {
    let matching_messages = detail
        .messages
        .iter()
        .filter(|message| message.content == prompt)
        .collect::<Vec<_>>();
    assert_eq!(
        matching_messages.len(),
        1,
        "expected exactly one authoritative user message for {prompt:?}"
    );
    let message = matching_messages[0];
    assert_eq!(message.role, MessageRole::User);
    let run = detail
        .runs
        .iter()
        .find(|run| run.id == message.run_id)
        .expect("authoritative user message must reference its persisted Run");
    assert_eq!(run.status, expected_run_status);
}

struct PersistAssertingPublisher {
    repository: WorkRepository,
    observed: mpsc::Sender<WorkEventEnvelope>,
}

impl PersistAssertingPublisher {
    fn channel(
        repository: WorkRepository,
        capacity: usize,
    ) -> (Self, mpsc::Receiver<WorkEventEnvelope>) {
        let (observed, receiver) = mpsc::channel(capacity);
        (
            Self {
                repository,
                observed,
            },
            receiver,
        )
    }
}

#[async_trait]
impl EventPublisher for PersistAssertingPublisher {
    async fn publish(
        &self,
        envelope: WorkEventEnvelope,
    ) -> Result<(), piwork_lib::error::AppError> {
        let persisted = self.repository.events_for_run(&envelope.run_id).await?;
        assert!(persisted.iter().any(|candidate| {
            candidate.run_id == envelope.run_id && candidate.sequence == envelope.sequence
        }));
        self.observed
            .send(envelope)
            .await
            .map_err(|error| piwork_lib::error::AppError::event_publish(error.to_string()))
    }
}

struct PanickingPublisher {
    panicked: std::sync::atomic::AtomicBool,
}

struct NoResourceStartFailingEngine {
    abort_calls: std::sync::atomic::AtomicUsize,
}

struct PanickingAbortEngine {
    abort_calls: std::sync::atomic::AtomicUsize,
}

impl PanickingAbortEngine {
    fn new() -> Self {
        Self {
            abort_calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn abort_calls(&self) -> usize {
        self.abort_calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait]
impl EngineAdapter for PanickingAbortEngine {
    fn kind(&self) -> &'static str {
        "panicking-abort"
    }

    async fn start(
        &self,
        _context: EngineRunContext,
        _input: EngineInput,
        _sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        Err(EngineError::Start("abort must recover this failure".into()))
    }

    async fn abort(&self, _run_id: &str) -> Result<(), EngineError> {
        self.abort_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        panic!("intentional abort panic");
    }
}

impl NoResourceStartFailingEngine {
    fn new() -> Self {
        Self {
            abort_calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn abort_calls(&self) -> usize {
        self.abort_calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait]
impl EngineAdapter for NoResourceStartFailingEngine {
    fn kind(&self) -> &'static str {
        "no-resource-start-failing"
    }

    async fn start(
        &self,
        _context: EngineRunContext,
        _input: EngineInput,
        _sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        Err(EngineError::Start(
            "failed before creating resources".into(),
        ))
    }

    async fn abort(&self, _run_id: &str) -> Result<(), EngineError> {
        self.abort_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(EngineError::NotRunning)
    }
}

#[derive(Clone, Copy)]
enum ControlledStartBehavior {
    Immediate,
    PartialError,
    Gated,
    Never,
}

struct ControlledResource {
    cancel: tokio::sync::watch::Sender<bool>,
    completion: tokio::sync::oneshot::Receiver<()>,
}

struct ControlledEngineState {
    behavior: ControlledStartBehavior,
    commands: tokio::sync::broadcast::Sender<EngineEvent>,
    resources: tokio::sync::Mutex<std::collections::HashMap<String, ControlledResource>>,
    started: tokio::sync::Semaphore,
    release_start: tokio::sync::Semaphore,
    abort_calls: std::sync::atomic::AtomicUsize,
    live_producers: std::sync::atomic::AtomicUsize,
    dropped_starts: std::sync::atomic::AtomicUsize,
    pause_next_abort: std::sync::atomic::AtomicBool,
    abort_paused: tokio::sync::Semaphore,
    release_abort: tokio::sync::Semaphore,
}

#[derive(Clone)]
struct ControlledEngine {
    state: Arc<ControlledEngineState>,
}

struct StartFutureGuard {
    state: Arc<ControlledEngineState>,
}

impl Drop for StartFutureGuard {
    fn drop(&mut self) {
        self.state
            .dropped_starts
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

impl ControlledEngine {
    fn new(behavior: ControlledStartBehavior) -> Self {
        let (commands, _) = tokio::sync::broadcast::channel(32);
        Self {
            state: Arc::new(ControlledEngineState {
                behavior,
                commands,
                resources: tokio::sync::Mutex::new(std::collections::HashMap::new()),
                started: tokio::sync::Semaphore::new(0),
                release_start: tokio::sync::Semaphore::new(0),
                abort_calls: std::sync::atomic::AtomicUsize::new(0),
                live_producers: std::sync::atomic::AtomicUsize::new(0),
                dropped_starts: std::sync::atomic::AtomicUsize::new(0),
                pause_next_abort: std::sync::atomic::AtomicBool::new(false),
                abort_paused: tokio::sync::Semaphore::new(0),
                release_abort: tokio::sync::Semaphore::new(0),
            }),
        }
    }

    fn new_with_paused_abort(behavior: ControlledStartBehavior) -> Self {
        let engine = Self::new(behavior);
        engine
            .state
            .pause_next_abort
            .store(true, std::sync::atomic::Ordering::SeqCst);
        engine
    }

    async fn wait_started(&self) {
        self.state.started.acquire().await.unwrap().forget();
    }

    fn release_start(&self) {
        self.state.release_start.add_permits(1);
    }

    async fn wait_until_abort_is_paused(&self) {
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            self.state.abort_paused.acquire(),
        )
        .await
        .expect("controlled engine did not pause its abort")
        .unwrap()
        .forget();
    }

    fn release_abort(&self) {
        self.state.release_abort.add_permits(1);
    }

    fn send(&self, event: EngineEvent) {
        self.state.commands.send(event).unwrap();
    }

    fn abort_calls(&self) -> usize {
        self.state
            .abort_calls
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    fn live_producers(&self) -> usize {
        self.state
            .live_producers
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    fn dropped_starts(&self) -> usize {
        self.state
            .dropped_starts
            .load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[async_trait]
impl EngineAdapter for ControlledEngine {
    fn kind(&self) -> &'static str {
        "controlled"
    }

    async fn start(
        &self,
        context: EngineRunContext,
        _input: EngineInput,
        sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        let _future_guard = StartFutureGuard {
            state: Arc::clone(&self.state),
        };
        let (cancel, mut cancellation) = tokio::sync::watch::channel(false);
        let (completion_sender, completion) = tokio::sync::oneshot::channel();
        let mut commands = self.state.commands.subscribe();
        let producer_state = Arc::clone(&self.state);
        self.state
            .live_producers
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    biased;
                    changed = cancellation.changed() => {
                        if changed.is_err() || *cancellation.borrow() {
                            if producer_state
                                .pause_next_abort
                                .swap(false, std::sync::atomic::Ordering::SeqCst)
                            {
                                producer_state.abort_paused.add_permits(1);
                                producer_state
                                    .release_abort
                                    .acquire()
                                    .await
                                    .expect("controlled abort release gate closed")
                                    .forget();
                            }
                            break;
                        }
                    }
                    command = commands.recv() => {
                        match command {
                            Ok(event) => {
                                if sink.send(event).await.is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
            producer_state
                .live_producers
                .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
            let _ = completion_sender.send(());
        });
        self.state
            .resources
            .lock()
            .await
            .insert(context.run_id, ControlledResource { cancel, completion });
        self.state.started.add_permits(1);

        match self.state.behavior {
            ControlledStartBehavior::Immediate => {}
            ControlledStartBehavior::PartialError => {
                return Err(EngineError::Start("partial controlled failure".into()));
            }
            ControlledStartBehavior::Gated => {
                self.state
                    .release_start
                    .acquire()
                    .await
                    .map_err(|_| EngineError::Start("start gate closed".into()))?
                    .forget();
            }
            ControlledStartBehavior::Never => std::future::pending::<()>().await,
        }

        Ok(EngineSessionRef {
            engine_kind: self.kind().into(),
            session_id: "controlled-session".into(),
        })
    }

    async fn abort(&self, run_id: &str) -> Result<(), EngineError> {
        self.state
            .abort_calls
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let resource = self
            .state
            .resources
            .lock()
            .await
            .remove(run_id)
            .ok_or(EngineError::NotRunning)?;
        let _ = resource.cancel.send(true);
        resource.completion.await.map_err(|_| EngineError::Aborted)
    }
}

async fn wait_for_no_controlled_producers(engine: &ControlledEngine) {
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while engine.live_producers() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("controlled producer remained alive");
}

#[tokio::test]
async fn stopping_an_active_work_aborts_once_and_persists_stopped_state() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Stop safely").await;
    let engine = ControlledEngine::new(ControlledStartBehavior::Immediate);
    let (publisher, _published) = ChannelEventPublisher::channel(16);
    let supervisor = EngineSupervisor::new(
        harness.repository.clone(),
        Arc::new(engine.clone()),
        Arc::new(publisher),
        "Controlled model",
    );

    let started = supervisor
        .start(&work.summary.id, "Keep working")
        .await
        .unwrap();
    supervisor.stop(&work.summary.id).await.unwrap();
    let detail = harness
        .repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(engine.abort_calls(), 1);
    assert_eq!(detail.summary.status, WorkStatus::Stopped);
    assert_eq!(detail.runs[0].id, started.run.id);
    assert_eq!(detail.runs[0].status, RunStatus::Stopped);
    assert!(detail.runs[0].completed_at.is_some());
    assert_authoritative_prompt(&detail, "Keep working", RunStatus::Stopped);
    wait_for_no_controlled_producers(&engine).await;

    let second = supervisor.stop(&work.summary.id).await.unwrap_err();
    assert_eq!(
        serde_json::to_value(second).unwrap()["code"],
        "engine_error"
    );
    assert_eq!(engine.abort_calls(), 1);
}

impl PanickingPublisher {
    fn new() -> Self {
        Self {
            panicked: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

#[async_trait]
impl EventPublisher for PanickingPublisher {
    async fn publish(
        &self,
        _envelope: WorkEventEnvelope,
    ) -> Result<(), piwork_lib::error::AppError> {
        if !self
            .panicked
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            panic!("intentional publisher panic");
        }
        Ok(())
    }
}

async fn receive_complete_run(
    receiver: &mut mpsc::Receiver<WorkEventEnvelope>,
) -> Vec<WorkEventEnvelope> {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let mut events = Vec::new();
        loop {
            let event = receiver.recv().await.unwrap();
            let terminal = matches!(
                event.payload,
                WorkEventPayload::RunCompleted { .. } | WorkEventPayload::RunFailed { .. }
            );
            events.push(event);
            if terminal {
                return events;
            }
        }
    })
    .await
    .expect("engine did not produce a terminal event")
}

#[tokio::test]
async fn receiver_observes_exact_event_id_only_after_it_is_journaled() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Persist before publish").await;
    let (publisher, mut published) =
        PersistAssertingPublisher::channel(harness.repository.clone(), 16);
    let supervisor = EngineSupervisor::new(
        harness.repository.clone(),
        Arc::new(FakeEngineAdapter::new(std::time::Duration::ZERO)),
        Arc::new(publisher),
        "Fake model",
    );

    let run = supervisor
        .start(&work.summary.id, "Build it")
        .await
        .unwrap();
    let event = published.recv().await.unwrap();
    assert_eq!(event.run_id, run.id);
    let persisted = harness.repository.events_for_run(&run.id).await.unwrap();
    assert!(event.event_id.is_some());
    assert!(
        persisted
            .iter()
            .any(|candidate| candidate.event_id == event.event_id),
        "the Publisher receiver observed an event before its exact eventId was queryable"
    );
}

#[tokio::test]
async fn start_work_returns_before_the_engine_stream_finishes() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Return immediately").await;
    let (publisher, _published) = ChannelEventPublisher::channel(16);
    let supervisor = Arc::new(EngineSupervisor::new(
        harness.repository.clone(),
        Arc::new(FakeEngineAdapter::new(std::time::Duration::from_millis(
            300,
        ))),
        Arc::new(publisher),
        "Fake model",
    ));
    let service = WorkService::with_supervisor(harness.repository, supervisor);

    let run = tokio::time::timeout(
        std::time::Duration::from_millis(150),
        service.start_work(
            &work.summary.id,
            StartWorkInput {
                prompt: "Build it".into(),
                referenced_files: Vec::new(),
                resource_ids: Vec::new(),
            },
        ),
    )
    .await
    .expect("start_work waited for the engine event stream")
    .unwrap();

    assert_eq!(run.status, RunStatus::Running);
}

struct BlockingStartEngine {
    entered: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
    release: tokio::sync::Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
}

impl BlockingStartEngine {
    fn new() -> (
        Self,
        tokio::sync::oneshot::Receiver<()>,
        tokio::sync::oneshot::Sender<()>,
    ) {
        let (entered_sender, entered_receiver) = tokio::sync::oneshot::channel();
        let (release_sender, release_receiver) = tokio::sync::oneshot::channel();
        (
            Self {
                entered: std::sync::Mutex::new(Some(entered_sender)),
                release: tokio::sync::Mutex::new(Some(release_receiver)),
            },
            entered_receiver,
            release_sender,
        )
    }
}

#[async_trait]
impl EngineAdapter for BlockingStartEngine {
    fn kind(&self) -> &'static str {
        "blocking-start"
    }

    async fn start(
        &self,
        _context: EngineRunContext,
        _input: EngineInput,
        sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        if let Some(entered) = self.entered.lock().unwrap().take() {
            let _ = entered.send(());
        }
        if let Some(release) = self.release.lock().await.take() {
            let _ = release.await;
        }
        sink.send(EngineEvent::RunStarted {
            model_label: "Blocking model".into(),
        })
        .await
        .map_err(|_| EngineError::ChannelClosed)?;
        sink.send(EngineEvent::RunCompleted {
            summary: "completed after caller cancellation".into(),
            artifacts: Vec::new(),
            validation: Vec::new(),
            limitations: Vec::new(),
        })
        .await
        .map_err(|_| EngineError::ChannelClosed)?;
        Ok(EngineSessionRef {
            engine_kind: self.kind().into(),
            session_id: "blocking-session".into(),
        })
    }

    async fn abort(&self, _run_id: &str) -> Result<(), EngineError> {
        Err(EngineError::NotRunning)
    }
}

#[tokio::test]
async fn cancelling_the_start_caller_does_not_cancel_engine_startup() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Cancellation safe start").await;
    let (engine, entered, release) = BlockingStartEngine::new();
    let (publisher, mut published) = ChannelEventPublisher::channel(16);
    let supervisor = Arc::new(EngineSupervisor::new(
        harness.repository.clone(),
        Arc::new(engine),
        Arc::new(publisher),
        "Blocking model",
    ));
    let caller_supervisor = Arc::clone(&supervisor);
    let work_id = work.summary.id.clone();
    let caller = tokio::spawn(async move { caller_supervisor.start(&work_id, "Build it").await });

    entered.await.unwrap();
    caller.abort();
    let _ = release.send(());
    let events = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        receive_complete_run(&mut published),
    )
    .await
    .expect("owned engine startup did not survive caller cancellation");
    let detail = harness
        .repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();

    assert!(matches!(
        events.last().unwrap().payload,
        WorkEventPayload::RunCompleted { .. }
    ));
    assert_eq!(detail.summary.status, WorkStatus::Completed);
}

#[tokio::test]
async fn context_failure_keeps_the_authoritative_prompt_and_failed_run() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Lose workspace before context").await;
    std::fs::remove_dir(&harness.workspace_path).unwrap();
    let engine = Arc::new(StartCountingEngine::new());
    let (publisher, _published) = ChannelEventPublisher::channel(16);
    let supervisor = EngineSupervisor::new(
        harness.repository.clone(),
        engine.clone(),
        Arc::new(publisher),
        "Context model",
    );

    let error = supervisor
        .start(&work.summary.id, "Context prompt")
        .await
        .unwrap_err();
    let detail = harness
        .repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(serde_json::to_value(error).unwrap()["code"], "engine_error");
    assert_eq!(engine.start_count(), 0);
    assert_eq!(detail.summary.status, WorkStatus::Failed);
    assert_authoritative_prompt(&detail, "Context prompt", RunStatus::Failed);
}

#[tokio::test]
async fn supervisor_times_out_and_terminates_an_engine_start_that_never_returns() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Timeout engine start").await;
    let engine = ControlledEngine::new(ControlledStartBehavior::Never);
    let (publisher, _published) = ChannelEventPublisher::channel(16);
    let supervisor = EngineSupervisor::with_timeouts(
        harness.repository.clone(),
        Arc::new(engine.clone()),
        Arc::new(publisher),
        "Controlled model",
        std::time::Duration::from_millis(50),
        std::time::Duration::from_millis(100),
    );

    let error = tokio::time::timeout(
        std::time::Duration::from_millis(300),
        supervisor.start(&work.summary.id, "Build it"),
    )
    .await
    .expect("supervisor did not own the engine startup deadline")
    .unwrap_err();
    let detail = harness
        .repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        serde_json::to_value(error).unwrap()["code"],
        "engine_start_failed"
    );
    assert_eq!(detail.summary.status, WorkStatus::Failed);
    assert_eq!(detail.runs[0].status, RunStatus::Failed);
    assert_authoritative_prompt(&detail, "Build it", RunStatus::Failed);
    assert_eq!(engine.abort_calls(), 1);
    assert_eq!(engine.dropped_starts(), 1);
    wait_for_no_controlled_producers(&engine).await;
}

#[tokio::test]
async fn startup_buffer_overflow_before_readiness_returns_error_and_keeps_prompt() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Fail consumer during startup").await;
    let engine = Arc::new(PostReturnOverflowEngine::new());
    let (publisher, _published) = ChannelEventPublisher::channel(1);
    let supervisor = Arc::new(EngineSupervisor::with_timeouts(
        harness.repository.clone(),
        engine.clone(),
        Arc::new(publisher),
        "Overflow model",
        std::time::Duration::from_secs(1),
        std::time::Duration::from_millis(100),
    ));
    let start_supervisor = Arc::clone(&supervisor);
    let work_id = work.summary.id.clone();
    let start =
        tokio::spawn(async move { start_supervisor.start(&work_id, "Consumer prompt").await });
    engine.wait_started().await;
    let attachment_blocker = harness.pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    engine.release_start();
    engine.wait_overflowed().await;
    attachment_blocker.commit().await.unwrap();

    let error = tokio::time::timeout(std::time::Duration::from_secs(1), start)
        .await
        .expect("startup readiness race did not resolve promptly")
        .unwrap()
        .unwrap_err();
    let detail = harness
        .repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        serde_json::to_value(error).unwrap()["code"],
        "engine_start_failed"
    );
    assert_eq!(engine.abort_calls(), 1);
    assert_eq!(detail.summary.status, WorkStatus::Failed);
    assert_eq!(detail.runs[0].status, RunStatus::Failed);
    assert_eq!(detail.events.len(), 1);
    assert!(matches!(
        detail.events[0].payload,
        WorkEventPayload::RunFailed { .. }
    ));
    assert_eq!(
        detail.events[0].session_id.as_deref(),
        Some("overflow-session")
    );
    assert_eq!(detail.events[0].causation_id, None);
    assert_authoritative_prompt(&detail, "Consumer prompt", RunStatus::Failed);
}

struct FloodingStartEngine;

#[async_trait]
impl EngineAdapter for FloodingStartEngine {
    fn kind(&self) -> &'static str {
        "flooding-start"
    }

    async fn start(
        &self,
        _context: EngineRunContext,
        _input: EngineInput,
        sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        sink.send(EngineEvent::RunStarted {
            model_label: "Flood model".into(),
        })
        .await
        .map_err(|_| EngineError::ChannelClosed)?;
        for index in 0..20 {
            sink.send(EngineEvent::AssistantDelta {
                text: format!("chunk-{index}"),
            })
            .await
            .map_err(|_| EngineError::ChannelClosed)?;
        }
        sink.send(EngineEvent::RunCompleted {
            summary: "flood completed".into(),
            artifacts: Vec::new(),
            validation: Vec::new(),
            limitations: Vec::new(),
        })
        .await
        .map_err(|_| EngineError::ChannelClosed)?;
        Ok(EngineSessionRef {
            engine_kind: self.kind().into(),
            session_id: "flood-session".into(),
        })
    }

    async fn abort(&self, _run_id: &str) -> Result<(), EngineError> {
        Err(EngineError::NotRunning)
    }
}

#[tokio::test]
async fn adapter_can_fill_more_than_the_engine_channel_before_start_returns() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Drain during start").await;
    let (publisher, mut published) = ChannelEventPublisher::channel(64);
    let supervisor = EngineSupervisor::new(
        harness.repository.clone(),
        Arc::new(FloodingStartEngine),
        Arc::new(publisher),
        "Flood model",
    );

    let run = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        supervisor.start(&work.summary.id, "Build it"),
    )
    .await
    .expect("adapter.start deadlocked against the bounded event channel")
    .unwrap();
    let events = receive_complete_run(&mut published).await;
    let persisted = harness.repository.events_for_run(&run.id).await.unwrap();

    assert_eq!(events.len(), 22);
    assert_eq!(persisted.len(), 22);
    assert_eq!(persisted.last().unwrap().sequence, 22);
}

#[tokio::test]
async fn partial_engine_start_failure_aborts_once_before_clearing_active() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Fail safely").await;
    let engine = ControlledEngine::new_with_paused_abort(ControlledStartBehavior::PartialError);
    let (publisher, _published) = ChannelEventPublisher::channel(16);
    let supervisor = Arc::new(EngineSupervisor::new(
        harness.repository.clone(),
        Arc::new(engine.clone()),
        Arc::new(publisher),
        "Controlled model",
    ));

    let first_supervisor = Arc::clone(&supervisor);
    let first_work_id = work.summary.id.clone();
    let first =
        tokio::spawn(async move { first_supervisor.start(&first_work_id, "Build it").await });
    engine.wait_until_abort_is_paused().await;

    let overlap_error = supervisor
        .start(&work.summary.id, "Overlapping")
        .await
        .unwrap_err();
    assert_eq!(
        serde_json::to_value(overlap_error).unwrap()["code"],
        "work_already_running"
    );
    assert_eq!(engine.abort_calls(), 1);
    assert_eq!(engine.live_producers(), 1);

    engine.release_abort();
    let first_error = first.await.unwrap().unwrap_err();
    let after_first = harness
        .repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        serde_json::to_value(first_error).unwrap()["code"],
        "engine_start_failed"
    );
    assert_eq!(after_first.summary.status, WorkStatus::Failed);
    assert_eq!(after_first.runs[0].status, RunStatus::Failed);
    assert_authoritative_prompt(&after_first, "Build it", RunStatus::Failed);
    assert!(after_first.runs[0].completed_at.is_some());
    assert_eq!(engine.abort_calls(), 1);
    assert_eq!(engine.dropped_starts(), 1);
    wait_for_no_controlled_producers(&engine).await;

    let second_error = supervisor
        .start(&work.summary.id, "Try again")
        .await
        .unwrap_err();
    assert_eq!(
        serde_json::to_value(second_error).unwrap()["code"],
        "engine_start_failed"
    );
    assert_eq!(engine.abort_calls(), 2);
    wait_for_no_controlled_producers(&engine).await;
    let after_second = harness
        .repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();
    assert_authoritative_prompt(&after_second, "Build it", RunStatus::Failed);
    assert_authoritative_prompt(&after_second, "Try again", RunStatus::Failed);
}

#[tokio::test]
async fn start_failure_without_an_engine_resource_is_confirmed_safe() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Fail before resource creation").await;
    let engine = Arc::new(NoResourceStartFailingEngine::new());
    let (publisher, _published) = ChannelEventPublisher::channel(16);
    let supervisor = EngineSupervisor::new(
        harness.repository.clone(),
        engine.clone(),
        Arc::new(publisher),
        "No-resource model",
    );

    let first = supervisor
        .start(&work.summary.id, "First")
        .await
        .unwrap_err();
    let after_first = harness
        .repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();
    let retry = supervisor
        .start(&work.summary.id, "Retry")
        .await
        .unwrap_err();
    let after_retry = harness
        .repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();

    let first = serde_json::to_value(first).unwrap();
    assert_eq!(first["code"], "engine_start_failed");
    assert_eq!(
        first["details"]["reason"],
        "engine failed to start: failed before creating resources"
    );
    assert_eq!(after_first.summary.status, WorkStatus::Failed);
    assert_eq!(after_first.runs[0].status, RunStatus::Failed);
    assert_authoritative_prompt(&after_first, "First", RunStatus::Failed);
    assert_eq!(
        serde_json::to_value(retry).unwrap()["code"],
        "engine_start_failed"
    );
    assert_eq!(engine.abort_calls(), 2);
    assert_authoritative_prompt(&after_retry, "First", RunStatus::Failed);
    assert_authoritative_prompt(&after_retry, "Retry", RunStatus::Failed);
}

#[tokio::test]
async fn abort_panic_becomes_unconfirmed_without_stranding_waiters() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Recover abort panic").await;
    let engine = Arc::new(PanickingAbortEngine::new());
    let (publisher, _published) = ChannelEventPublisher::channel(16);
    let supervisor = EngineSupervisor::with_timeouts(
        harness.repository.clone(),
        engine.clone(),
        Arc::new(publisher),
        "Panicking abort model",
        std::time::Duration::from_secs(1),
        std::time::Duration::from_millis(100),
    );

    let first = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        supervisor.start(&work.summary.id, "First"),
    )
    .await
    .expect("abort panic stranded a termination waiter")
    .unwrap_err();
    let detail = harness
        .repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();
    let retry = supervisor
        .start(&work.summary.id, "Retry")
        .await
        .unwrap_err();

    assert_eq!(
        serde_json::to_value(first).unwrap()["code"],
        "engine_faulted"
    );
    assert_eq!(detail.summary.status, WorkStatus::Failed);
    assert_eq!(detail.runs[0].status, RunStatus::Failed);
    assert_authoritative_prompt(&detail, "First", RunStatus::Failed);
    assert_eq!(
        serde_json::to_value(retry).unwrap()["code"],
        "engine_faulted"
    );
    assert_eq!(engine.abort_calls(), 1);
}

#[derive(Default)]
struct EarlyClosingEngine {
    sink: tokio::sync::Mutex<Option<mpsc::Sender<EngineEvent>>>,
}

impl EarlyClosingEngine {
    async fn close_after_started_event(&self) {
        let sink = self.sink.lock().await.take().unwrap();
        sink.send(EngineEvent::RunStarted {
            model_label: "early-closing".into(),
        })
        .await
        .unwrap();
    }
}

#[async_trait]
impl EngineAdapter for EarlyClosingEngine {
    fn kind(&self) -> &'static str {
        "early-closing"
    }

    async fn start(
        &self,
        _context: EngineRunContext,
        _input: EngineInput,
        sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        *self.sink.lock().await = Some(sink);
        Ok(EngineSessionRef {
            engine_kind: self.kind().into(),
            session_id: "early-close-session".into(),
        })
    }

    async fn abort(&self, _run_id: &str) -> Result<(), EngineError> {
        Err(EngineError::NotRunning)
    }
}

#[tokio::test]
async fn channel_close_before_terminal_is_journaled_as_a_failed_run() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Close early").await;
    let engine = Arc::new(EarlyClosingEngine::default());
    let (publisher, mut published) = ChannelEventPublisher::channel(16);
    let supervisor = EngineSupervisor::new(
        harness.repository.clone(),
        engine.clone(),
        Arc::new(publisher),
        "Early close model",
    );

    let run = supervisor
        .start(&work.summary.id, "Build it")
        .await
        .unwrap();
    engine.close_after_started_event().await;
    let first = published.recv().await.unwrap();
    let failed = tokio::time::timeout(std::time::Duration::from_secs(1), published.recv())
        .await
        .expect("supervisor did not publish a synthetic failure")
        .unwrap();
    let detail = harness
        .repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(first.sequence, 1);
    assert_eq!(failed.sequence, 2);
    assert!(matches!(failed.payload, WorkEventPayload::RunFailed { .. }));
    assert_eq!(detail.summary.status, WorkStatus::Failed);
    assert_eq!(detail.runs[0].id, run.id);
    assert_eq!(detail.runs[0].status, RunStatus::Failed);

    supervisor
        .start(&work.summary.id, "Retry after natural stop")
        .await
        .expect("a naturally stopped engine left the active slot faulted");
    engine.close_after_started_event().await;
    receive_complete_run(&mut published).await;
}

#[tokio::test]
async fn fake_run_completes_with_the_stable_result_payload_and_terminal_state() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Complete cleanly").await;
    let (publisher, mut published) = ChannelEventPublisher::channel(16);
    let supervisor = EngineSupervisor::new(
        harness.repository.clone(),
        Arc::new(FakeEngineAdapter::new(std::time::Duration::ZERO)),
        Arc::new(publisher),
        "Fake model",
    );

    let run = supervisor
        .start(&work.summary.id, "Build it")
        .await
        .unwrap();
    let events = receive_complete_run(&mut published).await;
    let detail = harness
        .repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(events.len(), 8);
    assert_eq!(
        events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5, 6, 7, 8]
    );
    assert!(matches!(
        &events[7].payload,
        WorkEventPayload::RunCompleted {
            summary,
            artifacts,
            validation,
            limitations,
        } if summary == "Completed by the deterministic fake engine"
            && artifacts.is_empty()
            && validation == &["fake validation passed"]
            && limitations == &["fake engine only"]
    ));
    assert_eq!(detail.summary.status, WorkStatus::Completed);
    assert_eq!(detail.runs[0].id, run.id);
    assert_eq!(detail.runs[0].status, RunStatus::Completed);
    assert!(detail.runs[0].completed_at.is_some());
}

#[tokio::test]
async fn a_work_rejects_a_second_active_run() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Only one active Run").await;
    let (publisher, _published) = ChannelEventPublisher::channel(16);
    let supervisor = EngineSupervisor::new(
        harness.repository,
        Arc::new(FakeEngineAdapter::new(std::time::Duration::from_millis(
            200,
        ))),
        Arc::new(publisher),
        "Fake model",
    );

    supervisor.start(&work.summary.id, "First").await.unwrap();
    let error = supervisor
        .start(&work.summary.id, "Second")
        .await
        .unwrap_err();

    assert_eq!(
        serde_json::to_value(error).unwrap(),
        serde_json::json!({
            "code": "work_already_running",
            "message": "Work already has an active Run",
            "details": { "workId": work.summary.id }
        })
    );
}

#[tokio::test]
async fn completed_work_can_start_a_second_run_with_fresh_sequence_numbers() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Run twice").await;
    let (publisher, mut published) = ChannelEventPublisher::channel(32);
    let supervisor = EngineSupervisor::new(
        harness.repository.clone(),
        Arc::new(FakeEngineAdapter::new(std::time::Duration::ZERO)),
        Arc::new(publisher),
        "Fake model",
    );

    let first = supervisor.start(&work.summary.id, "First").await.unwrap();
    let first_events = receive_complete_run(&mut published).await;
    let second = supervisor.start(&work.summary.id, "Second").await.unwrap();
    let second_events = receive_complete_run(&mut published).await;
    let detail = harness
        .repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();

    assert_ne!(first.id, second.id);
    assert_eq!(first_events[0].sequence, 1);
    assert_eq!(second_events[0].sequence, 1);
    assert_eq!(detail.runs.len(), 2);
    assert!(
        detail
            .runs
            .iter()
            .all(|run| run.status == RunStatus::Completed)
    );
}

#[tokio::test]
async fn begin_run_returns_the_authoritative_user_message_and_get_replays_it() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Persist prompt").await;
    assert!(work.messages.is_empty());

    let started = harness
        .repository
        .begin_run(
            &work.summary.id,
            "  Keep this user instruction  ",
            &[],
            "fake",
            "Fake model",
        )
        .await
        .unwrap();
    let detail = harness
        .repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(started.user_message.work_id, work.summary.id);
    assert_eq!(started.user_message.run_id, started.run.id);
    assert_eq!(started.user_message.role, MessageRole::User);
    assert_eq!(started.user_message.content, "Keep this user instruction");
    assert_eq!(detail.messages, vec![started.user_message]);
}

#[tokio::test]
async fn two_run_prompts_survive_database_reopen_in_stable_order() {
    let temporary_directory = tempfile::tempdir().unwrap();
    let database_path = temporary_directory.path().join("messages.sqlite3");
    let workspace_path = temporary_directory.path().join("workspace");
    std::fs::create_dir(&workspace_path).unwrap();
    let database = Database::open(&database_path).await.unwrap();
    let repository = WorkRepository::new(database.pool().clone());
    let work = repository
        .create(CreateWorkInput {
            title: "Two prompts".into(),
            goal: "Persist user instructions".into(),
            root_path: workspace_path.to_string_lossy().into_owned(),
            permission_mode: PermissionMode::Balanced,
            resource_draft_id: None,
        })
        .await
        .unwrap();

    let first = repository
        .begin_run(&work.summary.id, "First prompt", &[], "fake", "Fake model")
        .await
        .unwrap();
    repository
        .append_event_and_transition(&WorkEventEnvelope {
            version: 1,
            event_id: Some(Uuid::new_v4().to_string()),
            work_id: work.summary.id.clone(),
            run_id: first.run.id.clone(),
            turn_id: None,
            session_id: None,
            agent_id: None,
            assignment_id: None,
            causation_id: None,
            correlation_id: None,
            sequence: 1,
            occurred_at: chrono::Utc::now(),
            payload: WorkEventPayload::RunCompleted {
                summary: "First run finished without repeating the prompt".into(),
                artifacts: Vec::new(),
                validation: Vec::new(),
                limitations: Vec::new(),
            },
        })
        .await
        .unwrap();
    let second = repository
        .begin_run(&work.summary.id, "Second prompt", &[], "fake", "Fake model")
        .await
        .unwrap();
    drop(repository);
    drop(database);

    let reopened = Database::open(&database_path).await.unwrap();
    let detail = WorkRepository::new(reopened.pool().clone())
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        detail
            .messages
            .iter()
            .map(|message| (&message.run_id, message.content.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (&first.run.id, "First prompt"),
            (&second.run.id, "Second prompt"),
        ]
    );
}

#[tokio::test]
async fn engine_execution_identity_survives_database_reopen() {
    let temporary_directory = tempfile::tempdir().unwrap();
    let database_path = temporary_directory.path().join("identity.sqlite3");
    let workspace_path = temporary_directory.path().join("workspace");
    std::fs::create_dir(&workspace_path).unwrap();
    let database = Database::open(&database_path).await.unwrap();
    let repository = WorkRepository::new(database.pool().clone());
    let work = repository
        .create(CreateWorkInput {
            title: "Execution identity".into(),
            goal: "Recover the engine session".into(),
            root_path: workspace_path.to_string_lossy().into_owned(),
            permission_mode: PermissionMode::Balanced,
            resource_draft_id: None,
        })
        .await
        .unwrap();
    let (publisher, mut published) = ChannelEventPublisher::channel(16);
    let supervisor = EngineSupervisor::new(
        repository.clone(),
        Arc::new(FakeEngineAdapter::new(std::time::Duration::ZERO)),
        Arc::new(publisher),
        "Fake model",
    );
    let started = supervisor
        .start(&work.summary.id, "Build it")
        .await
        .unwrap();
    receive_complete_run(&mut published).await;
    drop(supervisor);
    drop(repository);
    drop(database);

    let reopened = Database::open(&database_path).await.unwrap();
    let detail = WorkRepository::new(reopened.pool().clone())
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(detail.runs[0].id, started.id);
    assert_eq!(detail.runs[0].engine_kind, "fake");
    assert_eq!(detail.runs[0].engine_session_id, started.engine_session_id);
    assert!(detail.runs[0].engine_session_id.is_some());
    assert_eq!(detail.runs[0].model_label, "Fake model");
}

#[tokio::test]
async fn start_rejects_an_empty_prompt_and_an_archived_work() {
    let harness = TestHarness::new().await;
    let empty_work = harness.create_work("Empty prompt").await;
    let archived_work = harness.create_work("Archived work").await;
    harness
        .repository
        .set_work_status(&archived_work.summary.id, WorkStatus::Archived)
        .await
        .unwrap();
    let (publisher, _published) = ChannelEventPublisher::channel(16);
    let supervisor = EngineSupervisor::new(
        harness.repository.clone(),
        Arc::new(FakeEngineAdapter::new(std::time::Duration::ZERO)),
        Arc::new(publisher),
        "Fake model",
    );

    let empty_error = supervisor
        .start(&empty_work.summary.id, " \n ")
        .await
        .unwrap_err();
    let archived_error = supervisor
        .start(&archived_work.summary.id, "Build it")
        .await
        .unwrap_err();

    assert_eq!(
        serde_json::to_value(empty_error).unwrap()["code"],
        "invalid_input"
    );
    assert_eq!(
        serde_json::to_value(archived_error).unwrap()["code"],
        "invalid_work_state"
    );
    assert!(
        harness
            .repository
            .get(&empty_work.summary.id)
            .await
            .unwrap()
            .unwrap()
            .runs
            .is_empty()
    );
}

#[tokio::test]
async fn referenced_files_expand_only_the_engine_prompt() {
    let harness = TestHarness::new().await;
    std::fs::write(harness.workspace_path.join("context.md"), "current context").unwrap();
    let work = harness.create_work("Referenced context").await;
    let engine = Arc::new(PromptRecordingEngine::default());
    let (publisher, _published) = ChannelEventPublisher::channel(16);
    let supervisor = Arc::new(EngineSupervisor::new(
        harness.repository.clone(),
        engine.clone(),
        Arc::new(publisher),
        "Recording model",
    ));
    let service = WorkService::with_supervisor(harness.repository.clone(), supervisor);

    let output = service
        .start_work(
            &work.summary.id,
            StartWorkInput {
                prompt: "Review @{context.md}".into(),
                referenced_files: vec!["context.md".into()],
                resource_ids: Vec::new(),
            },
        )
        .await
        .unwrap();

    assert_eq!(output.user_message.content, "Review @{context.md}");
    let engine_prompt = engine.prompt.lock().unwrap().clone().unwrap();
    assert!(engine_prompt.contains("current context"));
    assert!(engine_prompt.contains("path=\"context.md\""));
}

#[tokio::test]
async fn publisher_failure_does_not_stop_the_persisted_event_stream() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Ignore publish failure").await;
    let (publisher, published) = ChannelEventPublisher::channel(1);
    drop(published);
    let supervisor = EngineSupervisor::new(
        harness.repository.clone(),
        Arc::new(FakeEngineAdapter::new(std::time::Duration::ZERO)),
        Arc::new(publisher),
        "Fake model",
    );

    let run = supervisor
        .start(&work.summary.id, "Build it")
        .await
        .unwrap();
    let events = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let events = harness.repository.events_for_run(&run.id).await.unwrap();
            if events.len() == 8 {
                return events;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("publisher failure stopped event persistence");
    let detail = harness
        .repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(events.last().unwrap().sequence, 8);
    assert_eq!(detail.summary.status, WorkStatus::Completed);
    assert_eq!(detail.runs[0].status, RunStatus::Completed);
}

#[tokio::test]
async fn append_failure_is_finalized_as_a_durable_run_failed_event() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Finalize append failure").await;
    let engine = ControlledEngine::new(ControlledStartBehavior::Immediate);
    let (publisher, mut published) = ChannelEventPublisher::channel(16);
    let supervisor = EngineSupervisor::new(
        harness.repository.clone(),
        Arc::new(engine.clone()),
        Arc::new(publisher),
        "Controlled model",
    );
    let run = supervisor
        .start(&work.summary.id, "Build it")
        .await
        .unwrap();
    let preexisting = WorkEventEnvelope {
        version: 1,
        event_id: Some(Uuid::new_v4().to_string()),
        work_id: work.summary.id.clone(),
        run_id: run.id.clone(),
        turn_id: None,
        session_id: None,
        agent_id: None,
        assignment_id: None,
        causation_id: None,
        correlation_id: None,
        sequence: 1,
        occurred_at: chrono::Utc::now(),
        payload: WorkEventPayload::AssistantDelta {
            text: "reserved durable sequence".into(),
        },
    };
    harness
        .repository
        .append_event_and_transition(&preexisting)
        .await
        .unwrap();

    engine.send(EngineEvent::AssistantDelta {
        text: "engine sequence one".into(),
    });
    let failed = tokio::time::timeout(std::time::Duration::from_secs(1), published.recv())
        .await
        .expect("consumer did not publish its finalized failure")
        .unwrap();
    let events = harness.repository.events_for_run(&run.id).await.unwrap();

    assert_eq!(failed.sequence, 2);
    assert!(matches!(failed.payload, WorkEventPayload::RunFailed { .. }));
    assert_eq!(events, vec![preexisting, failed]);
    assert_eq!(engine.abort_calls(), 1);
    wait_for_no_controlled_producers(&engine).await;
}

#[tokio::test]
async fn attach_failure_aborts_idle_engine_once_and_keeps_the_active_slot_faulted() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Keep faulted slot").await;
    let engine = ControlledEngine::new(ControlledStartBehavior::Gated);
    let (publisher, _published) = ChannelEventPublisher::channel(16);
    let supervisor = Arc::new(EngineSupervisor::with_timeouts(
        harness.repository.clone(),
        Arc::new(engine.clone()),
        Arc::new(publisher),
        "Controlled model",
        std::time::Duration::from_secs(1),
        std::time::Duration::from_millis(200),
    ));
    let start_supervisor = Arc::clone(&supervisor);
    let start_work_id = work.summary.id.clone();
    let start =
        tokio::spawn(async move { start_supervisor.start(&start_work_id, "Build it").await });
    engine.wait_started().await;
    sqlx::query("PRAGMA query_only = ON")
        .execute(&harness.pool)
        .await
        .unwrap();
    engine.release_start();

    let start_error = tokio::time::timeout(std::time::Duration::from_secs(1), start)
        .await
        .expect("attach failure did not stop the idle engine promptly")
        .unwrap()
        .unwrap_err();
    assert_eq!(
        serde_json::to_value(start_error).unwrap()["code"],
        "engine_faulted"
    );
    assert_eq!(engine.abort_calls(), 1);
    wait_for_no_controlled_producers(&engine).await;

    let error = supervisor
        .start(&work.summary.id, "Try again")
        .await
        .unwrap_err();
    sqlx::query("PRAGMA query_only = OFF")
        .execute(&harness.pool)
        .await
        .unwrap();
    let detail = harness
        .repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        serde_json::to_value(error).unwrap(),
        serde_json::json!({
            "code": "engine_faulted",
            "message": "Work engine lifecycle is faulted",
            "details": { "workId": work.summary.id }
        })
    );
    assert_eq!(detail.summary.status, WorkStatus::Running);
    assert_authoritative_prompt(&detail, "Build it", RunStatus::Running);
}

#[tokio::test]
async fn publisher_panic_aborts_the_engine_once_and_gets_a_durable_failure_fallback() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Recover consumer panic").await;
    let engine = ControlledEngine::new(ControlledStartBehavior::Immediate);
    let supervisor = EngineSupervisor::new(
        harness.repository.clone(),
        Arc::new(engine.clone()),
        Arc::new(PanickingPublisher::new()),
        "Controlled model",
    );
    let run = supervisor
        .start(&work.summary.id, "Build it")
        .await
        .unwrap();
    engine.send(EngineEvent::AssistantDelta {
        text: "panic while publishing this event".into(),
    });

    let detail = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let detail = harness
                .repository
                .get(&work.summary.id)
                .await
                .unwrap()
                .unwrap();
            if detail.summary.status == WorkStatus::Failed {
                return detail;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("consumer panic left the durable Run active");

    assert_eq!(detail.runs[0].status, RunStatus::Failed);
    assert!(detail.events.iter().any(|event| {
        event.run_id == run.id && matches!(event.payload, WorkEventPayload::RunFailed { .. })
    }));
    assert_eq!(engine.abort_calls(), 1);
    wait_for_no_controlled_producers(&engine).await;
}

#[tokio::test]
async fn repository_rejects_late_events_before_payload_kind_matters() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Terminal is last").await;
    let run = harness
        .repository
        .begin_run(&work.summary.id, "Build it", &[], "fake", "Fake model")
        .await
        .unwrap();
    let completed = WorkEventEnvelope {
        version: 1,
        event_id: Some(Uuid::new_v4().to_string()),
        work_id: work.summary.id.clone(),
        run_id: run.id.clone(),
        turn_id: None,
        session_id: None,
        agent_id: None,
        assignment_id: None,
        causation_id: None,
        correlation_id: None,
        sequence: 1,
        occurred_at: chrono::Utc::now(),
        payload: WorkEventPayload::RunCompleted {
            summary: "done".into(),
            artifacts: Vec::new(),
            validation: Vec::new(),
            limitations: Vec::new(),
        },
    };
    harness
        .repository
        .append_event_and_transition(&completed)
        .await
        .unwrap();
    let representative_late_payloads = [
        WorkEventPayload::ThoughtDelta {
            text: "too late".into(),
        },
        WorkEventPayload::RunFailed {
            message: "late failure".into(),
        },
    ];
    for payload in representative_late_payloads {
        let late = WorkEventEnvelope {
            version: 1,
            event_id: Some(Uuid::new_v4().to_string()),
            work_id: work.summary.id.clone(),
            run_id: run.id.clone(),
            turn_id: None,
            session_id: None,
            agent_id: None,
            assignment_id: None,
            causation_id: None,
            correlation_id: None,
            sequence: 2,
            occurred_at: chrono::Utc::now(),
            payload,
        };
        let error = harness
            .repository
            .append_event_and_transition(&late)
            .await
            .unwrap_err();
        assert_eq!(
            serde_json::to_value(error).unwrap()["code"],
            "invalid_work_state"
        );
    }
    let events = harness.repository.events_for_run(&run.id).await.unwrap();

    assert_eq!(events, vec![completed]);
}

#[tokio::test]
async fn created_work_survives_database_reopen() {
    let temporary_directory = tempfile::tempdir().unwrap();
    let database_path = temporary_directory.path().join("piwork.sqlite3");
    let workspace_path = temporary_directory.path().join("workspace");
    std::fs::create_dir(&workspace_path).unwrap();

    let database = Database::open(&database_path).await.unwrap();
    let repository = WorkRepository::new(database.pool().clone());
    let created = repository
        .create(CreateWorkInput {
            title: "  Ship PiWork  ".into(),
            goal: "  Build the foundation  ".into(),
            root_path: workspace_path.to_string_lossy().into_owned(),
            permission_mode: PermissionMode::Balanced,
            resource_draft_id: None,
        })
        .await
        .unwrap();

    assert_eq!(created.summary.title, "Ship PiWork");
    assert_eq!(created.summary.goal, "Build the foundation");
    assert_eq!(created.summary.status, WorkStatus::Draft);
    assert_eq!(
        created.summary.root_path,
        dunce::canonicalize(&workspace_path)
            .unwrap()
            .to_string_lossy()
    );

    let work_id = created.summary.id.clone();
    drop(repository);
    drop(database);

    let reopened_database = Database::open(&database_path).await.unwrap();
    let reopened_repository = WorkRepository::new(reopened_database.pool().clone());
    let found = reopened_repository.get(&work_id).await.unwrap().unwrap();

    assert_eq!(found.summary, created.summary);
    assert!(found.runs.is_empty());
    assert!(found.events.is_empty());
}

#[tokio::test]
async fn create_rejects_blank_title_and_goal() {
    let temporary_directory = tempfile::tempdir().unwrap();
    let workspace_path = temporary_directory.path().join("workspace");
    std::fs::create_dir(&workspace_path).unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let repository = WorkRepository::new(database.pool().clone());

    for (field, title, goal, expected_message) in [
        ("title", " \t ", "Goal", "title must not be empty"),
        ("goal", "Title", "\n ", "goal must not be empty"),
    ] {
        let error = repository
            .create(CreateWorkInput {
                title: title.into(),
                goal: goal.into(),
                root_path: workspace_path.to_string_lossy().into_owned(),
                permission_mode: PermissionMode::Balanced,
                resource_draft_id: None,
            })
            .await
            .unwrap_err();

        assert_eq!(
            serde_json::to_value(error).unwrap(),
            serde_json::json!({
                "code": "invalid_input",
                "message": expected_message,
                "details": { "field": field }
            })
        );
    }
}

#[tokio::test]
async fn create_rejects_a_missing_workspace_path() {
    let temporary_directory = tempfile::tempdir().unwrap();
    let missing_path = temporary_directory.path().join("does-not-exist");
    let database = Database::open_in_memory().await.unwrap();
    let repository = WorkRepository::new(database.pool().clone());

    let error = repository
        .create(CreateWorkInput {
            title: "Title".into(),
            goal: "Goal".into(),
            root_path: missing_path.to_string_lossy().into_owned(),
            permission_mode: PermissionMode::Balanced,
            resource_draft_id: None,
        })
        .await
        .unwrap_err();

    assert_eq!(
        serde_json::to_value(error).unwrap(),
        serde_json::json!({
            "code": "path_resolution_error",
            "message": "Workspace path could not be resolved",
            "details": {
                "field": "rootPath",
                "path": missing_path.to_string_lossy()
            }
        })
    );
}

#[tokio::test]
async fn create_rejects_a_regular_file_as_the_workspace_root() {
    let temporary_directory = tempfile::tempdir().unwrap();
    let file_path = temporary_directory.path().join("not-a-directory.txt");
    std::fs::write(&file_path, "content").unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let repository = WorkRepository::new(database.pool().clone());

    let error = repository
        .create(CreateWorkInput {
            title: "Title".into(),
            goal: "Goal".into(),
            root_path: file_path.to_string_lossy().into_owned(),
            permission_mode: PermissionMode::Balanced,
            resource_draft_id: None,
        })
        .await
        .unwrap_err();

    assert_eq!(
        serde_json::to_value(error).unwrap(),
        serde_json::json!({
            "code": "invalid_input",
            "message": "rootPath must be a directory",
            "details": { "field": "rootPath" }
        })
    );
    assert!(repository.list().await.unwrap().is_empty());
}

#[tokio::test]
async fn list_orders_works_by_most_recent_update() {
    let temporary_directory = tempfile::tempdir().unwrap();
    let workspace_path = temporary_directory.path().join("workspace");
    std::fs::create_dir(&workspace_path).unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let repository = WorkRepository::new(database.pool().clone());

    let older = repository
        .create(CreateWorkInput {
            title: "Older".into(),
            goal: "Goal".into(),
            root_path: workspace_path.to_string_lossy().into_owned(),
            permission_mode: PermissionMode::Balanced,
            resource_draft_id: None,
        })
        .await
        .unwrap();
    let newer = repository
        .create(CreateWorkInput {
            title: "Newer".into(),
            goal: "Goal".into(),
            root_path: workspace_path.to_string_lossy().into_owned(),
            permission_mode: PermissionMode::Balanced,
            resource_draft_id: None,
        })
        .await
        .unwrap();

    sqlx::query("UPDATE works SET updated_at = ? WHERE id = ?")
        .bind("2026-01-01T00:00:00Z")
        .bind(&older.summary.id)
        .execute(database.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE works SET updated_at = ? WHERE id = ?")
        .bind("2026-01-02T00:00:00Z")
        .bind(&newer.summary.id)
        .execute(database.pool())
        .await
        .unwrap();

    let listed = repository.list().await.unwrap();

    assert_eq!(
        listed
            .iter()
            .map(|work| work.id.as_str())
            .collect::<Vec<_>>(),
        vec![newer.summary.id.as_str(), older.summary.id.as_str()]
    );
}

#[tokio::test]
async fn inserted_runs_are_returned_in_creation_order() {
    let temporary_directory = tempfile::tempdir().unwrap();
    let workspace_path = temporary_directory.path().join("workspace");
    std::fs::create_dir(&workspace_path).unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let repository = WorkRepository::new(database.pool().clone());
    let work = repository
        .create(CreateWorkInput {
            title: "Runs".into(),
            goal: "Keep history".into(),
            root_path: workspace_path.to_string_lossy().into_owned(),
            permission_mode: PermissionMode::Balanced,
            resource_draft_id: None,
        })
        .await
        .unwrap();

    let later = repository
        .insert_run(&work.summary.id, "later-model")
        .await
        .unwrap();
    let earlier = repository
        .insert_run(&work.summary.id, "earlier-model")
        .await
        .unwrap();
    assert_eq!(later.status, RunStatus::Queued);
    assert_eq!(earlier.status, RunStatus::Queued);

    sqlx::query("UPDATE runs SET created_at = ? WHERE id = ?")
        .bind("2026-01-02T00:00:00Z")
        .bind(&later.id)
        .execute(database.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE runs SET created_at = ? WHERE id = ?")
        .bind("2026-01-01T00:00:00Z")
        .bind(&earlier.id)
        .execute(database.pool())
        .await
        .unwrap();

    let found = repository.get(&work.summary.id).await.unwrap().unwrap();

    assert_eq!(
        found
            .runs
            .iter()
            .map(|run| run.model_label.as_str())
            .collect::<Vec<_>>(),
        vec!["earlier-model", "later-model"]
    );
    assert!(found.summary.updated_at >= work.summary.updated_at);
}

#[tokio::test]
async fn get_orders_and_decodes_events_by_run_then_sequence() {
    let temporary_directory = tempfile::tempdir().unwrap();
    let workspace_path = temporary_directory.path().join("workspace");
    std::fs::create_dir(&workspace_path).unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let repository = WorkRepository::new(database.pool().clone());
    let work = repository
        .create(CreateWorkInput {
            title: "Events".into(),
            goal: "Replay history".into(),
            root_path: workspace_path.to_string_lossy().into_owned(),
            permission_mode: PermissionMode::Balanced,
            resource_draft_id: None,
        })
        .await
        .unwrap();
    let first_run = repository
        .insert_run(&work.summary.id, "first-model")
        .await
        .unwrap();
    let second_run = repository
        .insert_run(&work.summary.id, "second-model")
        .await
        .unwrap();
    sqlx::query("UPDATE runs SET created_at = ? WHERE id = ?")
        .bind("2026-01-01T00:00:00Z")
        .bind(&first_run.id)
        .execute(database.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE runs SET created_at = ? WHERE id = ?")
        .bind("2026-01-02T00:00:00Z")
        .bind(&second_run.id)
        .execute(database.pool())
        .await
        .unwrap();

    for (id, run_id, sequence, text) in [
        ("event-2", first_run.id.as_str(), 2_i64, "second"),
        ("event-1", first_run.id.as_str(), 1_i64, "first"),
        ("event-3", second_run.id.as_str(), 1_i64, "third"),
    ] {
        let payload = serde_json::json!({ "type": "assistantDelta", "text": text });
        sqlx::query(
            "INSERT INTO events \
             (id, work_id, run_id, sequence, version, occurred_at, payload) \
             VALUES (?, ?, ?, ?, 1, ?, ?)",
        )
        .bind(id)
        .bind(&work.summary.id)
        .bind(run_id)
        .bind(sequence)
        .bind("2026-01-01T00:00:00Z")
        .bind(payload.to_string())
        .execute(database.pool())
        .await
        .unwrap();
    }

    let found = repository.get(&work.summary.id).await.unwrap().unwrap();

    assert_eq!(
        found
            .events
            .iter()
            .map(|event| (event.run_id.as_str(), event.sequence))
            .collect::<Vec<_>>(),
        vec![
            (first_run.id.as_str(), 1),
            (first_run.id.as_str(), 2),
            (second_run.id.as_str(), 1),
        ]
    );
    assert_eq!(
        found.events[0].payload,
        WorkEventPayload::AssistantDelta {
            text: "first".into()
        }
    );
}

#[tokio::test]
async fn setting_work_status_persists_and_updates_the_timestamp() {
    let temporary_directory = tempfile::tempdir().unwrap();
    let workspace_path = temporary_directory.path().join("workspace");
    std::fs::create_dir(&workspace_path).unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let repository = WorkRepository::new(database.pool().clone());
    let work = repository
        .create(CreateWorkInput {
            title: "Status".into(),
            goal: "Persist state".into(),
            root_path: workspace_path.to_string_lossy().into_owned(),
            permission_mode: PermissionMode::Balanced,
            resource_draft_id: None,
        })
        .await
        .unwrap();

    repository
        .set_work_status(&work.summary.id, WorkStatus::Archived)
        .await
        .unwrap();
    let found = repository.get(&work.summary.id).await.unwrap().unwrap();

    assert_eq!(found.summary.status, WorkStatus::Archived);
    assert!(found.summary.updated_at >= work.summary.updated_at);
}

#[tokio::test]
async fn setting_run_status_tracks_start_and_completion_times() {
    let temporary_directory = tempfile::tempdir().unwrap();
    let workspace_path = temporary_directory.path().join("workspace");
    std::fs::create_dir(&workspace_path).unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let repository = WorkRepository::new(database.pool().clone());
    let work = repository
        .create(CreateWorkInput {
            title: "Run status".into(),
            goal: "Track execution".into(),
            root_path: workspace_path.to_string_lossy().into_owned(),
            permission_mode: PermissionMode::Balanced,
            resource_draft_id: None,
        })
        .await
        .unwrap();
    let run = repository
        .insert_run(&work.summary.id, "model")
        .await
        .unwrap();

    repository
        .set_run_status(&run.id, RunStatus::Running)
        .await
        .unwrap();
    let running = repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap()
        .runs[0]
        .clone();
    assert_eq!(running.status, RunStatus::Running);
    assert!(running.started_at.is_some());
    assert!(running.completed_at.is_none());

    repository
        .set_run_status(&run.id, RunStatus::Completed)
        .await
        .unwrap();
    let completed = repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap()
        .runs[0]
        .clone();
    assert_eq!(completed.status, RunStatus::Completed);
    assert_eq!(completed.started_at, running.started_at);
    assert!(completed.completed_at.is_some());
}

#[tokio::test]
async fn archived_work_rejects_running_without_changing_persisted_state() {
    let temporary_directory = tempfile::tempdir().unwrap();
    let workspace_path = temporary_directory.path().join("workspace");
    std::fs::create_dir(&workspace_path).unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let repository = WorkRepository::new(database.pool().clone());
    let work = repository
        .create(CreateWorkInput {
            title: "Archived".into(),
            goal: "Stay archived".into(),
            root_path: workspace_path.to_string_lossy().into_owned(),
            permission_mode: PermissionMode::Balanced,
            resource_draft_id: None,
        })
        .await
        .unwrap();
    repository
        .set_work_status(&work.summary.id, WorkStatus::Archived)
        .await
        .unwrap();
    let before = repository.get(&work.summary.id).await.unwrap().unwrap();

    let error = repository
        .set_work_status(&work.summary.id, WorkStatus::Running)
        .await
        .unwrap_err();

    assert_eq!(
        serde_json::to_value(error).unwrap(),
        serde_json::json!({
            "code": "invalid_work_state",
            "message": "Work status transition is invalid",
            "details": {
                "workId": work.summary.id,
                "from": "archived",
                "to": "running"
            }
        })
    );
    let after = repository.get(&work.summary.id).await.unwrap().unwrap();
    assert_eq!(after.summary, before.summary);
}

#[tokio::test]
async fn completed_run_rejects_running_without_changing_persisted_state() {
    let temporary_directory = tempfile::tempdir().unwrap();
    let workspace_path = temporary_directory.path().join("workspace");
    std::fs::create_dir(&workspace_path).unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let repository = WorkRepository::new(database.pool().clone());
    let work = repository
        .create(CreateWorkInput {
            title: "Completed run".into(),
            goal: "Stay completed".into(),
            root_path: workspace_path.to_string_lossy().into_owned(),
            permission_mode: PermissionMode::Balanced,
            resource_draft_id: None,
        })
        .await
        .unwrap();
    let run = repository
        .insert_run(&work.summary.id, "model")
        .await
        .unwrap();
    repository
        .set_run_status(&run.id, RunStatus::Running)
        .await
        .unwrap();
    repository
        .set_run_status(&run.id, RunStatus::Completed)
        .await
        .unwrap();
    let before = repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap()
        .runs[0]
        .clone();

    let error = repository
        .set_run_status(&run.id, RunStatus::Running)
        .await
        .unwrap_err();

    assert_eq!(
        serde_json::to_value(error).unwrap(),
        serde_json::json!({
            "code": "invalid_work_state",
            "message": "Run status transition is invalid",
            "details": {
                "runId": run.id,
                "from": "completed",
                "to": "running"
            }
        })
    );
    let after = repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap()
        .runs[0]
        .clone();
    assert_eq!(after, before);
}

#[tokio::test]
async fn service_creates_lists_and_gets_work_details() {
    let temporary_directory = tempfile::tempdir().unwrap();
    let workspace_path = temporary_directory.path().join("workspace");
    std::fs::create_dir(&workspace_path).unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let service = WorkService::new(WorkRepository::new(database.pool().clone()));

    let created = service
        .create_work(CreateWorkInput {
            title: "Service".into(),
            goal: "Expose work".into(),
            root_path: workspace_path.to_string_lossy().into_owned(),
            permission_mode: PermissionMode::Balanced,
            resource_draft_id: None,
        })
        .await
        .unwrap();
    let listed = service.list_works().await.unwrap();
    let found = service.get_work(&created.summary.id).await.unwrap();

    assert_eq!(listed, vec![created.summary.clone()]);
    assert_eq!(found, created);
}

#[tokio::test]
async fn startup_marks_unfinished_runs_interrupted_without_resuming() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Recover interrupted run").await;
    let run = harness
        .repository
        .insert_run(&work.summary.id, "Fake model")
        .await
        .unwrap();
    harness
        .repository
        .set_work_status(&work.summary.id, WorkStatus::Queued)
        .await
        .unwrap();
    harness
        .repository
        .set_work_status(&work.summary.id, WorkStatus::Running)
        .await
        .unwrap();
    harness
        .repository
        .set_run_status(&run.id, RunStatus::Running)
        .await
        .unwrap();
    let engine = Arc::new(StartCountingEngine::new());
    let (publisher, _published) = ChannelEventPublisher::channel(1);
    let supervisor = Arc::new(EngineSupervisor::new(
        harness.repository.clone(),
        engine.clone(),
        Arc::new(publisher),
        "Fake model",
    ));
    let service = WorkService::with_supervisor(harness.repository.clone(), supervisor);

    let recovered = service.recover_interrupted_runs().await.unwrap();

    let recovered_work = harness
        .repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered, 1);
    assert_eq!(recovered_work.summary.status, WorkStatus::Interrupted);
    assert_eq!(recovered_work.runs[0].status, RunStatus::Interrupted);
    assert_eq!(engine.start_count(), 0);
}

#[tokio::test]
async fn startup_recovers_queued_and_waiting_runs_across_works_without_changing_terminal_runs() {
    let harness = TestHarness::new().await;
    let queued_work = harness.create_work("Recover queued run").await;
    let queued_run = harness
        .repository
        .insert_run(&queued_work.summary.id, "Fake model")
        .await
        .unwrap();
    harness
        .repository
        .set_work_status(&queued_work.summary.id, WorkStatus::Queued)
        .await
        .unwrap();

    let waiting_work = harness.create_work("Recover waiting run").await;
    let waiting_run = harness
        .repository
        .insert_run(&waiting_work.summary.id, "Fake model")
        .await
        .unwrap();
    harness
        .repository
        .set_work_status(&waiting_work.summary.id, WorkStatus::Queued)
        .await
        .unwrap();
    harness
        .repository
        .set_work_status(&waiting_work.summary.id, WorkStatus::Running)
        .await
        .unwrap();
    harness
        .repository
        .set_work_status(&waiting_work.summary.id, WorkStatus::Waiting)
        .await
        .unwrap();
    harness
        .repository
        .set_run_status(&waiting_run.id, RunStatus::Running)
        .await
        .unwrap();
    harness
        .repository
        .set_run_status(&waiting_run.id, RunStatus::Waiting)
        .await
        .unwrap();

    let completed_work = harness.create_work("Keep completed run").await;
    let completed_run = harness
        .repository
        .insert_run(&completed_work.summary.id, "Fake model")
        .await
        .unwrap();
    harness
        .repository
        .set_work_status(&completed_work.summary.id, WorkStatus::Queued)
        .await
        .unwrap();
    harness
        .repository
        .set_work_status(&completed_work.summary.id, WorkStatus::Running)
        .await
        .unwrap();
    harness
        .repository
        .set_work_status(&completed_work.summary.id, WorkStatus::Completed)
        .await
        .unwrap();
    harness
        .repository
        .set_run_status(&completed_run.id, RunStatus::Running)
        .await
        .unwrap();
    harness
        .repository
        .set_run_status(&completed_run.id, RunStatus::Completed)
        .await
        .unwrap();
    let terminal_before = harness
        .repository
        .get(&completed_work.summary.id)
        .await
        .unwrap()
        .unwrap();
    let service = WorkService::new(harness.repository.clone());

    let recovered = service.recover_interrupted_runs().await.unwrap();

    let queued_after = harness
        .repository
        .get(&queued_work.summary.id)
        .await
        .unwrap()
        .unwrap();
    let waiting_after = harness
        .repository
        .get(&waiting_work.summary.id)
        .await
        .unwrap()
        .unwrap();
    let terminal_after = harness
        .repository
        .get(&completed_work.summary.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered, 2);
    assert_eq!(queued_after.summary.status, WorkStatus::Interrupted);
    assert_eq!(queued_after.runs[0].id, queued_run.id);
    assert_eq!(queued_after.runs[0].status, RunStatus::Interrupted);
    assert_eq!(waiting_after.summary.status, WorkStatus::Interrupted);
    assert_eq!(waiting_after.runs[0].status, RunStatus::Interrupted);
    assert_eq!(terminal_after, terminal_before);
}

#[tokio::test]
async fn startup_recovery_rolls_back_work_changes_when_run_update_fails() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Rollback interrupted recovery").await;
    let run = harness
        .repository
        .insert_run(&work.summary.id, "Fake model")
        .await
        .unwrap();
    harness
        .repository
        .set_work_status(&work.summary.id, WorkStatus::Queued)
        .await
        .unwrap();
    harness
        .repository
        .set_work_status(&work.summary.id, WorkStatus::Running)
        .await
        .unwrap();
    harness
        .repository
        .set_run_status(&run.id, RunStatus::Running)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER reject_recovery_run_update \
         BEFORE UPDATE OF status ON runs \
         WHEN OLD.status = 'running' AND NEW.status = 'interrupted' \
         BEGIN SELECT RAISE(ABORT, 'injected recovery failure'); END",
    )
    .execute(&harness.pool)
    .await
    .unwrap();
    let service = WorkService::new(harness.repository.clone());

    assert!(service.recover_interrupted_runs().await.is_err());

    let after = harness
        .repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.summary.status, WorkStatus::Running);
    assert_eq!(after.runs[0].status, RunStatus::Running);
}

#[tokio::test]
async fn startup_recovery_leaves_every_terminal_run_and_work_unchanged() {
    let harness = TestHarness::new().await;
    let mut terminal_details = Vec::new();
    for (run_status, work_status) in [
        (RunStatus::Completed, WorkStatus::Completed),
        (RunStatus::Failed, WorkStatus::Failed),
        (RunStatus::Stopped, WorkStatus::Stopped),
        (RunStatus::Interrupted, WorkStatus::Interrupted),
    ] {
        let work = harness
            .create_work(&format!("Keep {run_status:?} run"))
            .await;
        let run = harness
            .repository
            .insert_run(&work.summary.id, "Fake model")
            .await
            .unwrap();
        harness
            .repository
            .set_work_status(&work.summary.id, WorkStatus::Queued)
            .await
            .unwrap();
        harness
            .repository
            .set_work_status(&work.summary.id, WorkStatus::Running)
            .await
            .unwrap();
        harness
            .repository
            .set_work_status(&work.summary.id, work_status)
            .await
            .unwrap();
        harness
            .repository
            .set_run_status(&run.id, RunStatus::Running)
            .await
            .unwrap();
        harness
            .repository
            .set_run_status(&run.id, run_status)
            .await
            .unwrap();
        terminal_details.push(
            harness
                .repository
                .get(&work.summary.id)
                .await
                .unwrap()
                .unwrap(),
        );
    }
    let service = WorkService::new(harness.repository.clone());

    assert_eq!(service.recover_interrupted_runs().await.unwrap(), 0);

    for before in terminal_details {
        let after = harness
            .repository
            .get(&before.summary.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after, before);
    }
}

#[tokio::test]
async fn service_get_returns_a_stable_not_found_error() {
    let database = Database::open_in_memory().await.unwrap();
    let service = WorkService::new(WorkRepository::new(database.pool().clone()));
    let missing_id = "00000000-0000-0000-0000-000000000000";

    let error = service.get_work(missing_id).await.unwrap_err();

    assert_eq!(
        serde_json::to_value(error).unwrap(),
        serde_json::json!({
            "code": "not_found",
            "message": "Work not found",
            "details": { "workId": missing_id }
        })
    );
}

#[tokio::test]
async fn app_state_exposes_the_shared_work_service() {
    let database = Database::open_in_memory().await.unwrap();
    let service = Arc::new(WorkService::new(WorkRepository::new(
        database.pool().clone(),
    )));
    let state = AppState::new(Arc::clone(&service));

    assert!(Arc::ptr_eq(state.work_service(), &service));
    assert!(state.work_service().list_works().await.unwrap().is_empty());
}
