use std::{sync::Arc, time::Duration};

use crate::storage::sqlite::Database;

use super::{
    super::{HAS_PENDING_EVENT_DELIVERIES_SQL, OutboxClaimPoll},
    *,
};

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
    let sink = Arc::new(RecordingSink::default());
    let outbox = open_seeded_outbox(pool.clone(), work_id, assignment_id, sink.clone()).await;
    enqueue_events(&pool, work_id, assignment_id, &[event_id.to_owned()]).await;

    let report = outbox.drain().await.unwrap();

    assert_eq!(report.attempted, 1);
    assert_eq!(report.published, 1);
    assert!(report.failed_event_ids.is_empty());
    assert_eq!(sink.event_ids(), vec![event_id.to_owned()]);
    assert!(outbox.pending_event_deliveries().await.unwrap().is_empty());
}

#[tokio::test]
async fn pending_ordinals_stay_stable_across_vacuum() {
    let temporary = tempfile::tempdir().unwrap();
    let database = Database::open(temporary.path().join("outbox-order.db"))
        .await
        .unwrap();
    let pool = database.pool().clone();
    let work_id = "work-outbox-order";
    let assignment_id = "assignment-outbox-order";
    let ids = event_ids(2);
    let outbox = open_seeded_outbox(
        pool.clone(),
        work_id,
        assignment_id,
        Arc::new(RecordingSink::default()),
    )
    .await;
    enqueue_events(&pool, work_id, assignment_id, &ids).await;

    let before = outbox.pending_event_deliveries().await.unwrap();
    assert_eq!(before.len(), 2);
    assert!(before[0].ordinal < before[1].ordinal);
    assert_eq!(
        before
            .iter()
            .map(|delivery| delivery.event_id.as_str())
            .collect::<Vec<_>>(),
        ids.iter().map(String::as_str).collect::<Vec<_>>()
    );

    sqlx::query("VACUUM").execute(&pool).await.unwrap();

    let after = outbox.pending_event_deliveries().await.unwrap();
    assert_eq!(
        after
            .iter()
            .map(|delivery| (delivery.ordinal, delivery.event_id.as_str()))
            .collect::<Vec<_>>(),
        before
            .iter()
            .map(|delivery| (delivery.ordinal, delivery.event_id.as_str()))
            .collect::<Vec<_>>()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn drain_claims_fixed_batches_and_preserves_order() {
    const EVENT_COUNT: usize = 130;
    const EXPECTED_BATCH_SIZE: usize = 64;

    let temporary = tempfile::tempdir().unwrap();
    let database = Database::open(temporary.path().join("outbox-batches.db"))
        .await
        .unwrap();
    let pool = database.pool().clone();
    let work_id = "work-outbox-batches";
    let assignment_id = "assignment-outbox-batches";
    let ids = event_ids(EVENT_COUNT);
    let sink = Arc::new(BlockingSink::default());
    let outbox = open_seeded_outbox(pool.clone(), work_id, assignment_id, sink.clone()).await;
    enqueue_events(&pool, work_id, assignment_id, &ids).await;

    let draining = outbox.clone();
    let drain = tokio::spawn(async move { draining.drain().await });
    wait_until_first_publish_starts(&sink).await;

    let snapshot = outbox.pending_event_deliveries().await.unwrap();
    let delivering = snapshot
        .iter()
        .filter(|delivery| delivery.status == "delivering")
        .count();
    let pending = snapshot
        .iter()
        .filter(|delivery| delivery.status == "pending")
        .count();
    sink.release_first_publish();
    let report = drain.await.unwrap().unwrap();

    assert_eq!(delivering, EXPECTED_BATCH_SIZE);
    assert_eq!(pending, EVENT_COUNT - EXPECTED_BATCH_SIZE);
    assert_eq!(report.attempted, EVENT_COUNT);
    assert_eq!(report.published, EVENT_COUNT);
    assert_eq!(sink.published_ids(), ids);
    assert!(outbox.pending_event_deliveries().await.unwrap().is_empty());
}

#[tokio::test]
async fn pending_diagnostics_are_bounded() {
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let work_id = "work-outbox-bound";
    let assignment_id = "assignment-outbox-bound";
    let ids = event_ids(300);
    let outbox = open_seeded_outbox(
        pool.clone(),
        work_id,
        assignment_id,
        Arc::new(RecordingSink::default()),
    )
    .await;
    enqueue_events(&pool, work_id, assignment_id, &ids).await;

    assert_eq!(outbox.pending_event_deliveries().await.unwrap().len(), 256);
    assert_eq!(
        outbox
            .pending_event_deliveries_limited(usize::MAX)
            .await
            .unwrap()
            .len(),
        256
    );
    assert_eq!(
        outbox
            .pending_event_deliveries_limited(7)
            .await
            .unwrap()
            .len(),
        7
    );
}
