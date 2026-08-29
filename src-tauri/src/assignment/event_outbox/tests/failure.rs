use std::sync::Arc;

use crate::storage::sqlite::Database;

use super::*;

#[tokio::test]
async fn sink_failure_returns_event_to_pending_releases_unattempted_and_redacts_error() {
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let work_id = "work-outbox-fail";
    let assignment_id = "assignment-outbox-fail";
    let ids = event_ids(3);
    let sink = Arc::new(FailingSink::with_failure_message(
        vec![1],
        "secret-token=do-not-persist",
    ));
    let outbox = open_seeded_outbox(pool.clone(), work_id, assignment_id, sink.clone()).await;
    enqueue_events(&pool, work_id, assignment_id, &ids).await;

    let report = outbox.drain().await.unwrap();

    assert_eq!(report.attempted, 1);
    assert_eq!(report.published, 0);
    assert_eq!(report.failed_event_ids, vec![ids[0].clone()]);
    assert!(sink.published_ids().is_empty());

    let pending = outbox.pending_event_deliveries().await.unwrap();
    assert_eq!(pending.len(), 3);
    assert_eq!(
        pending
            .iter()
            .map(|delivery| delivery.event_id.as_str())
            .collect::<Vec<_>>(),
        ids.iter().map(String::as_str).collect::<Vec<_>>()
    );
    assert_eq!(
        pending
            .iter()
            .map(|delivery| delivery.attempt_count)
            .collect::<Vec<_>>(),
        vec![1, 0, 0]
    );
    let last_error = pending[0].last_error.as_deref().unwrap();
    assert!(!last_error.contains("secret-token"));
    assert!(last_error.len() <= 512);
    assert!(pending[0].last_attempt_at.is_some());
}

#[tokio::test]
async fn failed_event_blocks_higher_ordinals_until_retry() {
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let work_id = "work-outbox-ordinals";
    let assignment_id = "assignment-outbox-ordinals";
    let ids = event_ids(8);
    let sink = Arc::new(FailingSink::new(vec![6]));
    let outbox = open_seeded_outbox(pool.clone(), work_id, assignment_id, sink.clone()).await;
    enqueue_events(&pool, work_id, assignment_id, &ids).await;

    let first = outbox.drain().await.unwrap();
    assert_eq!(first.attempted, 6);
    assert_eq!(first.published, 5);
    assert_eq!(first.failed_event_ids, vec![ids[5].clone()]);
    assert_eq!(sink.published_ids(), ids[..5]);

    let pending = outbox.pending_event_deliveries().await.unwrap();
    assert_eq!(pending.len(), 3);
    assert_eq!(
        pending
            .iter()
            .map(|delivery| delivery.attempt_count)
            .collect::<Vec<_>>(),
        vec![1, 0, 0]
    );

    let retry = outbox.drain().await.unwrap();
    assert_eq!(retry.attempted, 3);
    assert_eq!(retry.published, 3);
    assert!(retry.failed_event_ids.is_empty());
    assert_eq!(sink.published_ids(), ids);
    assert!(outbox.pending_event_deliveries().await.unwrap().is_empty());
}

#[tokio::test]
async fn attempt_metadata_counts_only_events_whose_sink_was_invoked() {
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let work_id = "work-outbox-attempts";
    let assignment_id = "assignment-outbox-attempts";
    let ids = event_ids(2);
    let sink = Arc::new(FailingSink::new(vec![1]));
    let outbox = open_seeded_outbox(pool.clone(), work_id, assignment_id, sink).await;
    enqueue_events(&pool, work_id, assignment_id, &ids).await;

    outbox.drain().await.unwrap();

    let pending = outbox.pending_event_deliveries().await.unwrap();
    assert_eq!(
        pending
            .iter()
            .map(|delivery| delivery.attempt_count)
            .collect::<Vec<_>>(),
        vec![1, 0]
    );
}

#[tokio::test]
async fn malformed_event_does_not_strand_a_delivery() {
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let work_id = "work-outbox-malformed";
    let assignment_id = "assignment-outbox-malformed";
    let event_id = "event-malformed";
    let sink = Arc::new(RecordingSink::default());
    let outbox = open_seeded_outbox(pool.clone(), work_id, assignment_id, sink).await;
    enqueue_events(&pool, work_id, assignment_id, &[event_id.to_owned()]).await;
    sqlx::query("UPDATE events SET payload = '{}' WHERE id = ?")
        .bind(event_id)
        .execute(&pool)
        .await
        .unwrap();

    outbox.drain().await.unwrap_err();

    let pending = outbox.pending_event_deliveries().await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].event_id, event_id);
    assert_eq!(pending[0].status, "pending");
}

#[tokio::test]
async fn unavailable_sink_leaves_events_pending() {
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let work_id = "work-outbox-unavailable";
    let assignment_id = "assignment-outbox-unavailable";
    seed_work_and_assignment(&pool, work_id, assignment_id).await;
    enqueue_events(
        &pool,
        work_id,
        assignment_id,
        &[String::from("event-unavailable")],
    )
    .await;
    let outbox = AssignmentEventOutbox::unavailable(pool);

    let report = outbox.drain().await.unwrap();

    assert_eq!(report.published, 0);
    assert_eq!(report.failed_event_ids, vec!["event-unavailable"]);
    assert_eq!(pending_statuses(&outbox).await, vec!["pending"]);
}
