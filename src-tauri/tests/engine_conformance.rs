use std::{
    collections::HashMap,
    future::Future,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use piwork_lib::{
    domain::{
        event::{LivenessState, SessionTransition},
        work::PermissionMode,
    },
    engine::{
        EngineAdapter, EngineCapabilities, EngineError, EngineEvent, EngineInput, EngineRunContext,
        EngineRunIdentity, EngineSessionRef,
        fake::{FakeEngineAdapter, FakeEngineConfig, FakeRunBehavior, FakeStartBarrier},
        pi::PiEngineAdapter,
    },
    model::{
        AvailableModel, CredentialVault, ModelConfigurationRepository, ModelConnectionInput,
        ModelConnectionResult, ModelConnectionTester, ModelProvider, ModelService,
        SaveModelConfigurationInput,
    },
    storage::sqlite::Database,
};
use tokio::sync::mpsc;

const CONTRACT_TIMEOUT: Duration = Duration::from_secs(10);
const OPAQUE_FAKE_SESSION: &str = "opaque-fake-session-7f06";
const CONTRACT_WORK_ID: &str = "work-contract-7f06";

fn input(message: &str) -> EngineInput {
    EngineInput {
        message: message.into(),
        images: Vec::new(),
        documents: Vec::new(),
    }
}

fn context(run_id: &str, generation: u32) -> EngineRunContext {
    EngineRunContext::new(
        EngineRunIdentity::new(
            CONTRACT_WORK_ID.into(),
            run_id.into(),
            format!("assignment-{run_id}"),
            "agent-instance-contract".into(),
            CONTRACT_WORK_ID.into(),
            generation,
        )
        .unwrap(),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        PermissionMode::Balanced,
        Some("model-configuration-contract".into()),
        PermissionMode::AskEveryStep,
    )
    .unwrap()
}

fn full_capabilities() -> EngineCapabilities {
    EngineCapabilities {
        session_resume: true,
        session_rotate: true,
        native_steer: true,
        cancel: true,
        thought_stream: true,
        plan_updates: true,
        permission_requests: true,
        tool_progress: true,
        usage_reporting: true,
        parallel_tool_calls: true,
    }
}

fn minimal_capabilities() -> EngineCapabilities {
    EngineCapabilities::default()
}

fn pi_capabilities() -> EngineCapabilities {
    EngineCapabilities {
        session_resume: true,
        session_rotate: false,
        native_steer: false,
        cancel: true,
        thought_stream: true,
        plan_updates: false,
        permission_requests: false,
        tool_progress: true,
        usage_reporting: true,
        parallel_tool_calls: false,
    }
}

async fn bounded<T>(label: &str, future: impl Future<Output = T>) -> T {
    tokio::time::timeout(CONTRACT_TIMEOUT, future)
        .await
        .unwrap_or_else(|_| panic!("{label} exceeded {CONTRACT_TIMEOUT:?}"))
}

async fn collect_closed(mut receiver: mpsc::Receiver<EngineEvent>) -> Vec<EngineEvent> {
    bounded("engine event channel closure", async move {
        let mut events = Vec::new();
        while let Some(event) = receiver.recv().await {
            events.push(event);
        }
        events
    })
    .await
}

async fn receive_event(receiver: &mut mpsc::Receiver<EngineEvent>) -> EngineEvent {
    bounded("next engine event", receiver.recv())
        .await
        .expect("engine event channel closed before the expected event")
}

fn assert_one_terminal(events: &[EngineEvent]) {
    assert_eq!(
        events.iter().filter(|event| event.is_terminal()).count(),
        1,
        "adapter must close every accepted run with exactly one terminal event: {events:?}"
    );
    assert!(
        events.last().is_some_and(EngineEvent::is_terminal),
        "terminal event must be last: {events:?}"
    );
}

fn assert_event_contract(events: &[EngineEvent], capabilities: EngineCapabilities) {
    assert_one_terminal(events);
    let started = events
        .iter()
        .position(|event| matches!(event, EngineEvent::RunStarted { .. }))
        .expect("accepted run omitted RunStarted");
    assert!(
        events[..started]
            .iter()
            .all(|event| matches!(event, EngineEvent::SessionChanged { .. })),
        "only session metadata may precede RunStarted: {events:?}"
    );

    let has_thought = events
        .iter()
        .any(|event| matches!(event, EngineEvent::ThoughtDelta { .. }));
    let has_plan = events
        .iter()
        .any(|event| matches!(event, EngineEvent::PlanChanged { .. }));
    let has_permission = events.iter().any(|event| {
        matches!(
            event,
            EngineEvent::PermissionRequested { .. } | EngineEvent::PermissionResolved { .. }
        )
    });
    let has_progress = events
        .iter()
        .any(|event| matches!(event, EngineEvent::ToolProgress { .. }));
    let has_usage = events
        .iter()
        .any(|event| matches!(event, EngineEvent::UsageUpdated { .. }));
    assert_eq!(has_thought, capabilities.thought_stream);
    assert_eq!(has_plan, capabilities.plan_updates);
    assert_eq!(has_permission, capabilities.permission_requests);
    assert_eq!(has_progress, capabilities.tool_progress);
    assert_eq!(has_usage, capabilities.usage_reporting);

    let mut active_tools = 0_i32;
    let mut maximum_active_tools = 0_i32;
    for event in events {
        match event {
            EngineEvent::ToolStarted { .. } => {
                active_tools += 1;
                maximum_active_tools = maximum_active_tools.max(active_tools);
            }
            EngineEvent::ToolFinished { .. } => {
                assert!(active_tools > 0, "tool finished before it started");
                active_tools -= 1;
            }
            _ => {}
        }
    }
    assert_eq!(active_tools, 0, "tool call remained unfinished");
    assert_eq!(
        maximum_active_tools > 1,
        capabilities.parallel_tool_calls,
        "parallel tool capability did not match observable ordering"
    );
}

#[derive(Debug, Clone, Copy)]
enum FixtureScenario {
    Complete,
    HoldUntilAbort,
    DuplicateTerminal,
}

struct AdapterFixture {
    adapter: Arc<dyn EngineAdapter>,
}

#[async_trait]
trait AdapterFactory: Send + Sync {
    fn name(&self) -> &'static str;
    fn expected_capabilities(&self) -> EngineCapabilities;
    async fn create(&self, scenario: FixtureScenario) -> AdapterFixture;
}

#[derive(Clone, Copy)]
enum FakeProfile {
    Full,
    Minimal,
}

struct FakeAdapterFactory {
    profile: FakeProfile,
}

#[async_trait]
impl AdapterFactory for FakeAdapterFactory {
    fn name(&self) -> &'static str {
        match self.profile {
            FakeProfile::Full => "fake-full",
            FakeProfile::Minimal => "fake-minimal",
        }
    }

    fn expected_capabilities(&self) -> EngineCapabilities {
        match self.profile {
            FakeProfile::Full => full_capabilities(),
            FakeProfile::Minimal => minimal_capabilities(),
        }
    }

    async fn create(&self, scenario: FixtureScenario) -> AdapterFixture {
        let behavior = match scenario {
            FixtureScenario::Complete => FakeRunBehavior::Complete,
            FixtureScenario::HoldUntilAbort => FakeRunBehavior::HoldUntilAbort,
            FixtureScenario::DuplicateTerminal => FakeRunBehavior::DuplicateTerminal,
        };
        AdapterFixture {
            adapter: Arc::new(FakeEngineAdapter::configured(
                FakeEngineConfig::new(self.expected_capabilities())
                    .with_session_id(OPAQUE_FAKE_SESSION)
                    .with_run_behavior(behavior),
            )),
        }
    }
}

