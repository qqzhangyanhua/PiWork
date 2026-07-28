use std::sync::Arc;

use async_trait::async_trait;
use piwork_lib::{
    app_state::AppState,
    domain::{
        event::{WorkEventEnvelope, WorkEventPayload},
        work::{CreateWorkInput, PermissionMode, RunStatus, WorkStatus},
    },
    engine::{
        EngineAdapter, EngineError, EngineEvent, EngineRunContext, EngineSessionRef,
        fake::FakeEngineAdapter, publisher::ChannelEventPublisher, supervisor::EngineSupervisor,
    },
    storage::sqlite::Database,
    work::{repository::WorkRepository, service::WorkService},
};
use tokio::sync::mpsc;

struct TestHarness {
    _temporary_directory: tempfile::TempDir,
    repository: WorkRepository,
    workspace_path: std::path::PathBuf,
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
            })
            .await
            .unwrap()
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
async fn event_is_persisted_before_it_is_published() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Persist before publish").await;
    let (publisher, mut published) = ChannelEventPublisher::channel(16);
    let supervisor = EngineSupervisor::new(
        harness.repository.clone(),
        Arc::new(FakeEngineAdapter::new(std::time::Duration::ZERO)),
        Arc::new(publisher),
    );

    let run = supervisor
        .start(&work.summary.id, "Build it")
        .await
        .unwrap();
    let event = published.recv().await.unwrap();
    let persisted = harness.repository.events_for_run(&run.id).await.unwrap();

    assert!(persisted.iter().any(|candidate| {
        candidate.run_id == event.run_id && candidate.sequence == event.sequence
    }));
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
    ));
    let service = WorkService::with_supervisor(harness.repository, supervisor);

    let run = tokio::time::timeout(
        std::time::Duration::from_millis(150),
        service.start_work(&work.summary.id, "Build it"),
    )
    .await
    .expect("start_work waited for the engine event stream")
    .unwrap();

    assert_eq!(run.status, RunStatus::Running);
}

struct StartFailingEngine;

#[async_trait]
impl EngineAdapter for StartFailingEngine {
    fn kind(&self) -> &'static str {
        "start-failing"
    }

    async fn start(
        &self,
        _context: EngineRunContext,
        _prompt: String,
        _sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        Err(EngineError::Start("intentional failure".into()))
    }

    async fn abort(&self, _run_id: &str) -> Result<(), EngineError> {
        Err(EngineError::Aborted)
    }
}

#[tokio::test]
async fn engine_start_failure_marks_the_run_and_work_failed_and_clears_active() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Fail safely").await;
    let (publisher, _published) = ChannelEventPublisher::channel(16);
    let supervisor = EngineSupervisor::new(
        harness.repository.clone(),
        Arc::new(StartFailingEngine),
        Arc::new(publisher),
    );

    let first_error = supervisor
        .start(&work.summary.id, "Build it")
        .await
        .unwrap_err();
    let after_first = harness
        .repository
        .get(&work.summary.id)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        serde_json::to_value(first_error).unwrap()["code"],
        "engine_error"
    );
    assert_eq!(after_first.summary.status, WorkStatus::Failed);
    assert_eq!(after_first.runs[0].status, RunStatus::Failed);
    assert!(after_first.runs[0].completed_at.is_some());

    let second_error = supervisor
        .start(&work.summary.id, "Try again")
        .await
        .unwrap_err();
    assert_eq!(
        serde_json::to_value(second_error).unwrap()["code"],
        "engine_error"
    );
}

struct EarlyClosingEngine;

#[async_trait]
impl EngineAdapter for EarlyClosingEngine {
    fn kind(&self) -> &'static str {
        "early-closing"
    }

    async fn start(
        &self,
        _context: EngineRunContext,
        _prompt: String,
        sink: mpsc::Sender<EngineEvent>,
    ) -> Result<EngineSessionRef, EngineError> {
        tokio::spawn(async move {
            let _ = sink
                .send(EngineEvent::RunStarted {
                    model_label: "early-closing".into(),
                })
                .await;
        });
        Ok(EngineSessionRef {
            engine_kind: self.kind().into(),
            session_id: "early-close-session".into(),
        })
    }

    async fn abort(&self, _run_id: &str) -> Result<(), EngineError> {
        Err(EngineError::Aborted)
    }
}

#[tokio::test]
async fn channel_close_before_terminal_is_journaled_as_a_failed_run() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Close early").await;
    let (publisher, mut published) = ChannelEventPublisher::channel(16);
    let supervisor = EngineSupervisor::new(
        harness.repository.clone(),
        Arc::new(EarlyClosingEngine),
        Arc::new(publisher),
    );

    let run = supervisor
        .start(&work.summary.id, "Build it")
        .await
        .unwrap();
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

    assert_eq!(events.len(), 6);
    assert_eq!(
        events
            .iter()
            .map(|event| event.sequence)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5, 6]
    );
    assert!(matches!(
        &events[5].payload,
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
    );

    let first = supervisor.start(&work.summary.id, "First").await.unwrap();
    let first_events = receive_complete_run(&mut published).await;
    tokio::task::yield_now().await;
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
async fn publisher_failure_does_not_stop_the_persisted_event_stream() {
    let harness = TestHarness::new().await;
    let work = harness.create_work("Ignore publish failure").await;
    let (publisher, published) = ChannelEventPublisher::channel(1);
    drop(published);
    let supervisor = EngineSupervisor::new(
        harness.repository.clone(),
        Arc::new(FakeEngineAdapter::new(std::time::Duration::ZERO)),
        Arc::new(publisher),
    );

    let run = supervisor
        .start(&work.summary.id, "Build it")
        .await
        .unwrap();
    let events = tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let events = harness.repository.events_for_run(&run.id).await.unwrap();
            if events.len() == 6 {
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

    assert_eq!(events.last().unwrap().sequence, 6);
    assert_eq!(detail.summary.status, WorkStatus::Completed);
    assert_eq!(detail.runs[0].status, RunStatus::Completed);
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
        })
        .await
        .unwrap_err();

    assert_eq!(
        serde_json::to_value(error).unwrap(),
        serde_json::json!({
            "code": "path_resolution_error",
            "message": "Workspace path could not be resolved",
            "details": { "path": missing_path.to_string_lossy() }
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
        })
        .await
        .unwrap();
    let newer = repository
        .create(CreateWorkInput {
            title: "Newer".into(),
            goal: "Goal".into(),
            root_path: workspace_path.to_string_lossy().into_owned(),
            permission_mode: PermissionMode::Balanced,
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
        })
        .await
        .unwrap();
    let listed = service.list_works().await.unwrap();
    let found = service.get_work(&created.summary.id).await.unwrap();

    assert_eq!(listed, vec![created.summary.clone()]);
    assert_eq!(found, created);
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
