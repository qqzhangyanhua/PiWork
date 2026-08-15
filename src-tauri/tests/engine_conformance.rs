use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use piwork_lib::{
    domain::{
        event::{LivenessState, PermissionOutcome},
        work::PermissionMode,
    },
    engine::{
        EngineAdapter, EngineCapabilities, EngineError, EngineEvent, EngineInput, EngineRunContext,
        EngineSessionRef,
        fake::{FakeEngineAdapter, FakeEngineConfig, FakeRunBehavior, FakeStartBarrier},
    },
};
use tokio::sync::mpsc;

fn input(message: &str) -> EngineInput {
    EngineInput {
        message: message.into(),
        images: Vec::new(),
        documents: Vec::new(),
    }
}

fn context(run_id: &str, generation: u32) -> EngineRunContext {
    EngineRunContext {
        work_id: "work-1".into(),
        run_id: run_id.into(),
        root_path: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        permission_mode: PermissionMode::Balanced,
        assignment_id: "assignment-1".into(),
        agent_instance_id: "agent-instance-1".into(),
        agent_session_id: "agent-session-1".into(),
        session_generation: generation,
        resolved_model_configuration_id: Some("model-configuration-1".into()),
        effective_permission: PermissionMode::AskEveryStep,
    }
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
    EngineCapabilities {
        session_resume: false,
        session_rotate: false,
        native_steer: false,
        cancel: false,
        thought_stream: false,
        plan_updates: false,
        permission_requests: false,
        tool_progress: false,
        usage_reporting: false,
        parallel_tool_calls: false,
    }
}

fn configured_fake(
    capabilities: EngineCapabilities,
    behavior: FakeRunBehavior,
) -> FakeEngineAdapter {
    FakeEngineAdapter::configured(
        FakeEngineConfig::new(capabilities)
            .with_session_id("fake-session-1")
            .with_run_behavior(behavior),
    )
}

