//! Characterization: Event persist-before-publish (#14).
//!
//! Seams under test:
//! - `EngineHarness::execute` + `EventPublisher` + `WorkRepository::events_for_run`
//! - `AssignmentRepository::accept` + `AssignmentEventSink` + `events_for_assignment`
//!
//! A published event must already be queryable from the journal. Transport
//! failure must not roll back that durable fact.

use std::{
    sync::{
        Arc, Condvar, Mutex,
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
    },
    domain::{
        assignment::{AssignmentKind, AssignmentSideEffect, AssignmentStatus},
        event::{WorkEventEnvelope, WorkEventPayload},
        work::PermissionMode,
    },
    engine::{
        EngineInput,
        fake::FakeEngineAdapter,
        harness::{AssignmentExecutionOutcome, AssignmentExecutionRequest, EngineHarness},
        publisher::EventPublisher,
    },
    error::AppError,
    storage::sqlite::Database,
    work::repository::WorkRepository,
};
use serde_json::json;

struct PersistAssertingPublisher {
    works: WorkRepository,
    observed: Mutex<Vec<WorkEventEnvelope>>,
}

impl PersistAssertingPublisher {
    fn new(works: WorkRepository) -> Self {
        Self {
            works,
            observed: Mutex::new(Vec::new()),
        }
    }

    fn observed(&self) -> Vec<WorkEventEnvelope> {
        self.observed.lock().unwrap().clone()
    }
}

#[async_trait]
impl EventPublisher for PersistAssertingPublisher {
    async fn publish(&self, envelope: WorkEventEnvelope) -> Result<(), AppError> {
        let run_id = envelope
            .run_id
            .as_deref()
            .expect("engine events carry a run id");
        let event_id = envelope
            .event_id
            .as_deref()
            .expect("engine events carry a stable event id");
        let persisted = self.works.events_for_run(run_id).await?;
        assert!(
            persisted
                .iter()
                .any(|candidate| candidate.event_id.as_deref() == Some(event_id)),
            "EventPublisher observed {event_id} before WorkRepository could read it"
        );
        self.observed.lock().unwrap().push(envelope);
        Ok(())
    }
}

struct PersistAssertingThenFailingPublisher {
    inner: PersistAssertingPublisher,
    attempts: AtomicUsize,
}

impl PersistAssertingThenFailingPublisher {
    fn new(works: WorkRepository) -> Self {
        Self {
            inner: PersistAssertingPublisher::new(works),
            attempts: AtomicUsize::new(0),
        }
    }

    fn observed(&self) -> Vec<WorkEventEnvelope> {
        self.inner.observed()
    }