#[derive(Default)]
struct FixtureVault {
    secrets: Mutex<HashMap<String, String>>,
}

impl CredentialVault for FixtureVault {
    fn store_api_key(&self, configuration_id: &str, api_key: &str) -> Result<(), String> {
        self.secrets
            .lock()
            .unwrap()
            .insert(configuration_id.into(), api_key.into());
        Ok(())
    }

    fn delete_api_key(&self, configuration_id: &str) -> Result<(), String> {
        self.secrets.lock().unwrap().remove(configuration_id);
        Ok(())
    }

    fn load_api_key(&self, configuration_id: &str) -> Result<String, String> {
        self.secrets
            .lock()
            .unwrap()
            .get(configuration_id)
            .cloned()
            .ok_or_else(|| "fixture credential missing".into())
    }

    fn load_legacy_api_key(&self) -> Result<String, String> {
        Err("fixture has no legacy credential".into())
    }

    fn delete_legacy_api_key(&self) -> Result<(), String> {
        Ok(())
    }
}

struct FixtureConnectionTester;

#[async_trait]
impl ModelConnectionTester for FixtureConnectionTester {
    async fn test(&self, _input: &ModelConnectionInput) -> Result<ModelConnectionResult, String> {
        Ok(ModelConnectionResult {
            models: vec![AvailableModel {
                id: "fixture-model".into(),
                label: "Fixture model".into(),
            }],
        })
    }
}

