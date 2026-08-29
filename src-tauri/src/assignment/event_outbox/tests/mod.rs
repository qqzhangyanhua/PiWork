use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use chrono::Utc;
use sqlx::SqlitePool;

use crate::{
    domain::event::{WorkEventEnvelope, WorkEventPayload},
    error::AppError,
};

use super::{AssignmentEventOutbox, AssignmentEventSink};

mod concurrency;
mod drain;
mod failure;
mod recover;

#[derive(Default)]
pub struct RecordingSink {
    events: Mutex<Vec<WorkEventEnvelope>>,
}

impl AssignmentEventSink for RecordingSink {
    fn publish(&self, event: WorkEventEnvelope) -> Result<(), AppError> {
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}

impl RecordingSink {
    pub fn event_ids(&self) -> Vec<String> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| event.event_id.clone())
            .collect()
    }
}

#[derive(Default)]
pub struct ConfirmingSink {
    published: Mutex<Vec<String>>,
    confirmed: Mutex<Vec<String>>,
}

impl AssignmentEventSink for ConfirmingSink {
    fn publish(&self, event: WorkEventEnvelope) -> Result<(), AppError> {
        self.published
            .lock()
            .unwrap()
            .push(event.event_id.clone().expect("test events have IDs"));
        Ok(())
    }

    fn delivery_confirmed(&self, event_id: &str) {
        self.confirmed.lock().unwrap().push(event_id.to_owned());
    }
}

impl ConfirmingSink {
    pub fn published_ids(&self) -> Vec<String> {
        self.published.lock().unwrap().clone()
    }

    pub fn confirmed_ids(&self) -> Vec<String> {
        self.confirmed.lock().unwrap().clone()
    }
}

pub struct FailingSink {
    call_count: AtomicUsize,
    fail_on: Vec<usize>,
    failure_message: String,
    published: Mutex<Vec<String>>,
}

impl FailingSink {
    pub fn new(fail_on: Vec<usize>) -> Self {
        Self::with_failure_message(fail_on, "forced publication failure")
    }

    pub fn with_failure_message(fail_on: Vec<usize>, failure_message: &str) -> Self {
        Self {
            call_count: AtomicUsize::new(0),
            fail_on,
            failure_message: failure_message.into(),
            published: Mutex::new(Vec::new()),
        }
    }

    pub fn published_ids(&self) -> Vec<String> {
        self.published.lock().unwrap().clone()
    }
}

impl AssignmentEventSink for FailingSink {
    fn publish(&self, event: WorkEventEnvelope) -> Result<(), AppError> {
        let call = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_on.contains(&call) {
            return Err(AppError::event_publish(format!(
                "{} at {call}",
                self.failure_message
            )));
        }
        self.published
            .lock()
            .unwrap()
            .push(event.event_id.clone().expect("test events have IDs"));
        Ok(())
    }
}

#[derive(Default)]
pub struct BlockingSink {
    call_count: AtomicUsize,
    first_started: (Mutex<bool>, Condvar),
    release_first: (Mutex<bool>, Condvar),
    published: Mutex<Vec<String>>,
}

impl BlockingSink {
    fn wait_until_first_publish_starts(&self) {
        let (started, ready) = &self.first_started;
        let mut started = started.lock().unwrap();
        while !*started {
            started = ready.wait(started).unwrap();
        }
    }

    pub fn release_first_publish(&self) {
        let (released, ready) = &self.release_first;
        *released.lock().unwrap() = true;
        ready.notify_all();
    }

    pub fn published_ids(&self) -> Vec<String> {
        self.published.lock().unwrap().clone()
    }

    pub fn publish_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

impl AssignmentEventSink for BlockingSink {
    fn publish(&self, event: WorkEventEnvelope) -> Result<(), AppError> {
        let call = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
        if call == 1 {
            let (started, ready) = &self.first_started;
            *started.lock().unwrap() = true;
            ready.notify_all();

            let (released, ready) = &self.release_first;
            let mut released = released.lock().unwrap();
            while !*released {
                released = ready.wait(released).unwrap();
            }
        }
        self.published
            .lock()
            .unwrap()
            .push(event.event_id.clone().expect("test events have IDs"));
        Ok(())
    }
}

pub struct PanickingSink;

impl AssignmentEventSink for PanickingSink {
    fn publish(&self, _event: WorkEventEnvelope) -> Result<(), AppError> {
        panic!("forced sink panic");
    }
}

pub struct PanicAfterFirstPublish {
    inner: Arc<dyn AssignmentEventSink>,
    calls: AtomicUsize,
}

impl PanicAfterFirstPublish {
    pub fn wrap(inner: Arc<dyn AssignmentEventSink>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            calls: AtomicUsize::new(0),
        })
    }
}

impl AssignmentEventSink for PanicAfterFirstPublish {
    fn publish(&self, event: WorkEventEnvelope) -> Result<(), AppError> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        let result = self.inner.publish(event);
        if call == 1 {
            panic!("forced crash after publish");
        }
        result
    }

    fn delivery_confirmed(&self, event_id: &str) {
        self.inner.delivery_confirmed(event_id);
    }
}