async fn collect_closed(mut receiver: mpsc::Receiver<EngineEvent>) -> Vec<EngineEvent> {
    let mut events = Vec::new();
    while let Some(event) = receiver.recv().await {
        events.push(event);
    }
    events
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

struct DefaultsOnlyAdapter;

#[async_trait]
impl EngineAdapter for DefaultsOnlyAdapter {
    fn kind(&self) -> &'static str {
        "defaults-only"
    }

    fn capabilities(&self) -> EngineCapabilities {
        minimal_capabilities()
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
    let adapter = DefaultsOnlyAdapter;
    let (sender, _receiver) = mpsc::channel(1);

    assert!(matches!(
        adapter
            .resume(context("run-resume", 1), input("resume"), sender)
            .await,
        Err(EngineError::Unsupported("session_resume"))
    ));
    assert!(matches!(
        adapter
            .rotate(context("run-rotate", 1), "context limit")
            .await,
        Err(EngineError::Unsupported("session_rotate"))
    ));
    assert!(matches!(
        adapter.steer("run-steer", input("change direction")).await,
        Err(EngineError::Unsupported("native_steer"))
    ));
    assert!(matches!(
        adapter.abort("run-abort").await,
        Err(EngineError::Unsupported("cancel"))
    ));
}

#[tokio::test]
async fn fake_factory_start_is_barrier_controlled_and_observable() {
    let barrier = Arc::new(FakeStartBarrier::new());
    let adapter = Arc::new(FakeEngineAdapter::configured(
        FakeEngineConfig::new(full_capabilities())
            .with_session_id("barrier-session")
            .with_start_barrier(Arc::clone(&barrier)),
    ));
    let (sender, receiver) = mpsc::channel(32);
    let start_adapter = Arc::clone(&adapter);
    let start = tokio::spawn(async move {
        start_adapter
            .start(context("run-barrier", 1), input("start"), sender)
            .await
    });

    barrier.wait_until_entered().await;
    assert!(
        !start.is_finished(),
        "start returned before its barrier opened"
    );
    barrier.release();

    let session = start.await.unwrap().unwrap();
    assert_eq!(session.session_id, "barrier-session");
    let events = collect_closed(receiver).await;
    assert_one_terminal(&events);
    assert_eq!(
        adapter.observations().await.started_run_ids,
        ["run-barrier"]
    );
}

#[tokio::test]
async fn full_fake_adapter_orders_events_and_declares_every_event_capability_truthfully() {
    let adapter = configured_fake(full_capabilities(), FakeRunBehavior::Complete);
    assert_eq!(adapter.capabilities(), full_capabilities());
    let (sender, receiver) = mpsc::channel(32);

    adapter
        .start(context("run-full", 1), input("build it"), sender)
        .await
        .unwrap();
    let events = collect_closed(receiver).await;

    assert_eq!(
        events.iter().map(EngineEvent::kind).collect::<Vec<_>>(),
        [
            "run_started",
            "thought_delta",
            "plan_changed",
            "assistant_delta",
            "tool_started",
            "tool_started",
            "tool_progress",
            "permission_requested",
            "permission_resolved",
            "tool_finished",
            "tool_finished",
            "usage_updated",
            "assistant_delta",
            "run_completed",
        ]
    );
    assert_one_terminal(&events);
    assert!(events.iter().any(|event| matches!(
        event,
        EngineEvent::PermissionResolved {
            outcome: PermissionOutcome::AllowedOnce,
            ..
        }
    )));
}

#[tokio::test]
async fn minimal_fake_adapter_never_emits_events_for_disabled_capabilities() {
    let adapter = configured_fake(minimal_capabilities(), FakeRunBehavior::Complete);
    let (sender, receiver) = mpsc::channel(16);
    adapter
        .start(context("run-minimal", 1), input("build it"), sender)
        .await
        .unwrap();
    let events = collect_closed(receiver).await;

    assert_one_terminal(&events);
    assert!(!events.iter().any(|event| matches!(
        event,
        EngineEvent::ThoughtDelta { .. }
            | EngineEvent::PlanChanged { .. }
            | EngineEvent::PermissionRequested { .. }
            | EngineEvent::PermissionResolved { .. }
            | EngineEvent::ToolProgress { .. }
            | EngineEvent::UsageUpdated { .. }
    )));
    let active_tools = events.iter().try_fold(0_i32, |active, event| match event {
        EngineEvent::ToolStarted { .. } => (active == 0).then_some(active + 1),
        EngineEvent::ToolFinished { .. } => (active == 1).then_some(active - 1),
        _ => Some(active),
    });
    assert_eq!(active_tools, Some(0), "parallel tool calls were fabricated");

    let (resume_sender, _resume_receiver) = mpsc::channel(1);
    assert!(matches!(
        adapter
            .resume(
                context("run-disabled-resume", 1),
                input("resume"),
                resume_sender,
            )
            .await,
        Err(EngineError::Unsupported("session_resume"))
    ));
    assert!(matches!(
        adapter
            .rotate(context("run-disabled-rotate", 1), "rotate")
            .await,
        Err(EngineError::Unsupported("session_rotate"))
    ));
    assert!(matches!(
        adapter.steer("run-disabled-steer", input("steer")).await,
        Err(EngineError::Unsupported("native_steer"))
    ));
    assert!(matches!(
        adapter.abort("run-disabled-abort").await,
        Err(EngineError::Unsupported("cancel"))
    ));
}

#[tokio::test]
async fn fake_resume_reuses_the_session_and_emits_one_ordered_terminal_run() {
    let adapter = configured_fake(full_capabilities(), FakeRunBehavior::Complete);
    let (sender, receiver) = mpsc::channel(32);

    let session = adapter
        .resume(context("run-resume", 4), input("continue"), sender)
        .await
        .unwrap();
    let events = collect_closed(receiver).await;

    assert_eq!(session.session_id, "fake-session-1");
    assert_eq!(
        events.first().map(EngineEvent::kind),
        Some("session_changed")
    );
    assert_one_terminal(&events);
    let observations = adapter.observations().await;
    assert_eq!(observations.resumed_run_ids, ["run-resume"]);
    assert!(observations.started_run_ids.is_empty());
}

#[tokio::test]
async fn fake_abort_stops_an_active_run_and_is_observable() {
    let mut capabilities = minimal_capabilities();
    capabilities.cancel = true;
    let adapter = configured_fake(capabilities, FakeRunBehavior::HoldUntilAbort);
    let (sender, mut receiver) = mpsc::channel(16);
    adapter
        .start(context("run-abort", 1), input("wait"), sender)
        .await
        .unwrap();
    assert_eq!(receiver.recv().await.unwrap().kind(), "run_started");

    adapter.abort("run-abort").await.unwrap();
    let mut events = vec![EngineEvent::RunStarted {
        model_label: "Fake model".into(),
    }];
    events.extend(collect_closed(receiver).await);

    assert_one_terminal(&events);
    assert_eq!(adapter.observations().await.aborted_run_ids, ["run-abort"]);
}

#[tokio::test]
async fn rotate_and_native_steer_are_observable_when_declared() {
    let adapter = configured_fake(full_capabilities(), FakeRunBehavior::HoldUntilAbort);

    let rotated = adapter
        .rotate(context("run-rotate", 8), "context limit")
        .await
        .unwrap();
    let (sender, mut receiver) = mpsc::channel(16);
    adapter
        .start(context("run-steer", 8), input("initial direction"), sender)
        .await
        .unwrap();
    assert_eq!(receiver.recv().await.unwrap().kind(), "run_started");
    adapter
        .steer("run-steer", input("use the safer approach"))
        .await
        .unwrap();
    adapter.abort("run-steer").await.unwrap();
    collect_closed(receiver).await;

    assert_ne!(rotated.session_id, "fake-session-1");
    let observations = adapter.observations().await;
    assert_eq!(
        observations.rotations,
        [("run-rotate".into(), "context limit".into())]
    );
    assert_eq!(
        observations.steers,
        [("run-steer".into(), "use the safer approach".into())]
    );
}

#[tokio::test]
async fn unsupported_native_steer_degrades_explicitly_to_abort_and_restart() {
    let mut capabilities = minimal_capabilities();
    capabilities.cancel = true;
    let adapter = configured_fake(capabilities, FakeRunBehavior::HoldUntilAbort);
    let (sender, mut receiver) = mpsc::channel(16);
    adapter
        .start(context("run-old", 1), input("old direction"), sender)
        .await
        .unwrap();
    assert_eq!(receiver.recv().await.unwrap().kind(), "run_started");

    assert!(matches!(
        adapter.steer("run-old", input("new direction")).await,
        Err(EngineError::Unsupported("native_steer"))
    ));
    adapter.abort("run-old").await.unwrap();
    collect_closed(receiver).await;

    let replacement = configured_fake(capabilities, FakeRunBehavior::Complete);
    let (replacement_sender, replacement_receiver) = mpsc::channel(16);
    replacement
        .start(
            context("run-replacement", 1),
            input("old direction\n\nnew direction"),
            replacement_sender,
        )
        .await
        .unwrap();
    assert_one_terminal(&collect_closed(replacement_receiver).await);

    let old_observations = adapter.observations().await;
    assert!(old_observations.steers.is_empty());
    assert_eq!(old_observations.aborted_run_ids, ["run-old"]);
    assert_eq!(
        replacement.observations().await.started_run_ids,
        ["run-replacement"]
    );
}

#[tokio::test]
async fn crash_and_liveness_timeout_each_close_with_one_failure_terminal() {
    for (run_id, behavior, expected_liveness) in [
        ("run-crash", FakeRunBehavior::Crash, false),
        ("run-timeout", FakeRunBehavior::LivenessTimeout, true),
    ] {
        let adapter = configured_fake(minimal_capabilities(), behavior);
        let (sender, receiver) = mpsc::channel(16);
        adapter
            .start(context(run_id, 1), input("run"), sender)
            .await
            .unwrap();
        let events = collect_closed(receiver).await;

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