async fn fixture_model_service() -> Arc<ModelService> {
    let database = Database::open_in_memory().await.unwrap();
    let service = Arc::new(ModelService::new(
        ModelConfigurationRepository::new(database.pool().clone()),
        Arc::new(FixtureVault::default()),
        Arc::new(FixtureConnectionTester),
    ));
    service
        .save(SaveModelConfigurationInput {
            id: Some("fixture-configuration".into()),
            provider: ModelProvider::Custom,
            api_key: "fixture-key-never-sent-to-a-provider".into(),
            base_url: "https://fixture.invalid/v1".into(),
            model_id: "fixture-model".into(),
        })
        .await
        .unwrap();
    service
}

fn node_executable() -> PathBuf {
    let output = if cfg!(windows) {
        Command::new("where.exe").arg("node.exe").output()
    } else {
        Command::new("which").arg("node").output()
    }
    .expect("Node lookup must run for the Pi fixture");
    assert!(
        output.status.success(),
        "Node is required for the Pi fixture"
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(PathBuf::from)
        .expect("Node lookup returned no executable")
}

fn install_node_fixture(root: &Path, scenario: FixtureScenario) -> PathBuf {
    let bundle = root.join("bundle");
    let fixture_directory = bundle.join("fixture");
    std::fs::create_dir_all(&fixture_directory).unwrap();
    let bundled_node = bundle.join(if cfg!(windows) { "node.exe" } else { "node" });
    let source_node = node_executable();
    if std::fs::hard_link(&source_node, &bundled_node).is_err() {
        std::fs::copy(&source_node, &bundled_node).unwrap();
    }

    let scenario = match scenario {
        FixtureScenario::Complete => "complete",
        FixtureScenario::HoldUntilAbort => "hold",
        FixtureScenario::DuplicateTerminal => "duplicate_terminal",
    };
    let script = format!(
        r#"const readline = require('readline');
const scenario = '{scenario}';
const rl = readline.createInterface({{ input: process.stdin }});
const send = value => process.stdout.write(JSON.stringify(value) + '\n');
rl.on('line', line => {{
  const command = JSON.parse(line);
  if (command.type === 'prompt') {{
    send({{ type: 'response', id: command.id, success: true }});
    if (scenario === 'hold') return;
    send({{ type: 'message_update', assistantMessageEvent: {{ type: 'thinking_delta', delta: 'checking' }} }});
    send({{ type: 'message_update', assistantMessageEvent: {{ type: 'text_delta', delta: command.message }} }});
    send({{ type: 'tool_execution_start', toolCallId: 'call-1', toolName: 'read', args: {{ path: 'fixture.txt' }} }});
    send({{ type: 'tool_execution_update', toolCallId: 'call-1', toolName: 'read', partialResult: {{ content: [{{ type: 'text', text: 'halfway' }}] }} }});
    send({{ type: 'tool_execution_end', toolCallId: 'call-1', toolName: 'read', result: {{ content: [{{ type: 'text', text: 'done' }}] }}, isError: false }});
    send({{ type: 'message_end', message: {{ role: 'assistant', usage: {{ input: 1, output: 2, cacheRead: 3, cacheWrite: 4, totalTokens: 10 }} }} }});
    send({{ type: 'agent_end', messages: [] }});
    if (scenario === 'duplicate_terminal') send({{ type: 'agent_end', messages: [] }});
  }}
}});
"#
    );
    let script_path = fixture_directory.join("pi-rpc-fixture.js");
    std::fs::write(&script_path, script).unwrap();
    script_path
}

struct PiFixtureAdapter {
    inner: PiEngineAdapter,
    _root: tempfile::TempDir,
}

#[async_trait]
impl EngineAdapter for PiFixtureAdapter {
    fn kind(&self) -> &'static str {
        self.inner.kind()
    }

    fn capabilities(&self) -> EngineCapabilities {
        self.inner.capabilities()
    }

    async fn model_label(&self, fallback: &str) -> Result<String, EngineError> {
        self.inner.model_label(fallback).await
    }

    async fn start(
        &self,
        context: EngineRunContext,
        input: EngineInput,
        sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        self.inner.start(context, input, sink).await
    }

    async fn resume(
        &self,
        context: EngineRunContext,
        input: EngineInput,
        sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        self.inner.resume(context, input, sink).await
    }

    async fn rotate(
        &self,
        context: EngineRunContext,
        reason: &str,
    ) -> Result<EngineSessionRef, EngineError> {
        self.inner.rotate(context, reason).await
    }

    async fn steer(&self, run_id: &str, input: EngineInput) -> Result<(), EngineError> {
        self.inner.steer(run_id, input).await
    }

    async fn abort(&self, run_id: &str) -> Result<(), EngineError> {
        self.inner.abort(run_id).await
    }
}