    fn attempts(&self) -> usize {
        self.attempts.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl EventPublisher for PersistAssertingThenFailingPublisher {
    async fn publish(&self, envelope: WorkEventEnvelope) -> Result<(), AppError> {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        self.inner.publish(envelope).await?;
        Err(AppError::event_publish(
            "forced EventPublisher failure after journal lookup",
        ))
    }
}

#[derive(Default)]
struct BlockingSink {
    in_flight: Mutex<Vec<String>>,
    published: Mutex<Vec<String>>,
    first_started: (Mutex<bool>, Condvar),
    release_first: (Mutex<bool>, Condvar),
}

impl BlockingSink {
    fn wait_until_first_publish_starts(&self) {
        let (started, ready) = &self.first_started;
        let mut started = started.lock().unwrap();
        while !*started {
            started = ready.wait(started).unwrap();
        }
    }

    fn release_first_publish(&self) {
        let (released, ready) = &self.release_first;
        *released.lock().unwrap() = true;
        ready.notify_all();
    }

    fn in_flight_ids(&self) -> Vec<String> {
        self.in_flight.lock().unwrap().clone()
    }

    fn published_ids(&self) -> Vec<String> {
        self.published.lock().unwrap().clone()
    }
}

impl AssignmentEventSink for BlockingSink {
    fn publish(&self, event: WorkEventEnvelope) -> Result<(), AppError> {
        let event_id = event.event_id.clone().expect("assignment events have IDs");
        self.in_flight.lock().unwrap().push(event_id.clone());

        let first_call = {
            let (started, ready) = &self.first_started;
            let mut started = started.lock().unwrap();
            if *started {
                false
            } else {
                *started = true;
                ready.notify_all();
                true
            }
        };
        if first_call {
            let (released, ready) = &self.release_first;
            let mut released = released.lock().unwrap();
            while !*released {
                released = ready.wait(released).unwrap();
            }
        }

        self.published.lock().unwrap().push(event_id);
        Ok(())
    }
}

struct FailingSink {
    calls: AtomicUsize,
}

impl FailingSink {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl AssignmentEventSink for FailingSink {
    fn publish(&self, _event: WorkEventEnvelope) -> Result<(), AppError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(AppError::event_publish(
            "forced AssignmentEventSink failure",
        ))
    }
}

async fn seed_work(pool: &sqlx::SqlitePool, work_id: &str, root_path: &str) {
    let now = Utc.with_ymd_and_hms(2026, 8, 29, 12, 0, 0).unwrap();
    sqlx::query(
        "INSERT INTO works (id, title, goal, root_path, permission_mode, status, created_at, updated_at) \
         VALUES (?, 'Persist before publish', 'Journal first', ?, 'balanced', 'draft', ?, ?)",
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

fn lead_accept(work_id: &str, assignment_id: &str) -> AcceptAssignmentInput {
    AcceptAssignmentInput {
        id: Some(assignment_id.into()),
        work_id: work_id.into(),
        parent_assignment_id: None,
        created_by_agent_id: None,
        assigned_agent_id: "agent-instance:piwork-lead".into(),
        capability_pack_id: None,
        kind: AssignmentKind::Lead,
        side_effect: AssignmentSideEffect::ReadOnly,
        title: "Persist first".into(),
        instruction: "Journal before publish".into(),
        context_manifest: json!({}),
        expected_result_schema: json!({}),
        acceptance_criteria: json!([]),
        permission_scope: json!({"mode": "inherit_work"}),
        priority: 10,
        max_attempts: 3,
        not_before: None,
    }
}

async fn claimed_lead(
    repository: &AssignmentRepository,
    work_repository: &WorkRepository,
    agent_repository: &AgentRepository,
    work_id: &str,
    assignment_id: &str,
    runtime_owner: &str,
) -> AssignmentExecutionRequest {
    let assignment = repository
        .accept(lead_accept(work_id, assignment_id))
        .await
        .unwrap();
    let assignment = repository
        .claim(&assignment.id, runtime_owner, Utc::now())
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
    AssignmentExecutionRequest {
        assignment,
        work,
        agent,
        run,
        input: EngineInput {
            message: "Build it".into(),
            images: vec![],
            documents: vec![],
        },
        effective_permission: PermissionMode::Balanced,
        runtime_owner: runtime_owner.into(),
        extension_tool_ids: Vec::new(),
    }
}

fn journaled_event_ids(events: &[WorkEventEnvelope]) -> Vec<String> {
    events
        .iter()
        .map(|event| event.event_id.clone().expect("journaled events have IDs"))
        .collect()
}

fn assert_published_event_ids_are_journaled(
    published: &[WorkEventEnvelope],
    journaled: &[WorkEventEnvelope],
) {
    let journaled_ids = journaled_event_ids(journaled);
    for event in published {
        let event_id = event
            .event_id
            .as_deref()
            .expect("published events have IDs");
        assert!(
            journaled_ids.iter().any(|candidate| candidate == event_id),
            "published {event_id} was missing from the journal {journaled_ids:?}"
        );
    }
}

#[tokio::test]
async fn harness_journals_each_engine_event_before_the_publisher_observes_it() {
    let temp = tempfile::tempdir().unwrap();
    let database = Database::open(temp.path().join("harness-persist-before-publish.db"))
        .await
        .unwrap();
    let pool = database.pool().clone();
    let root_path = temp.path().join("workspace");
    std::fs::create_dir_all(&root_path).unwrap();
    let root_path = root_path.to_string_lossy().into_owned();
    seed_work(&pool, "work-harness-persist", &root_path).await;

    let assignments = AssignmentRepository::new(pool.clone());
    let works = WorkRepository::new(pool.clone());
    let agents = AgentRepository::new(pool);
    let publisher = Arc::new(PersistAssertingPublisher::new(works.clone()));
    let request = claimed_lead(
        &assignments,
        &works,
        &agents,
        "work-harness-persist",
        "assignment-harness-persist",
        "persist-owner",
    )
    .await;
    let run_id = request.run.id.clone();
    let harness = EngineHarness::new(
        Arc::new(FakeEngineAdapter::new(Duration::ZERO)),
        works.clone(),
        assignments,
        Arc::clone(&publisher) as _,
    );

    let outcome = harness.execute(request).await.unwrap();
    assert!(
        matches!(
            outcome,
            AssignmentExecutionOutcome::Waiting { ref reason } if reason == "delivery_required"
        ),
        "lead RunCompleted waits for delivery; outcome={outcome:?}"
    );

    let published = publisher.observed();
    assert!(
        !published.is_empty(),
        "the fake engine must publish a journaled stream"
    );
    assert!(
        published
            .iter()
            .any(|event| matches!(event.payload, WorkEventPayload::RunStarted { .. })),
        "the published stream must include RunStarted"
    );
    assert!(
        published
            .iter()
            .any(|event| matches!(event.payload, WorkEventPayload::RunCompleted { .. })),
        "the published stream must include the terminal RunCompleted"
    );

    let persisted = works.events_for_run(&run_id).await.unwrap();
    assert_published_event_ids_are_journaled(&published, &persisted);
}

#[tokio::test]
async fn harness_keeps_journaled_events_when_the_publisher_fails() {
    let temp = tempfile::tempdir().unwrap();
    let database = Database::open(temp.path().join("harness-publish-fail.db"))
        .await
        .unwrap();
    let pool = database.pool().clone();
    let root_path = temp.path().join("workspace");
    std::fs::create_dir_all(&root_path).unwrap();
    let root_path = root_path.to_string_lossy().into_owned();
    seed_work(&pool, "work-harness-publish-fail", &root_path).await;

    let assignments = AssignmentRepository::new(pool.clone());
    let works = WorkRepository::new(pool.clone());
    let agents = AgentRepository::new(pool);
    let publisher = Arc::new(PersistAssertingThenFailingPublisher::new(works.clone()));
    let request = claimed_lead(
        &assignments,
        &works,
        &agents,
        "work-harness-publish-fail",
        "assignment-harness-publish-fail",
        "persist-fail-owner",
    )
    .await;
    let run_id = request.run.id.clone();
    let harness = EngineHarness::new(
        Arc::new(FakeEngineAdapter::new(Duration::ZERO)),
        works.clone(),
        assignments,
        Arc::clone(&publisher) as _,
    );

    let outcome = harness.execute(request).await.unwrap();
    assert!(
        matches!(
            outcome,
            AssignmentExecutionOutcome::Waiting { ref reason } if reason == "delivery_required"
        ),
        "publisher failure must not stop harness journaling; outcome={outcome:?}"
    );
    assert!(
        publisher.attempts() > 0,
        "the publisher must have been invoked after each journal commit"
    );

    let persisted = works.events_for_run(&run_id).await.unwrap();
    assert!(
        persisted.len() >= publisher.attempts(),
        "publisher failure must not drop journaled engine events"
    );
    assert_published_event_ids_are_journaled(&publisher.observed(), &persisted);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn assignment_accept_journals_the_event_before_sink_publish_returns() {
    let temporary = tempfile::tempdir().unwrap();
    let database = Database::open(
        temporary
            .path()
            .join("assignment-persist-before-publish.db"),
    )
    .await
    .unwrap();
    let pool = database.pool().clone();
    seed_work(&pool, "work-assignment-persist", ".").await;
    let sink = Arc::new(BlockingSink::default());
    let repository = AssignmentRepository::with_event_sink(pool, Arc::clone(&sink) as _);
    let assignment_id = "assignment-accept-persist";

    let accepting = {
        let repository = repository.clone();
        tokio::spawn(async move {
            repository
                .accept(lead_accept("work-assignment-persist", assignment_id))
                .await
        })
    };
    tokio::time::timeout(
        Duration::from_secs(2),
        tokio::task::spawn_blocking({
            let sink = Arc::clone(&sink);
            move || sink.wait_until_first_publish_starts()
        }),
    )
    .await
    .expect("accept never invoked the AssignmentEventSink")
    .unwrap();

    let in_flight = sink.in_flight_ids();
    assert_eq!(in_flight.len(), 1, "drain must publish the queued event");
    assert!(
        sink.published_ids().is_empty(),
        "sink must not complete delivery before the journal is checked"
    );

    let journaled = repository
        .events_for_assignment(assignment_id)
        .await
        .unwrap();
    assert_eq!(journaled.len(), 1);
    assert_eq!(journaled[0].run_id, None);
    assert_eq!(
        journaled[0].event_id.as_deref(),
        Some(in_flight[0].as_str())
    );
    assert!(matches!(
        journaled[0].payload,
        WorkEventPayload::AssignmentQueued { .. }
    ));

    sink.release_first_publish();
    let assignment = accepting.await.unwrap().unwrap();
    assert_eq!(assignment.status, AssignmentStatus::Queued);
    assert_eq!(sink.published_ids(), in_flight);
}

#[tokio::test]
async fn assignment_accept_keeps_the_journaled_event_when_the_sink_fails() {
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    seed_work(&pool, "work-assignment-publish-fail", ".").await;
    let sink = Arc::new(FailingSink::new());
    let repository = AssignmentRepository::with_event_sink(pool, Arc::clone(&sink) as _);
    let assignment_id = "assignment-accept-publish-fail";

    let assignment = repository
        .accept(lead_accept("work-assignment-publish-fail", assignment_id))
        .await
        .unwrap();

    assert_eq!(assignment.status, AssignmentStatus::Queued);
    assert_eq!(sink.calls(), 1);

    let journaled = repository
        .events_for_assignment(assignment_id)
        .await
        .unwrap();
    assert_eq!(journaled.len(), 1);
    assert!(journaled[0].event_id.is_some());
    assert!(matches!(
        journaled[0].payload,
        WorkEventPayload::AssignmentQueued { .. }
    ));
}
