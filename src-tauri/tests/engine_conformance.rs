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

const CONTRACT_TIMEOUT: Duration = Duration::from_secs(30);
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

#[test]
fn invalid_path_identities_are_rejected_before_any_files_are_created() {
    let sandbox = tempfile::tempdir().unwrap();
    let workspace = sandbox.path().join("workspace");
    let outside = sandbox.path().join("outside");
    std::fs::create_dir(&workspace).unwrap();

    for (work_id, run_id) in [("../outside", "run-safe"), ("work-safe", "../outside")] {
        let identity = EngineRunIdentity::new(
            work_id.into(),
            run_id.into(),
            "assignment-safe".into(),
            "agent-instance-safe".into(),
            "agent-session-safe".into(),
            0,
        );

        assert!(matches!(identity, Err(EngineError::Start(_))));
    }

    assert_eq!(std::fs::read_dir(&workspace).unwrap().count(), 0);
    assert!(!outside.exists());
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

#[test]
fn tool_call_validator_rejects_malicious_id_and_name_sequences() {
    let started = |id: &str, name: &str| EngineEvent::ToolStarted {
        tool_call_id: id.into(),
        tool_name: name.into(),
        input_summary: String::new(),
    };
    let progress = |id: &str, name: &str| EngineEvent::ToolProgress {
        tool_call_id: id.into(),
        tool_name: name.into(),
        output_summary: String::new(),
    };
    let finished = |id: &str, name: &str| EngineEvent::ToolFinished {
        tool_call_id: id.into(),
        tool_name: name.into(),
        output_summary: String::new(),
        success: true,
    };
    let terminal = || EngineEvent::RunCompleted {
        summary: String::new(),
        artifacts: Vec::new(),
        validation: Vec::new(),
        limitations: Vec::new(),
    };
    let invalid = [
        vec![
            started("call-a", "read"),
            started("call-b", "bash"),
            finished("call-a", "read"),
            finished("call-a", "read"),
            terminal(),
        ],
        vec![progress("missing", "read"), terminal()],
        vec![
            started("call-a", "read"),
            progress("call-a", "bash"),
            finished("call-a", "read"),
            terminal(),
        ],
        vec![
            started("call-a", "read"),
            finished("call-a", "bash"),
            terminal(),
        ],
        vec![
            started("call-a", "read"),
            started("call-a", "read"),
            finished("call-a", "read"),
            terminal(),
        ],
        vec![started("call-a", "read"), terminal()],
    ];

    for events in invalid {
        assert!(
            validate_tool_calls(&events).is_err(),
            "validator accepted {events:?}"
        );
    }

    let valid_parallel = vec![
        started("call-a", "read"),
        started("call-b", "bash"),
        progress("call-b", "bash"),
        finished("call-a", "read"),
        finished("call-b", "bash"),
        terminal(),
    ];
    assert_eq!(validate_tool_calls(&valid_parallel), Ok(2));
}

fn validate_tool_calls(events: &[EngineEvent]) -> Result<usize, String> {
    let mut active = HashMap::<String, String>::new();
    let mut maximum_active = 0;

    for event in events {
        match event {
            EngineEvent::ToolStarted {
                tool_call_id,
                tool_name,
                ..
            } => {
                if active
                    .insert(tool_call_id.clone(), tool_name.clone())
                    .is_some()
                {
                    return Err("tool call ID started more than once".into());
                }
                maximum_active = maximum_active.max(active.len());
            }
            EngineEvent::ToolProgress {
                tool_call_id,
                tool_name,
                ..
            } => match active.get(tool_call_id) {
                Some(active_name) if active_name == tool_name => {}
                Some(_) => return Err("tool progress changed the active tool name".into()),
                None => return Err("tool progress referenced an inactive call ID".into()),
            },
            EngineEvent::ToolFinished {
                tool_call_id,
                tool_name,
                ..
            } => match active.get(tool_call_id) {
                Some(active_name) if active_name == tool_name => {
                    active.remove(tool_call_id);
                }
                Some(_) => return Err("tool finish changed the active tool name".into()),
                None => return Err("tool finish referenced an inactive call ID".into()),
            },
            event if event.is_terminal() && !active.is_empty() => {
                return Err("terminal event arrived with active tool calls".into());
            }
            _ => {}
        }
    }

    if !active.is_empty() {
        return Err("tool call remained unfinished".into());
    }
    Ok(maximum_active)
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

    let maximum_active_tools =
        validate_tool_calls(events).unwrap_or_else(|error| panic!("{error}: {events:?}"));
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
    StartupCrash,
    PromptRejected,
    StartupTimeout,
    PreAcceptanceEvents,
    PreAcceptanceTerminal,
    PreAcceptanceFlood,
    SensitiveRawEvent,
    Backpressure,
    MalformedBackpressure,
    InvalidUtf8Backpressure,
    InheritedStderrComplete,
    InheritedStderrHold,
    StartupBarrier,
    AbortBarrier,
    HoldWithWatchdog,
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
            FixtureScenario::StartupCrash => FakeRunBehavior::Crash,
            FixtureScenario::PromptRejected | FixtureScenario::StartupTimeout => {
                FakeRunBehavior::Crash
            }
            FixtureScenario::PreAcceptanceEvents => FakeRunBehavior::Complete,
            FixtureScenario::PreAcceptanceTerminal | FixtureScenario::PreAcceptanceFlood => {
                FakeRunBehavior::Crash
            }
            FixtureScenario::SensitiveRawEvent => FakeRunBehavior::Complete,
            FixtureScenario::Backpressure
            | FixtureScenario::MalformedBackpressure
            | FixtureScenario::InvalidUtf8Backpressure
            | FixtureScenario::InheritedStderrHold => FakeRunBehavior::HoldUntilAbort,
            FixtureScenario::InheritedStderrComplete => FakeRunBehavior::Complete,
            FixtureScenario::StartupBarrier
            | FixtureScenario::AbortBarrier
            | FixtureScenario::HoldWithWatchdog => FakeRunBehavior::HoldUntilAbort,
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
        FixtureScenario::StartupCrash => "startup_crash",
        FixtureScenario::PromptRejected => "prompt_rejected",
        FixtureScenario::StartupTimeout => "startup_timeout",
        FixtureScenario::PreAcceptanceEvents => "pre_acceptance_events",
        FixtureScenario::PreAcceptanceTerminal => "pre_acceptance_terminal",
        FixtureScenario::PreAcceptanceFlood => "pre_acceptance_flood",
        FixtureScenario::SensitiveRawEvent => "sensitive_raw_event",
        FixtureScenario::Backpressure => "backpressure",
        FixtureScenario::MalformedBackpressure => "malformed_backpressure",
        FixtureScenario::InvalidUtf8Backpressure => "invalid_utf8_backpressure",
        FixtureScenario::InheritedStderrComplete => "inherited_stderr_complete",
        FixtureScenario::InheritedStderrHold => "inherited_stderr_hold",
        FixtureScenario::StartupBarrier => "startup_barrier",
        FixtureScenario::AbortBarrier => "abort_barrier",
        FixtureScenario::HoldWithWatchdog => "hold_watchdog",
    };
    let script = format!(
        r#"const readline = require('readline');
const fs = require('fs');
const path = require('path');
const childProcess = require('child_process');
const scenario = '{scenario}';
fs.writeFileSync(path.join(__dirname, 'argv.json'), JSON.stringify(process.argv.slice(2)));
const rl = readline.createInterface({{ input: process.stdin }});
const send = value => process.stdout.write(JSON.stringify(value) + '\n');
const marker = name => path.join(__dirname, name);
const waitForMarker = (name, callback) => {{
  const timer = setInterval(() => {{
    if (fs.existsSync(marker(name))) {{
      clearInterval(timer);
      callback();
    }}
  }}, 10);
}};
const spawnStderrHolder = () => {{
  const holderScript =
    "const fs=require('fs');const path=require('path');" +
    "const directory=" + JSON.stringify(__dirname) + ";" +
    "fs.writeFileSync(path.join(directory,'stderr-holder-entered'),String(process.pid));" +
    "const timer=setInterval(()=>{{if(fs.existsSync(path.join(directory,'stderr-holder-release'))){{" +
    "clearInterval(timer);fs.writeFileSync(path.join(directory,'stderr-holder-exited'),String(process.pid));process.exit(0);}}}},10);" +
    "setTimeout(()=>{{fs.writeFileSync(path.join(directory,'stderr-holder-watchdog'),String(process.pid));" +
    "fs.writeFileSync(path.join(directory,'stderr-holder-exited'),String(process.pid));process.exit(0);}},5000);";
  const holder = childProcess.spawn(process.execPath, ['-e', holderScript], {{
    detached: true,
    stdio: ['ignore', 'ignore', 'inherit']
  }});
  holder.unref();
}};
rl.on('line', line => {{
  const command = JSON.parse(line);
  if (command.type === 'prompt') {{
    if (scenario === 'startup_crash') {{
      process.stderr.write('startup-stderr-sentinel|' + process.env.PIWORK_MODEL_API_KEY + '|' + process.argv.join('|'));
      process.exit(17);
      return;
    }}
    if (scenario === 'prompt_rejected') {{
      process.stderr.write('prompt-rejection-stderr-sentinel|' + process.env.PIWORK_MODEL_API_KEY);
      send({{ type: 'response', id: command.id, success: false, error: {{ token: 'prompt-rejection-detail-sentinel' }} }});
      return;
    }}
    if (scenario === 'startup_timeout' && !fs.existsSync(marker('startup-timeout-consumed'))) {{
      fs.writeFileSync(marker('startup-timeout-consumed'), String(process.pid));
      process.stderr.write('startup-timeout-stderr-sentinel|' + process.env.PIWORK_MODEL_API_KEY);
      return;
    }}
    if (scenario === 'startup_barrier') {{
      fs.appendFileSync(marker('startup-entered.log'), process.pid + '\n');
      waitForMarker('startup-release', () => send({{ type: 'response', id: command.id, success: true }}));
      return;
    }}
    if (scenario === 'pre_acceptance_events') {{
      send({{ type: 'message_update', assistantMessageEvent: {{ type: 'thinking_delta', delta: 'before-acceptance' }} }});
      send({{ type: 'future_pre_acceptance', payload: 'buffered-raw' }});
      send({{ type: 'response', id: command.id, success: true }});
      send({{ type: 'agent_end', messages: [] }});
      return;
    }}
    if (scenario === 'pre_acceptance_terminal') {{
      send({{ type: 'message_update', assistantMessageEvent: {{ type: 'error', error: 'early failure' }} }});
      send({{ type: 'response', id: command.id, success: true }});
      send({{ type: 'agent_end', messages: [] }});
      return;
    }}
    if (scenario === 'pre_acceptance_flood') {{
      for (let index = 0; index < 65; index += 1) {{
        send({{ type: 'future_pre_acceptance', index }});
      }}
      fs.writeFileSync(marker('pre-acceptance-flood-emitted'), String(process.pid));
      return;
    }}
    send({{ type: 'response', id: command.id, success: true }});
    if (scenario === 'sensitive_raw_event') {{
      const argument = name => process.argv[process.argv.indexOf(name) + 1];
      const model = argument('--model');
      send({{
        type: 'future-' + model,
        token: 'raw-sensitive-key-sentinel',
        values: {{
          api: process.env.PIWORK_MODEL_API_KEY,
          work: argument('--session-id'),
          workspace: process.cwd(),
          sessionPath: argument('--session-dir'),
          runtimePath: process.env.PI_CODING_AGENT_DIR,
          model
        }}
      }});
      send({{ type: 'agent_end', messages: [] }});
      return;
    }}
    if (scenario === 'backpressure') {{
      send({{ type: 'message_update', assistantMessageEvent: {{ type: 'thinking_delta', delta: 'blocked' }} }});
      fs.writeFileSync(marker('backpressure-emitted'), String(process.pid));
      return;
    }}
    if (scenario === 'malformed_backpressure') {{
      process.stdout.write('{{malformed\n');
      fs.writeFileSync(marker('backpressure-emitted'), String(process.pid));
      return;
    }}
    if (scenario === 'invalid_utf8_backpressure') {{
      process.stdout.write(Buffer.from([0xff, 0x0a]));
      fs.writeFileSync(marker('backpressure-emitted'), String(process.pid));
      return;
    }}
    if (scenario === 'inherited_stderr_complete') {{
      spawnStderrHolder();
      waitForMarker('stderr-holder-entered', () => send({{ type: 'agent_end', messages: [] }}));
      return;
    }}
    if (scenario === 'inherited_stderr_hold') {{
      spawnStderrHolder();
      return;
    }}
    if (scenario === 'hold' || scenario === 'abort_barrier') return;
    if (scenario === 'hold_watchdog') {{
      setTimeout(() => {{
        fs.writeFileSync(marker('watchdog-fired'), String(process.pid));
        process.exit(0);
      }}, 1000);
      return;
    }}
    send({{ type: 'message_update', assistantMessageEvent: {{ type: 'thinking_delta', delta: 'checking' }} }});
    send({{ type: 'message_update', assistantMessageEvent: {{ type: 'text_delta', delta: command.message }} }});
    send({{ type: 'tool_execution_start', toolCallId: 'call-1', toolName: 'read', args: {{ path: 'fixture.txt' }} }});
    send({{ type: 'tool_execution_update', toolCallId: 'call-1', toolName: 'read', partialResult: {{ content: [{{ type: 'text', text: 'halfway' }}] }} }});
    send({{ type: 'tool_execution_end', toolCallId: 'call-1', toolName: 'read', result: {{ content: [{{ type: 'text', text: 'done' }}] }}, isError: false }});
    send({{ type: 'message_end', message: {{ role: 'assistant', usage: {{ input: 1, output: 2, cacheRead: 3, cacheWrite: 4, totalTokens: 10 }} }} }});
    send({{ type: 'agent_end', messages: [] }});
    if (scenario === 'duplicate_terminal') send({{ type: 'agent_end', messages: [] }});
  }}
  if (command.type === 'abort') {{
    fs.writeFileSync(marker('abort-entered'), String(process.pid));
    if (scenario === 'prompt_rejected' || scenario === 'startup_timeout') {{
      fs.writeFileSync(marker('startup-cleanup-entered'), String(process.pid));
      waitForMarker('startup-cleanup-release', () => process.exit(0));
      return;
    }}
    if (scenario === 'abort_barrier') {{
      waitForMarker('abort-release', () => process.exit(0));
      return;
    }}
    process.exit(0);
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
    let mut factories: Vec<Box<dyn AdapterFactory>> = vec![
        Box::new(FakeAdapterFactory {
            profile: FakeProfile::Full,
        }),
        Box::new(FakeAdapterFactory {
            profile: FakeProfile::Minimal,
        }),
    ];
    #[cfg(windows)]
    factories.push(Box::new(PiAdapterFactory));
    factories
}

fn fixture_marker(root: &Path, name: &str) -> PathBuf {
    root.join("bundle").join("fixture").join(name)
}

async fn wait_for_fixture_marker(root: &Path, name: &str) {
    let path = fixture_marker(root, name);
    let label = format!("Pi fixture marker {name}");
    bounded(&label, async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
}

fn fixture_descendant_pid(root: &Path) -> u32 {
    std::fs::read_to_string(fixture_marker(root, "stderr-holder-entered"))
        .unwrap()
        .parse()
        .unwrap()
}

#[cfg(windows)]
fn process_is_alive(process_id: u32) -> bool {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "OpenProcess"]
        fn open_process(access: u32, inherit: i32, process_id: u32) -> *mut c_void;
        #[link_name = "WaitForSingleObject"]
        fn wait_for_single_object(handle: *mut c_void, milliseconds: u32) -> u32;
        #[link_name = "CloseHandle"]
        fn close_handle(handle: *mut c_void) -> i32;
    }

    const SYNCHRONIZE: u32 = 0x0010_0000;
    const WAIT_TIMEOUT: u32 = 258;
    let handle = unsafe { open_process(SYNCHRONIZE, 0, process_id) };
    if handle.is_null() {
        return false;
    }
    let alive = unsafe { wait_for_single_object(handle, 0) } == WAIT_TIMEOUT;
    unsafe {
        close_handle(handle);
    }
    alive
}

#[cfg(unix)]
fn process_is_alive(process_id: u32) -> bool {
    unsafe extern "C" {
        fn kill(process_id: i32, signal: i32) -> i32;
    }

    unsafe { kill(process_id as i32, 0) == 0 }
    || std::io::Error::last_os_error().raw_os_error() == Some(1)
}

async fn release_fixture_descendant_if_alive(root: &Path, process_id: u32) {
    if process_is_alive(process_id) {
        std::fs::write(fixture_marker(root, "stderr-holder-release"), b"release").unwrap();
        wait_for_fixture_marker(root, "stderr-holder-exited").await;
    }
}

async fn abort_returned_before_fixture_marker(
    root: &Path,
    abort: &tokio::task::JoinHandle<Result<(), EngineError>>,
) -> bool {
    let marker = fixture_marker(root, "abort-entered");
    bounded("Pi abort RPC or premature return", async {
        loop {
            if marker.exists() {
                break false;
            }
            if abort.is_finished() {
                break true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
}

async fn start_returned_before_fixture_marker(
    root: &Path,
    marker_name: &str,
    start: &tokio::task::JoinHandle<Result<EngineSessionRef, EngineError>>,
) -> bool {
    let marker = fixture_marker(root, marker_name);
    bounded("Pi startup cleanup marker or premature return", async {
        loop {
            if marker.exists() {
                break false;
            }
            if start.is_finished() {
                break true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
}

async fn pi_fixture_adapter(root: &Path, scenario: FixtureScenario) -> Arc<PiEngineAdapter> {
    Arc::new(
        PiEngineAdapter::production_with_executable(
            fixture_model_service().await,
            root.join("sessions"),
            root.join("runtime"),
            Some(install_node_fixture(root, scenario)),
        )
        .unwrap(),
    )
}

#[cfg(windows)]
#[tokio::test]
async fn pi_uses_effective_permission_for_execution_tools() {
    let root = tempfile::tempdir().unwrap();
    let executable = install_node_fixture(root.path(), FixtureScenario::Complete);
    let arguments_path = executable.parent().unwrap().join("argv.json");
    let adapter: Arc<dyn EngineAdapter> = Arc::new(
        PiEngineAdapter::production_with_executable(
            fixture_model_service().await,
            root.path().join("sessions"),
            root.path().join("runtime"),
            Some(executable),
        )
        .unwrap(),
    );

    let (_, events) = start_and_collect(&adapter, "run-effective-permission").await;
    assert_one_terminal(&events);
    let arguments: Vec<String> = serde_json::from_slice(
        &std::fs::read(arguments_path).expect("Pi fixture must capture its argv"),
    )
    .unwrap();
    let tools = arguments
        .windows(2)
        .find(|pair| pair[0] == "--tools")
        .expect("Pi argv must declare tools")[1]
        .as_str();

    assert!(tools.contains("read"));
    assert!(!tools.contains("edit"));
    assert!(!tools.contains("write"));
    assert!(!tools.contains("bash"));
}

#[cfg(windows)]
#[tokio::test]
async fn pi_startup_crash_diagnostic_does_not_include_stderr_or_launch_secrets() {
    let root = tempfile::tempdir().unwrap();
    let adapter = pi_fixture_adapter(root.path(), FixtureScenario::StartupCrash).await;
    let (sender, _receiver) = mpsc::channel(8);

    let error = adapter
        .start(context("run-startup-crash", 0), input("crash"), sender)
        .await
        .unwrap_err();
    let rendered = format!("{error}\n{error:?}");

    for sentinel in [
        "startup-stderr-sentinel",
        "fixture-key-never-sent-to-a-provider",
        "fixture-model",
        CONTRACT_WORK_ID,
        root.path().to_string_lossy().as_ref(),
    ] {
        assert!(
            !rendered.contains(sentinel),
            "startup diagnostic leaked {sentinel}"
        );
    }
    assert_eq!(
        error.to_string(),
        "engine failed to start: Pi RPC exited before accepting the Run"
    );
    assert!(!root.path().join("runtime/run-startup-crash/agent").exists());
    assert!(matches!(
        adapter.abort("run-startup-crash").await,
        Err(EngineError::NotRunning)
    ));
    let (retry_sender, _retry_receiver) = mpsc::channel(8);
    let retry = adapter
        .start(
            context("run-startup-crash", 0),
            input("retry"),
            retry_sender,
        )
        .await
        .unwrap_err();
    assert_eq!(
        retry.to_string(),
        "engine failed to start: Pi RPC exited before accepting the Run"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn pi_prompt_rejection_diagnostic_ignores_rpc_details_and_stderr() {
    let root = tempfile::tempdir().unwrap();
    let adapter = pi_fixture_adapter(root.path(), FixtureScenario::PromptRejected).await;
    let (sender, _receiver) = mpsc::channel(8);
    let start_adapter = Arc::clone(&adapter);
    let start = tokio::spawn(async move {
        start_adapter
            .start(context("run-prompt-rejected", 0), input("reject"), sender)
            .await
    });
    let returned_before_cleanup =
        start_returned_before_fixture_marker(root.path(), "startup-cleanup-entered", &start).await;
    std::fs::write(
        fixture_marker(root.path(), "startup-cleanup-release"),
        b"release",
    )
    .unwrap();
    let error = bounded("rejected Pi startup cleanup", start)
        .await
        .unwrap()
        .unwrap_err();
    let rendered = format!("{error}\n{error:?}");

    for sentinel in [
        "prompt-rejection-stderr-sentinel",
        "prompt-rejection-detail-sentinel",
        "fixture-key-never-sent-to-a-provider",
    ] {
        assert!(!rendered.contains(sentinel));
    }
    assert_eq!(
        error.to_string(),
        "engine failed to start: Pi rejected the Run prompt"
    );
    assert!(
        !returned_before_cleanup,
        "prompt rejection returned before child cleanup began"
    );
    assert!(
        !root
            .path()
            .join("runtime/run-prompt-rejected/agent")
            .exists()
    );
    assert!(matches!(
        adapter.abort("run-prompt-rejected").await,
        Err(EngineError::NotRunning)
    ));
    let (retry_sender, _retry_receiver) = mpsc::channel(8);
    let retry = adapter
        .start(
            context("run-prompt-rejected", 0),
            input("retry"),
            retry_sender,
        )
        .await
        .unwrap_err();
    assert_eq!(
        retry.to_string(),
        "engine failed to start: Pi rejected the Run prompt"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn pi_startup_timeout_diagnostic_ignores_stderr() {
    let root = tempfile::tempdir().unwrap();
    let adapter = pi_fixture_adapter(root.path(), FixtureScenario::StartupTimeout).await;
    let (sender, _receiver) = mpsc::channel(8);
    let start_adapter = Arc::clone(&adapter);
    let start = tokio::spawn(async move {
        start_adapter
            .start(context("run-startup-timeout", 0), input("timeout"), sender)
            .await
    });
    let returned_before_cleanup =
        start_returned_before_fixture_marker(root.path(), "startup-cleanup-entered", &start).await;
    std::fs::write(
        fixture_marker(root.path(), "startup-cleanup-release"),
        b"release",
    )
    .unwrap();
    let error = bounded("timed-out Pi startup cleanup", start)
        .await
        .unwrap()
        .unwrap_err();
    let rendered = format!("{error}\n{error:?}");

    for sentinel in [
        "startup-timeout-stderr-sentinel",
        "fixture-key-never-sent-to-a-provider",
    ] {
        assert!(!rendered.contains(sentinel));
    }
    assert_eq!(
        error.to_string(),
        "engine failed to start: Pi RPC did not accept the Run in time"
    );
    assert!(
        !returned_before_cleanup,
        "startup timeout returned before child cleanup began"
    );
    assert!(
        !root
            .path()
            .join("runtime/run-startup-timeout/agent")
            .exists()
    );
    assert!(matches!(
        adapter.abort("run-startup-timeout").await,
        Err(EngineError::NotRunning)
    ));
    let (retry_sender, retry_receiver) = mpsc::channel(8);
    adapter
        .start(
            context("run-startup-timeout", 0),
            input("retry"),
            retry_sender,
        )
        .await
        .unwrap();
    assert_one_terminal(&collect_closed(retry_receiver).await);
}

#[cfg(windows)]
#[tokio::test]
async fn pi_buffers_pre_acceptance_events_until_after_run_started() {
    let root = tempfile::tempdir().unwrap();
    let adapter = pi_fixture_adapter(root.path(), FixtureScenario::PreAcceptanceEvents).await;
    let (sender, receiver) = mpsc::channel(8);

    adapter
        .start(
            context("run-pre-acceptance-events", 0),
            input("buffer"),
            sender,
        )
        .await
        .unwrap();
    let events = collect_closed(receiver).await;

    assert!(matches!(
        events.first(),
        Some(EngineEvent::RunStarted { .. })
    ));
    assert!(matches!(
        events.get(1),
        Some(EngineEvent::ThoughtDelta { text }) if text == "before-acceptance"
    ));
    assert!(matches!(
        events.get(2),
        Some(EngineEvent::RawEngineEvent { kind, .. }) if kind == "future_pre_acceptance"
    ));
    assert!(matches!(
        events.last(),
        Some(EngineEvent::RunCompleted { .. })
    ));
    assert_one_terminal(&events);
}

#[cfg(windows)]
#[tokio::test]
async fn pi_start_returns_before_capacity_one_pre_acceptance_buffer_flush() {
    let root = tempfile::tempdir().unwrap();
    let adapter = pi_fixture_adapter(root.path(), FixtureScenario::PreAcceptanceEvents).await;
    let (sender, receiver) = mpsc::channel(1);

    tokio::time::timeout(
        Duration::from_secs(2),
        adapter.start(
            context("run-capacity-one-pre-acceptance", 0),
            input("buffer"),
            sender,
        ),
    )
    .await
    .expect("accepted start must not wait for buffered event sink capacity")
    .unwrap();

    let events = collect_closed(receiver).await;
    assert!(matches!(
        events.first(),
        Some(EngineEvent::RunStarted { .. })
    ));
    assert!(matches!(
        events.get(1),
        Some(EngineEvent::ThoughtDelta { text }) if text == "before-acceptance"
    ));
    assert!(matches!(
        events.get(2),
        Some(EngineEvent::RawEngineEvent { kind, .. }) if kind == "future_pre_acceptance"
    ));
    assert_one_terminal(&events);
}

#[cfg(windows)]
#[tokio::test]
async fn pi_treats_a_pre_acceptance_terminal_as_startup_failure() {
    let root = tempfile::tempdir().unwrap();
    let adapter = pi_fixture_adapter(root.path(), FixtureScenario::PreAcceptanceTerminal).await;
    let (sender, receiver) = mpsc::channel(8);

    let result = adapter
        .start(
            context("run-pre-acceptance-terminal", 0),
            input("fail"),
            sender,
        )
        .await;
    let events = collect_closed(receiver).await;

    assert!(matches!(result, Err(EngineError::Start(_))));
    assert_eq!(events.len(), 1, "unexpected startup events: {events:?}");
    assert!(matches!(
        events.first(),
        Some(EngineEvent::RunFailed { .. })
    ));
}

#[cfg(windows)]
#[tokio::test]
async fn pi_rejects_a_pre_acceptance_event_flood_without_filling_the_sink() {
    let root = tempfile::tempdir().unwrap();
    let adapter = pi_fixture_adapter(root.path(), FixtureScenario::PreAcceptanceFlood).await;
    let (sender, receiver) = mpsc::channel(1);
    let mut receiver = Some(receiver);
    let start_adapter = Arc::clone(&adapter);
    let mut start = tokio::spawn(async move {
        start_adapter
            .start(
                context("run-pre-acceptance-flood", 0),
                input("flood"),
                sender,
            )
            .await
    });
    wait_for_fixture_marker(root.path(), "pre-acceptance-flood-emitted").await;

    let result = match tokio::time::timeout(Duration::from_secs(2), &mut start).await {
        Ok(result) => result.unwrap(),
        Err(_) => {
            drop(receiver.take());
            let _ = bounded("blocked pre-acceptance flood cleanup", start).await;
            panic!("pre-acceptance flood filled the public event sink");
        }
    };
    let mut receiver = receiver.unwrap();

    assert!(matches!(result, Err(EngineError::Start(_))));
    assert!(matches!(
        receive_event(&mut receiver).await,
        EngineEvent::RunFailed { .. }
    ));
    assert!(receiver.recv().await.is_none());
}

#[cfg(windows)]
#[tokio::test]
async fn pi_runtime_context_redacts_sensitive_raw_events() {
    let root = tempfile::tempdir().unwrap();
    let adapter = pi_fixture_adapter(root.path(), FixtureScenario::SensitiveRawEvent).await;
    let (sender, receiver) = mpsc::channel(8);
    adapter
        .start(context("run-sensitive-raw", 0), input("raw"), sender)
        .await
        .unwrap();
    let events = collect_closed(receiver).await;
    let (kind, payload_json) = events
        .iter()
        .find_map(|event| match event {
            EngineEvent::RawEngineEvent { kind, payload_json } => Some((kind, payload_json)),
            _ => None,
        })
        .expect("fixture must emit a raw event");
    let payload: serde_json::Value = serde_json::from_str(payload_json).unwrap();

    assert_eq!(kind, "future-[REDACTED]");
    assert_eq!(payload["type"], "future-[REDACTED]");
    assert_eq!(payload["token"], "[REDACTED]");
    for field in [
        "api",
        "work",
        "workspace",
        "sessionPath",
        "runtimePath",
        "model",
    ] {
        assert_eq!(payload["values"][field], "[REDACTED]", "field {field}");
    }
}

#[cfg(windows)]
#[tokio::test]
async fn pi_reserves_a_run_before_concurrent_startup() {
    let root = tempfile::tempdir().unwrap();
    let adapter = pi_fixture_adapter(root.path(), FixtureScenario::StartupBarrier).await;
    let (first_sender, mut first_receiver) = mpsc::channel(8);
    let first_adapter = Arc::clone(&adapter);
    let first = tokio::spawn(async move {
        first_adapter
            .start(
                context("run-concurrent-start", 0),
                input("first"),
                first_sender,
            )
            .await
    });
    wait_for_fixture_marker(root.path(), "startup-entered.log").await;

    let (second_sender, _second_receiver) = mpsc::channel(8);
    let second = tokio::time::timeout(
        Duration::from_millis(250),
        adapter.start(
            context("run-concurrent-start", 0),
            input("second"),
            second_sender,
        ),
    )
    .await;

    std::fs::write(fixture_marker(root.path(), "startup-release"), b"release").unwrap();
    bounded("first Pi startup", first).await.unwrap().unwrap();
    assert!(matches!(
        receive_event(&mut first_receiver).await,
        EngineEvent::RunStarted { .. }
    ));
    bounded("first Pi abort", adapter.abort("run-concurrent-start"))
        .await
        .unwrap();
    collect_closed(first_receiver).await;

    assert!(matches!(second, Ok(Err(EngineError::Start(_)))));
}

#[cfg(windows)]
#[tokio::test]
async fn pi_abort_waits_for_child_exit_and_cleanup_acknowledgement() {
    let root = tempfile::tempdir().unwrap();
    let adapter = pi_fixture_adapter(root.path(), FixtureScenario::AbortBarrier).await;
    let (sender, mut receiver) = mpsc::channel(8);
    adapter
        .start(context("run-abort-ack", 0), input("hold"), sender)
        .await
        .unwrap();
    assert!(matches!(
        receive_event(&mut receiver).await,
        EngineEvent::RunStarted { .. }
    ));

    let abort_adapter = Arc::clone(&adapter);
    let abort = tokio::spawn(async move { abort_adapter.abort("run-abort-ack").await });
    let returned_before_release = abort_returned_before_fixture_marker(root.path(), &abort).await;
    std::fs::write(fixture_marker(root.path(), "abort-release"), b"release").unwrap();
    bounded("acknowledged Pi abort", abort)
        .await
        .unwrap()
        .unwrap();
    let events = collect_closed(receiver).await;
    assert!(matches!(events.last(), Some(EngineEvent::RunFailed { .. })));

    assert!(!returned_before_release, "abort returned before child exit");
    assert!(!root.path().join("runtime/run-abort-ack/agent").exists());
}

#[cfg(windows)]
#[tokio::test]
async fn pi_abort_interrupts_sink_backpressure_and_delivers_a_terminal_after_cleanup() {
    let root = tempfile::tempdir().unwrap();
    let adapter = pi_fixture_adapter(root.path(), FixtureScenario::Backpressure).await;
    let (sender, mut receiver) = mpsc::channel(1);
    adapter
        .start(context("run-abort-backpressure", 0), input("hold"), sender)
        .await
        .unwrap();
    wait_for_fixture_marker(root.path(), "backpressure-emitted").await;
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }

    let abort_adapter = Arc::clone(&adapter);
    let abort = tokio::spawn(async move { abort_adapter.abort("run-abort-backpressure").await });
    let abort_observed = tokio::time::timeout(Duration::from_secs(2), async {
        let marker = fixture_marker(root.path(), "abort-entered");
        while !marker.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .is_ok();
    if !abort_observed {
        drop(receiver);
        let _ = bounded("backpressured Pi cleanup", abort).await;
        panic!("Pi abort could not interrupt a blocked event sink");
    }
    bounded("backpressured Pi resource cleanup", async {
        while root
            .path()
            .join("runtime/run-abort-backpressure/agent")
            .exists()
        {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(
        !abort.is_finished(),
        "Pi abort acknowledged before terminal delivery"
    );

    let started = receive_event(&mut receiver).await;
    let terminal = receive_event(&mut receiver).await;
    assert!(matches!(started, EngineEvent::RunStarted { .. }));
    assert!(matches!(terminal, EngineEvent::RunFailed { .. }));
    bounded("backpressured Pi abort", abort)
        .await
        .unwrap()
        .unwrap();
    assert!(receiver.recv().await.is_none());
}

#[cfg(windows)]
#[tokio::test]
async fn pi_abort_fails_boundedly_when_capacity_one_sink_is_never_drained() {
    let root = tempfile::tempdir().unwrap();
    let adapter = pi_fixture_adapter(root.path(), FixtureScenario::Backpressure).await;
    let (sender, receiver) = mpsc::channel(1);
    adapter
        .start(
            context("run-never-drained-terminal", 0),
            input("hold"),
            sender,
        )
        .await
        .unwrap();
    wait_for_fixture_marker(root.path(), "backpressure-emitted").await;
    bounded("capacity-one Pi sink to become full", async {
        while receiver.len() != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await;

    let error = tokio::time::timeout(
        Duration::from_secs(5),
        adapter.abort("run-never-drained-terminal"),
    )
    .await
    .expect("terminal delivery failure must complete within its deadline")
    .expect_err("an undelivered terminal must not acknowledge abort");
    assert!(
        matches!(error, EngineError::Start(ref message) if message.contains("delivery")),
        "unexpected abort error: {error:?}"
    );
    assert!(
        !root
            .path()
            .join("runtime/run-never-drained-terminal/agent")
            .exists()
    );

    drop(receiver);
    let (retry_sender, mut retry_receiver) = mpsc::channel(8);
    adapter
        .start(
            context("run-never-drained-terminal", 0),
            input("retry"),
            retry_sender,
        )
        .await
        .expect("failed terminal delivery must remove the active generation");
    assert!(matches!(
        receive_event(&mut retry_receiver).await,
        EngineEvent::RunStarted { .. }
    ));
    adapter.abort("run-never-drained-terminal").await.unwrap();
    assert!(matches!(
        receive_event(&mut retry_receiver).await,
        EngineEvent::RunFailed { .. }
    ));
}

#[cfg(windows)]
#[tokio::test]
async fn pi_abort_interrupts_error_terminal_backpressure() {
    for (scenario, run_id) in [
        (
            FixtureScenario::MalformedBackpressure,
            "run-malformed-backpressure",
        ),
        (
            FixtureScenario::InvalidUtf8Backpressure,
            "run-invalid-utf8-backpressure",
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let adapter = pi_fixture_adapter(root.path(), scenario).await;
        let (sender, mut receiver) = mpsc::channel(1);
        adapter
            .start(context(run_id, 0), input("hold"), sender)
            .await
            .unwrap();
        wait_for_fixture_marker(root.path(), "backpressure-emitted").await;
        for _ in 0..100 {
            tokio::task::yield_now().await;
        }

        let abort_adapter = Arc::clone(&adapter);
        let owned_run_id = run_id.to_owned();
        let abort = tokio::spawn(async move { abort_adapter.abort(&owned_run_id).await });
        let abort_observed = tokio::time::timeout(Duration::from_secs(2), async {
            let marker = fixture_marker(root.path(), "abort-entered");
            while !marker.exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .is_ok();
        if !abort_observed {
            drop(receiver);
            let _ = bounded("error-terminal Pi cleanup", abort).await;
            panic!("Pi abort could not interrupt a blocked error terminal for {run_id}");
        }

        assert!(matches!(
            receive_event(&mut receiver).await,
            EngineEvent::RunStarted { .. }
        ));
        assert!(matches!(
            receive_event(&mut receiver).await,
            EngineEvent::RunFailed { .. }
        ));
        bounded("error-terminal Pi abort", abort)
            .await
            .unwrap()
            .unwrap();
        assert!(receiver.recv().await.is_none());
    }
}

#[cfg(windows)]
#[tokio::test]
async fn pi_completion_reaps_a_descendant_that_inherits_stderr() {
    let root = tempfile::tempdir().unwrap();
    let adapter = pi_fixture_adapter(root.path(), FixtureScenario::InheritedStderrComplete).await;
    let (sender, mut receiver) = mpsc::channel(8);
    adapter
        .start(
            context("run-inherited-stderr", 0),
            input("complete"),
            sender,
        )
        .await
        .unwrap();
    wait_for_fixture_marker(root.path(), "stderr-holder-entered").await;
    let descendant_pid = fixture_descendant_pid(root.path());
    assert!(matches!(
        receive_event(&mut receiver).await,
        EngineEvent::RunStarted { .. }
    ));
    assert!(matches!(
        receive_event(&mut receiver).await,
        EngineEvent::RunCompleted { .. }
    ));
    let descendant_survived_terminal = process_is_alive(descendant_pid);
    assert!(receiver.recv().await.is_none());
    release_fixture_descendant_if_alive(root.path(), descendant_pid).await;

    assert!(
        !descendant_survived_terminal,
        "Pi emitted RunCompleted while a descendant was still alive"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn pi_abort_reaps_a_descendant_that_inherits_stderr() {
    let root = tempfile::tempdir().unwrap();
    let adapter = pi_fixture_adapter(root.path(), FixtureScenario::InheritedStderrHold).await;
    let (sender, mut receiver) = mpsc::channel(8);
    adapter
        .start(
            context("run-inherited-stderr-abort", 0),
            input("hold"),
            sender,
        )
        .await
        .unwrap();
    wait_for_fixture_marker(root.path(), "stderr-holder-entered").await;
    let descendant_pid = fixture_descendant_pid(root.path());
    assert!(matches!(
        receive_event(&mut receiver).await,
        EngineEvent::RunStarted { .. }
    ));

    let abort_adapter = Arc::clone(&adapter);
    bounded(
        "inherited-stderr Pi abort",
        abort_adapter.abort("run-inherited-stderr-abort"),
    )
    .await
    .unwrap();
    collect_closed(receiver).await;
    let descendant_survived = process_is_alive(descendant_pid);
    release_fixture_descendant_if_alive(root.path(), descendant_pid).await;

    assert!(
        !descendant_survived,
        "Pi abort returned while a descendant was still alive"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn dropping_pi_reaps_a_descendant_that_inherits_stderr() {
    let root = tempfile::tempdir().unwrap();
    let adapter = pi_fixture_adapter(root.path(), FixtureScenario::InheritedStderrHold).await;
    let (sender, mut receiver) = mpsc::channel(8);
    adapter
        .start(
            context("run-inherited-stderr-drop", 0),
            input("hold"),
            sender,
        )
        .await
        .unwrap();
    wait_for_fixture_marker(root.path(), "stderr-holder-entered").await;
    let descendant_pid = fixture_descendant_pid(root.path());
    assert!(matches!(
        receive_event(&mut receiver).await,
        EngineEvent::RunStarted { .. }
    ));
    let agent_directory = root.path().join("runtime/run-inherited-stderr-drop/agent");
    assert!(agent_directory.exists());
    drop(receiver);
    drop(adapter);

    bounded("inherited-stderr Pi drop cleanup", async {
        while agent_directory.exists() {
            tokio::task::yield_now().await;
        }
    })
    .await;
    let descendant_survived = process_is_alive(descendant_pid);
    release_fixture_descendant_if_alive(root.path(), descendant_pid).await;

    assert!(
        !descendant_survived,
        "dropping Pi returned while a descendant was still alive"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn pi_rejects_restart_while_abort_cleanup_is_in_progress() {
    let root = tempfile::tempdir().unwrap();
    let adapter = pi_fixture_adapter(root.path(), FixtureScenario::AbortBarrier).await;
    let (sender, mut receiver) = mpsc::channel(8);
    adapter
        .start(context("run-abort-restart", 0), input("hold"), sender)
        .await
        .unwrap();
    receive_event(&mut receiver).await;

    let abort_adapter = Arc::clone(&adapter);
    let abort = tokio::spawn(async move { abort_adapter.abort("run-abort-restart").await });
    abort_returned_before_fixture_marker(root.path(), &abort).await;
    let (restart_sender, restart_receiver) = mpsc::channel(8);
    let restart = bounded(
        "restart during Pi abort",
        adapter.start(
            context("run-abort-restart", 0),
            input("restart"),
            restart_sender,
        ),
    )
    .await;

    std::fs::write(fixture_marker(root.path(), "abort-release"), b"release").unwrap();
    bounded("Pi abort cleanup", abort).await.unwrap().unwrap();
    collect_closed(receiver).await;
    if restart.is_ok() {
        let _ = adapter.abort("run-abort-restart").await;
        collect_closed(restart_receiver).await;
    }

    assert!(matches!(restart, Err(EngineError::Start(_))));
}

#[cfg(windows)]
#[tokio::test]
async fn pi_allows_same_run_restart_after_abort_cleanup_acknowledgement() {
    let root = tempfile::tempdir().unwrap();
    let adapter = pi_fixture_adapter(root.path(), FixtureScenario::AbortBarrier).await;
    let (first_sender, mut first_receiver) = mpsc::channel(8);
    adapter
        .start(
            context("run-abort-then-restart", 0),
            input("first"),
            first_sender,
        )
        .await
        .unwrap();
    receive_event(&mut first_receiver).await;

    let first_abort_adapter = Arc::clone(&adapter);
    let first_abort =
        tokio::spawn(async move { first_abort_adapter.abort("run-abort-then-restart").await });
    abort_returned_before_fixture_marker(root.path(), &first_abort).await;
    std::fs::write(fixture_marker(root.path(), "abort-release"), b"release").unwrap();
    bounded("first acknowledged Pi abort", first_abort)
        .await
        .unwrap()
        .unwrap();
    collect_closed(first_receiver).await;

    let (second_sender, mut second_receiver) = mpsc::channel(8);
    adapter
        .start(
            context("run-abort-then-restart", 0),
            input("second"),
            second_sender,
        )
        .await
        .unwrap();
    assert!(matches!(
        receive_event(&mut second_receiver).await,
        EngineEvent::RunStarted { .. }
    ));
    bounded(
        "second acknowledged Pi abort",
        adapter.abort("run-abort-then-restart"),
    )
    .await
    .unwrap();
    collect_closed(second_receiver).await;
}

#[cfg(windows)]
#[tokio::test]
async fn cancelling_a_pi_abort_caller_does_not_strand_completion() {
    let root = tempfile::tempdir().unwrap();
    let adapter = pi_fixture_adapter(root.path(), FixtureScenario::AbortBarrier).await;
    let (sender, mut receiver) = mpsc::channel(8);
    adapter
        .start(
            context("run-cancelled-abort-caller", 0),
            input("hold"),
            sender,
        )
        .await
        .unwrap();
    receive_event(&mut receiver).await;

    let abort_adapter = Arc::clone(&adapter);
    let abort =
        tokio::spawn(async move { abort_adapter.abort("run-cancelled-abort-caller").await });
    wait_for_fixture_marker(root.path(), "abort-entered").await;
    assert!(!abort.is_finished());
    abort.abort();
    assert!(abort.await.unwrap_err().is_cancelled());
    std::fs::write(fixture_marker(root.path(), "abort-release"), b"release").unwrap();
    collect_closed(receiver).await;

    let mut restarted_receiver = bounded("restart after cancelled Pi abort caller", async {
        loop {
            let (restart_sender, restart_receiver) = mpsc::channel(8);
            match adapter
                .start(
                    context("run-cancelled-abort-caller", 0),
                    input("restart"),
                    restart_sender,
                )
                .await
            {
                Ok(_) => break restart_receiver,
                Err(EngineError::Start(_)) => tokio::task::yield_now().await,
                Err(error) => panic!("unexpected restart error: {error}"),
            }
        }
    })
    .await;
    assert!(matches!(
        receive_event(&mut restarted_receiver).await,
        EngineEvent::RunStarted { .. }
    ));
    bounded(
        "cleanup restarted Pi run",
        adapter.abort("run-cancelled-abort-caller"),
    )
    .await
    .unwrap();
    collect_closed(restarted_receiver).await;
}

#[cfg(windows)]
#[tokio::test]
async fn dropping_pi_after_receiver_close_releases_the_node_process() {
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().to_owned();
    let adapter = pi_fixture_adapter(&root_path, FixtureScenario::HoldWithWatchdog).await;
    let (sender, mut receiver) = mpsc::channel(8);
    adapter
        .start(context("run-drop-cleanup", 0), input("hold"), sender)
        .await
        .unwrap();
    receive_event(&mut receiver).await;
    drop(receiver);
    drop(adapter);

    let abort_won = bounded("Pi drop abort before fixture watchdog", async {
        loop {
            if fixture_marker(&root_path, "abort-entered").exists() {
                break true;
            }
            if fixture_marker(&root_path, "watchdog-fired").exists() {
                break false;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    bounded("dropped Pi process and fixture cleanup", async {
        loop {
            if std::fs::remove_dir_all(&root_path).is_ok() || !root_path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;

    assert!(abort_won, "dropping Pi relied on the fixture watchdog");
}

#[tokio::test]
async fn dropping_a_holding_fake_releases_its_run_task() {
    let adapter = FakeEngineAdapter::configured(
        FakeEngineConfig::new(full_capabilities())
            .with_run_behavior(FakeRunBehavior::HoldUntilAbort),
    );
    let (sender, mut receiver) = mpsc::channel(8);
    adapter
        .start(context("run-drop-holding-fake", 0), input("hold"), sender)
        .await
        .unwrap();
    assert!(matches!(
        receive_event(&mut receiver).await,
        EngineEvent::RunStarted { .. }
    ));

    drop(adapter);

    let events = tokio::time::timeout(Duration::from_secs(1), collect_closed(receiver))
        .await
        .expect("dropping the fake adapter left its HoldUntilAbort task alive");
    assert_one_terminal(&events);
    assert!(matches!(events.last(), Some(EngineEvent::RunFailed { .. })));
}

#[cfg(windows)]
#[tokio::test]
async fn cancelling_pi_startup_sends_abort_and_reaps_the_node_process() {
    let root = tempfile::tempdir().unwrap();
    let root_path = root.path().to_owned();
    let adapter = pi_fixture_adapter(&root_path, FixtureScenario::StartupBarrier).await;
    let (sender, _receiver) = mpsc::channel(8);
    let start_adapter = Arc::clone(&adapter);
    let start = tokio::spawn(async move {
        start_adapter
            .start(context("run-startup-cancel", 0), input("hold"), sender)
            .await
    });
    wait_for_fixture_marker(&root_path, "startup-entered.log").await;

    start.abort();
    assert!(start.await.unwrap_err().is_cancelled());

    wait_for_fixture_marker(&root_path, "abort-entered").await;
    bounded("cancelled Pi startup cleanup", async {
        loop {
            if std::fs::remove_dir_all(&root_path).is_ok() || !root_path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
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
        assert_factory_contract(factory.as_ref()).await;
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