struct PiAdapterFactory;

#[async_trait]
impl AdapterFactory for PiAdapterFactory {
    fn name(&self) -> &'static str {
        "pi-rpc-fixture"
    }

    fn expected_capabilities(&self) -> EngineCapabilities {
        pi_capabilities()
    }

    async fn create(&self, scenario: FixtureScenario) -> AdapterFixture {
        let root = tempfile::tempdir().unwrap();
        let executable = install_node_fixture(root.path(), scenario);
        let inner = PiEngineAdapter::production_with_executable(
            fixture_model_service().await,
            root.path().join("sessions"),
            root.path().join("runtime"),
            Some(executable),
        )
        .unwrap();
        AdapterFixture {
            adapter: Arc::new(PiFixtureAdapter { inner, _root: root }),
        }
    }
}

fn adapter_factories() -> Vec<Box<dyn AdapterFactory>> {
    vec![
        Box::new(FakeAdapterFactory {
            profile: FakeProfile::Full,
        }),
        Box::new(FakeAdapterFactory {
            profile: FakeProfile::Minimal,
        }),
        Box::new(PiAdapterFactory),
    ]
}

async fn start_and_collect(
    adapter: &Arc<dyn EngineAdapter>,
    run_id: &str,
) -> (EngineSessionRef, Vec<EngineEvent>) {
    start_with_input_and_collect(adapter, run_id, "execute contract").await
}

async fn start_with_input_and_collect(
    adapter: &Arc<dyn EngineAdapter>,
    run_id: &str,
    message: &str,
) -> (EngineSessionRef, Vec<EngineEvent>) {
    let (sender, receiver) = mpsc::channel(64);
    let session = bounded(
        "adapter start",
        adapter.start(context(run_id, 0), input(message), sender),
    )
    .await
    .unwrap();
    let events = collect_closed(receiver).await;
    (session, events)
}

