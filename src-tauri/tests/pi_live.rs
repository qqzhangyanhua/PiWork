#![cfg(windows)]

use std::{path::PathBuf, sync::Arc, time::Duration};

use piwork_lib::{
    domain::work::PermissionMode,
    engine::{
        EngineAdapter, EngineEvent, EngineInput, EngineRunContext, EngineRunIdentity,
        pi::PiEngineAdapter,
    },
    model::{ModelConfigurationRepository, ModelService},
};

#[tokio::test]
#[ignore = "uses the locally saved PiWork provider credential"]
async fn saved_provider_drives_a_real_pi_tool_loop() {
    let database_path = std::env::var_os("PIWORK_LIVE_DATABASE_PATH")
        .map(PathBuf::from)
        .expect("PIWORK_LIVE_DATABASE_PATH is required");
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}?mode=ro",
        database_path.to_string_lossy().replace('\\', "/")
    ))
    .await
    .unwrap();
    let model_service = Arc::new(
        ModelService::production(ModelConfigurationRepository::new(pool))
            .expect("production model service"),
    );
    let workspace = tempfile::tempdir().unwrap();
    let external_workspace = std::env::var_os("PIWORK_LIVE_WORKSPACE_PATH").map(PathBuf::from);
    let workspace_path = external_workspace
        .clone()
        .unwrap_or_else(|| workspace.path().to_path_buf());
    let runtime = tempfile::tempdir().unwrap();
    let sessions = tempfile::tempdir().unwrap();
    let engine = PiEngineAdapter::production(
        model_service,
        sessions.path().to_path_buf(),
        runtime.path().to_path_buf(),
    )
    .unwrap();
    let work_id = "00000000-0000-0000-0000-000000000111";
    let context = EngineRunContext::new(
        EngineRunIdentity::new(
            work_id.into(),
            "00000000-0000-0000-0000-000000000222".into(),
            "00000000-0000-0000-0000-000000000444".into(),
            "agent-instance:piwork-lead".into(),
            work_id.into(),
            0,
        )
        .unwrap(),
        workspace_path.clone(),
        PermissionMode::AutoExecute,
        None,
        PermissionMode::AutoExecute,
    )
    .unwrap();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(64);

    let prompt = if external_workspace.is_some() {
        "Use Pi read-only tools to inspect the workspace root and report the names of two top-level entries. Do not modify any files."
    } else {
        "Use Pi tools to create pi-agent-proof.txt containing exactly PI_AGENT_OK. Read it back, then run a shell command that prints PI_COMMAND_OK. Do not only describe the steps; perform them."
    };
    let session = engine
        .start(
            context,
            EngineInput {
                message: prompt.into(),
                images: Vec::new(),
                documents: Vec::new(),
            },
            sender,
        )
        .await
        .unwrap();
    assert_eq!(session.engine_kind, "pi_rpc");

    let events = tokio::time::timeout(Duration::from_secs(240), async {
        let mut events = Vec::new();
        while let Some(event) = receiver.recv().await {
            let terminal = event.is_terminal();
            events.push(event);
            if terminal {
                break;
            }
        }
        events
    })
    .await
    .expect("Pi tool loop timed out");

    assert!(
        events
            .iter()
            .any(|event| matches!(event, EngineEvent::ToolStarted { .. }))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, EngineEvent::ToolFinished { success: true, .. }))
    );
    assert!(matches!(
        events.last(),
        Some(EngineEvent::RunCompleted { .. })
    ));
    if external_workspace.is_some() {
        return;
    }
    assert_eq!(
        std::fs::read_to_string(workspace_path.join("pi-agent-proof.txt"))
            .unwrap()
            .trim(),
        "PI_AGENT_OK"
    );

    tokio::time::sleep(Duration::from_millis(500)).await;
    let context = EngineRunContext::new(
        EngineRunIdentity::new(
            work_id.into(),
            "00000000-0000-0000-0000-000000000333".into(),
            "00000000-0000-0000-0000-000000000555".into(),
            "agent-instance:piwork-lead".into(),
            work_id.into(),
            0,
        )
        .unwrap(),
        workspace_path.clone(),
        PermissionMode::AutoExecute,
        None,
        PermissionMode::AutoExecute,
    )
    .unwrap();
    let (sender, mut receiver) = tokio::sync::mpsc::channel(64);
    engine
        .start(
            context,
            EngineInput {
                message: "Continue this Work. Use tools to replace the proof file contents with exactly PI_AGENT_OK_SECOND, then read it back.".into(),
                images: Vec::new(),
                documents: Vec::new(),
            },
            sender,
        )
        .await
        .unwrap();
    let second_events = tokio::time::timeout(Duration::from_secs(240), async {
        let mut events = Vec::new();
        while let Some(event) = receiver.recv().await {
            let terminal = event.is_terminal();
            events.push(event);
            if terminal {
                break;
            }
        }
        events
    })
    .await
    .expect("second Pi tool loop timed out");
    assert!(
        second_events
            .iter()
            .any(|event| matches!(event, EngineEvent::ToolStarted { .. }))
    );
    assert!(matches!(
        second_events.last(),
        Some(EngineEvent::RunCompleted { .. })
    ));
    assert_eq!(
        std::fs::read_to_string(workspace_path.join("pi-agent-proof.txt"))
            .unwrap()
            .trim(),
        "PI_AGENT_OK_SECOND"
    );
}
