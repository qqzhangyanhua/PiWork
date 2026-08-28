use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::Utc;
use sqlx::SqlitePool;

use crate::{
    domain::event::{WorkEventEnvelope, WorkEventPayload},
    error::AppError,
    storage::sqlite::Database,
};

use super::{
    AssignmentEventOutbox, AssignmentEventSink, HAS_PENDING_EVENT_DELIVERIES_SQL, OutboxClaimPoll,
};

#[derive(Default)]
struct RecordingSink {
    events: Mutex<Vec<WorkEventEnvelope>>,
}

impl AssignmentEventSink for RecordingSink {
    fn publish(&self, event: WorkEventEnvelope) -> Result<(), AppError> {
        self.events.lock().unwrap().push(event);
        Ok(())
    }
}

impl RecordingSink {
    fn event_ids(&self) -> Vec<String> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| event.event_id.clone())
            .collect()
    }
}

#[test]
fn outbox_claim_poll_uses_bounded_exponential_backoff() {
    let mut poll = OutboxClaimPoll::default();

    assert_eq!(poll.next_delay(), Duration::from_millis(10));
    assert_eq!(poll.next_delay(), Duration::from_millis(20));
    assert_eq!(poll.next_delay(), Duration::from_millis(40));
    assert_eq!(poll.next_delay(), Duration::from_millis(80));
    assert_eq!(poll.next_delay(), Duration::from_millis(160));
    assert_eq!(poll.next_delay(), Duration::from_millis(250));
    assert_eq!(poll.next_delay(), Duration::from_millis(250));
}

#[tokio::test]
async fn pending_probe_uses_the_partial_outbox_index() {
    let database = Database::open_in_memory().await.unwrap();
    let plan = sqlx::query_as::<_, (i64, i64, i64, String)>(&format!(
        "EXPLAIN QUERY PLAN {HAS_PENDING_EVENT_DELIVERIES_SQL}"
    ))
    .fetch_all(database.pool())
    .await
    .unwrap();

    assert!(
        plan.iter()
            .any(|(_, _, _, detail)| detail.contains("idx_assignment_event_outbox_pending")),
        "pending probe query plan did not use the partial index: {plan:?}"
    );
}

#[tokio::test]
async fn drain_delivers_enqueued_event_id_and_leaves_pending_empty() {
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let work_id = "work-outbox-drain";
    let assignment_id = "assignment-outbox-drain";
    let event_id = "event-outbox-drain-1";
    seed_work_and_assignment(&pool, work_id, assignment_id).await;

    let sink = Arc::new(RecordingSink::default());
    let outbox = AssignmentEventOutbox::new(pool.clone(), sink.clone());
    let occurred_at = Utc::now();
    let event = WorkEventEnvelope {
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
        sequence: 1,
        occurred_at,
        payload: WorkEventPayload::AssignmentQueued {
            assignment_id: assignment_id.into(),
            assigned_agent_id: "agent-instance:piwork-lead".into(),
            title: "Outbox drain".into(),
            priority: 10,
        },
    };

    let mut transaction = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    insert_event_row(&mut transaction, &event).await;
    AssignmentEventOutbox::insert(&mut transaction, event_id, assignment_id, occurred_at)
        .await
        .unwrap();
    transaction.commit().await.unwrap();

    let report = outbox.drain().await.unwrap();

    assert_eq!(report.attempted, 1);
    assert_eq!(report.published, 1);
    assert!(report.failed_event_ids.is_empty());
    assert_eq!(sink.event_ids(), vec![event_id.to_owned()]);
    assert!(outbox.pending_event_deliveries().await.unwrap().is_empty());
}

async fn seed_work_and_assignment(pool: &SqlitePool, work_id: &str, assignment_id: &str) {
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