async fn assert_factory_contract(factory: &dyn AdapterFactory) {
    let fixture = factory.create(FixtureScenario::Complete).await;
    let adapter = &fixture.adapter;
    let capabilities = factory.expected_capabilities();
    assert_eq!(
        adapter.capabilities(),
        capabilities,
        "{} capability bitmap differs from its registration",
        factory.name()
    );

    let (session, events) = start_and_collect(adapter, "run-start").await;
    assert_event_contract(&events, capabilities);
    for event in &events {
        assert!(
            !format!("{event:?}").contains(&session.session_id),
            "{} leaked its opaque session id into an event",
            factory.name()
        );
    }

    let (resume_sender, resume_receiver) = mpsc::channel(64);
    let resume = bounded(
        "adapter resume",
        adapter.resume(
            context("run-resume", 0),
            input("continue contract"),
            resume_sender,
        ),
    )
    .await;
    if capabilities.session_resume {
        let resumed = resume.unwrap();
        assert_eq!(resumed.session_id, session.session_id);
        let resumed_events = collect_closed(resume_receiver).await;
        assert_event_contract(&resumed_events, capabilities);
        assert!(matches!(
            resumed_events.first(),
            Some(EngineEvent::SessionChanged {
                transition: SessionTransition::Resumed,
                ..
            })
        ));
    } else {
        assert!(matches!(
            resume,
            Err(EngineError::Unsupported("session_resume"))
        ));
    }

    let rotate = bounded(
        "adapter rotate",
        adapter.rotate(context("run-rotate", 0), "contract rotation"),
    )
    .await;
    if capabilities.session_rotate {
        assert_ne!(rotate.unwrap().session_id, session.session_id);
    } else {
        assert!(matches!(
            rotate,
            Err(EngineError::Unsupported("session_rotate"))
        ));
    }

    if capabilities.cancel {
        let controlled = factory.create(FixtureScenario::HoldUntilAbort).await;
        let (sender, mut receiver) = mpsc::channel(8);
        bounded(
            "controlled adapter start",
            controlled
                .adapter
                .start(context("run-control", 0), input("wait for control"), sender),
        )
        .await
        .unwrap();
        let first = receive_event(&mut receiver).await;
        assert!(matches!(first, EngineEvent::RunStarted { .. }));
        let steer = bounded(
            "adapter steer",
            controlled
                .adapter
                .steer("run-control", input("change direction")),
        )
        .await;
        if capabilities.native_steer {
            steer.unwrap();
        } else {
            assert!(matches!(
                steer,
                Err(EngineError::Unsupported("native_steer"))
            ));
        }
        bounded("adapter abort", controlled.adapter.abort("run-control"))
            .await
            .unwrap();
        let mut aborted_events = vec![first];
        aborted_events.extend(collect_closed(receiver).await);
        assert_one_terminal(&aborted_events);
        assert!(matches!(
            aborted_events.last(),
            Some(EngineEvent::RunFailed { .. })
        ));

        if !capabilities.native_steer {
            let replacement = factory.create(FixtureScenario::Complete).await;
            let revised_input = "initial direction\n\nchange direction";
            let (_, replacement_events) = start_with_input_and_collect(
                &replacement.adapter,
                "run-control-replacement",
                revised_input,
            )
            .await;
            assert_event_contract(&replacement_events, capabilities);
            assert!(replacement_events.iter().any(|event| matches!(
                event,
                EngineEvent::AssistantDelta { text } if text.contains(revised_input)
            )));
        }
    } else {
        assert!(matches!(
            bounded(
                "unsupported adapter steer",
                adapter.steer("missing-run", input("steer"))
            )
            .await,
            Err(EngineError::Unsupported("native_steer"))
        ));
        assert!(matches!(
            bounded("unsupported adapter abort", adapter.abort("missing-run")).await,
            Err(EngineError::Unsupported("cancel"))
        ));
    }

    let error = bounded("post-run abort", adapter.abort("missing-run")).await;
    let error = error.unwrap_err();
    assert!(!error.to_string().contains(&session.session_id));
    assert!(!format!("{error:?}").contains(&session.session_id));
}

#[tokio::test]
async fn every_registered_adapter_passes_the_shared_capability_contract() {
    for factory in adapter_factories() {
        bounded(factory.name(), assert_factory_contract(factory.as_ref())).await;
    }
}

#[tokio::test]
async fn duplicate_terminal_input_is_normalized_to_exactly_one_terminal_event() {
    for factory in adapter_factories() {
        let fixture = factory.create(FixtureScenario::DuplicateTerminal).await;
        let (_, events) = start_and_collect(&fixture.adapter, "run-duplicate-terminal").await;
        assert_one_terminal(&events);
    }
}

#[tokio::test]
async fn receiver_close_is_bounded_and_surfaces_a_clear_error_without_panicking() {
    for factory in adapter_factories()
        .into_iter()
        .filter(|factory| factory.expected_capabilities().cancel)
    {
        let fixture = factory.create(FixtureScenario::Complete).await;
        let (sender, mut receiver) = mpsc::channel(1);
        bounded(
            "early-close adapter start",
            fixture.adapter.start(
                context("run-early-close", 0),
                input("close receiver"),
                sender,
            ),
        )
        .await
        .unwrap();
        assert!(matches!(
            receive_event(&mut receiver).await,
            EngineEvent::RunStarted { .. }
        ));
        drop(receiver);

        let final_error = bounded("early-close cleanup", async {
            loop {
                match fixture.adapter.abort("run-early-close").await {
                    Err(EngineError::NotRunning) => break EngineError::NotRunning,
                    Ok(()) | Err(EngineError::Aborted) => tokio::task::yield_now().await,
                    Err(error) => panic!("unexpected early-close error: {error}"),
                }
            }
        })
        .await;
        assert_eq!(final_error.to_string(), "engine run is not active");
    }
}

struct DefaultsOnlyAdapter;

#[async_trait]
impl EngineAdapter for DefaultsOnlyAdapter {
    fn kind(&self) -> &'static str {
        "defaults-only"
    }

    async fn start(
        &self,
        _context: EngineRunContext,
        _input: EngineInput,
        _sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        Err(EngineError::Start("not used by this contract test".into()))
    }
}