fn queued_event(
    work_id: &str,
    assignment_id: &str,
    event_id: &str,
    sequence: u32,
) -> WorkEventEnvelope {
    WorkEventEnvelope {
        version: 2,
        event_id: Some(event_id.into()),
        work_id: work_id.into(),
        run_id: None,
        turn_id: None,
        session_id: None,
        agent_id: Some("agent-instance:piwork-lead".into()),
        assignment_id: Some(assignment_id.into()),
        causation_id: None,
        correlation_id: Some(assignment_id.into()),
        sequence,
        occurred_at: Utc::now(),
        payload: WorkEventPayload::AssignmentQueued {
            assignment_id: assignment_id.into(),
            assigned_agent_id: "agent-instance:piwork-lead".into(),
            title: "Outbox event".into(),
            priority: 10,
        },
    }
}

pub async fn seed_work_and_assignment(pool: &SqlitePool, work_id: &str, assignment_id: &str) {
    let now = Utc::now();
    sqlx::query("INSERT INTO works (id, title, goal, root_path, permission_mode, status, created_at, updated_at) VALUES (?, 'Outbox test', 'test', '.', 'balanced', 'draft', ?, ?)")
        .bind(work_id)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO work_agents (work_id, agent_instance_id, role_kind, status, permission_policy, joined_at, updated_at) VALUES (?, 'agent-instance:piwork-lead', 'lead', 'joined', 'inherit_work', ?, ?)")
        .bind(work_id)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO work_leads (work_id, agent_instance_id, created_at) VALUES (?, 'agent-instance:piwork-lead', ?)")
        .bind(work_id)
        .bind(now)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO assignments (id, work_id, assigned_agent_id, kind, side_effect, title, instruction, context_manifest_json, expected_result_schema_json, acceptance_criteria_json, permission_scope_json, priority, status, attempt_count, max_attempts, created_at, updated_at) VALUES (?, ?, 'agent-instance:piwork-lead', 'lead', 'read_only', 'Outbox drain', 'Deliver the event', '{}', '{}', '[]', '{\"mode\":\"inherit_work\"}', 10, 'queued', 0, 3, ?, ?)",
    )
    .bind(assignment_id)
    .bind(work_id)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_event_row(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    event: &WorkEventEnvelope,
) {
    let payload = serde_json::to_string(&event.payload).unwrap();
    let event_id = event.event_id.as_deref().expect("test events have IDs");
    sqlx::query("INSERT INTO events (id, work_id, run_id, sequence, version, occurred_at, payload, turn_id, session_id, agent_id, assignment_id, causation_id, correlation_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(event_id)
        .bind(&event.work_id)
        .bind(&event.run_id)
        .bind(i64::from(event.sequence))
        .bind(i64::from(event.version))
        .bind(event.occurred_at)
        .bind(payload)
        .bind(&event.turn_id)
        .bind(&event.session_id)
        .bind(&event.agent_id)
        .bind(&event.assignment_id)
        .bind(&event.causation_id)
        .bind(&event.correlation_id)
        .execute(&mut **transaction)
        .await
        .unwrap();
}

pub async fn enqueue_events(
    pool: &SqlitePool,
    work_id: &str,
    assignment_id: &str,
    event_ids: &[String],
) {
    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    for (index, event_id) in event_ids.iter().enumerate() {
        let event = queued_event(work_id, assignment_id, event_id, (index + 1) as u32);
        insert_event_row(&mut transaction, &event).await;
        AssignmentEventOutbox::insert(&mut transaction, event_id, assignment_id, event.occurred_at)
            .await
            .unwrap();
    }
    transaction.commit().await.unwrap();
}

pub fn event_ids(count: usize) -> Vec<String> {
    (1..=count)
        .map(|index| format!("event-{index:04}"))
        .collect()
}

pub async fn open_seeded_outbox(
    pool: SqlitePool,
    work_id: &str,
    assignment_id: &str,
    sink: Arc<dyn AssignmentEventSink>,
) -> AssignmentEventOutbox {
    seed_work_and_assignment(&pool, work_id, assignment_id).await;
    AssignmentEventOutbox::new(pool, sink)
}

pub async fn wait_until_first_publish_starts(sink: &Arc<BlockingSink>) {
    tokio::time::timeout(
        Duration::from_secs(2),
        tokio::task::spawn_blocking({
            let sink = sink.clone();
            move || sink.wait_until_first_publish_starts()
        }),
    )
    .await
    .expect("drain never invoked its first sink event")
    .unwrap();
}

pub async fn pending_statuses(outbox: &AssignmentEventOutbox) -> Vec<String> {
    outbox
        .pending_event_deliveries()
        .await
        .unwrap()
        .into_iter()
        .map(|delivery| delivery.status)
        .collect()
}