#[tokio::test]
async fn default_control_methods_return_unsupported_without_fabricating_success() {
    let adapter: Arc<dyn EngineAdapter> = Arc::new(DefaultsOnlyAdapter);
    let (sender, _receiver) = mpsc::channel(1);

    assert!(matches!(
        bounded(
            "default resume",
            adapter.resume(context("run-resume", 0), input("resume"), sender)
        )
        .await,
        Err(EngineError::Unsupported("session_resume"))
    ));
    assert!(matches!(
        bounded(
            "default rotate",
            adapter.rotate(context("run-rotate", 0), "context limit")
        )
        .await,
        Err(EngineError::Unsupported("session_rotate"))
    ));
    assert!(matches!(
        bounded(
            "default steer",
            adapter.steer("run-steer", input("change direction"))
        )
        .await,
        Err(EngineError::Unsupported("native_steer"))
    ));
    assert!(matches!(
        bounded("default abort", adapter.abort("run-abort")).await,
        Err(EngineError::Unsupported("cancel"))
    ));
}

#[tokio::test]
async fn fake_start_barrier_is_bounded_auxiliary_evidence() {
    let barrier = Arc::new(FakeStartBarrier::new());
    let adapter = Arc::new(FakeEngineAdapter::configured(
        FakeEngineConfig::new(full_capabilities())
            .with_session_id(OPAQUE_FAKE_SESSION)
            .with_start_barrier(Arc::clone(&barrier)),
    ));
    let (sender, receiver) = mpsc::channel(32);
    let start_adapter = Arc::clone(&adapter);
    let start = tokio::spawn(async move {
        start_adapter
            .start(context("run-barrier", 0), input("start"), sender)
            .await
    });

    bounded("fake start barrier", barrier.wait_until_entered()).await;
    assert!(!start.is_finished());
    barrier.release();
    bounded("barrier start task", start).await.unwrap().unwrap();
    assert_one_terminal(&collect_closed(receiver).await);
    let observations = bounded("fake start observations", adapter.observations()).await;
    assert_eq!(observations.started_run_ids, ["run-barrier"]);
}

#[tokio::test]
async fn fake_control_observations_record_steer_and_abort_calls() {
    let adapter = Arc::new(FakeEngineAdapter::configured(
        FakeEngineConfig::new(full_capabilities())
            .with_session_id(OPAQUE_FAKE_SESSION)
            .with_run_behavior(FakeRunBehavior::HoldUntilAbort),
    ));
    let (sender, mut receiver) = mpsc::channel(8);
    bounded(
        "fake controlled start",
        adapter.start(
            context("run-fake-control", 0),
            input("wait for control"),
            sender,
        ),
    )
    .await
    .unwrap();
    let first = receive_event(&mut receiver).await;
    assert!(matches!(first, EngineEvent::RunStarted { .. }));

    bounded(
        "fake steer",
        adapter.steer("run-fake-control", input("use the safer approach")),
    )
    .await
    .unwrap();
    bounded("fake abort", adapter.abort("run-fake-control"))
        .await
        .unwrap();

    let mut events = vec![first];
    events.extend(collect_closed(receiver).await);
    assert_one_terminal(&events);
    let observations = bounded("fake control observations", adapter.observations()).await;
    assert_eq!(
        observations.steers,
        [("run-fake-control".into(), "use the safer approach".into())]
    );
    assert_eq!(observations.aborted_run_ids, ["run-fake-control"]);
}

#[tokio::test]
async fn fake_crash_and_liveness_timeout_close_with_one_failure_terminal() {
    for (run_id, behavior, expected_liveness) in [
        ("run-crash", FakeRunBehavior::Crash, false),
        ("run-timeout", FakeRunBehavior::LivenessTimeout, true),
    ] {
        let adapter: Arc<dyn EngineAdapter> = Arc::new(FakeEngineAdapter::configured(
            FakeEngineConfig::new(minimal_capabilities()).with_run_behavior(behavior),
        ));
        let (_, events) = start_and_collect(&adapter, run_id).await;

        assert_one_terminal(&events);
        assert!(matches!(events.last(), Some(EngineEvent::RunFailed { .. })));
        assert_eq!(
            events.iter().any(|event| matches!(
                event,
                EngineEvent::Liveness {
                    state: LivenessState::Stalled
                }
            )),
            expected_liveness
        );
    }
}
