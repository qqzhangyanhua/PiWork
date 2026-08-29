use std::{sync::Arc, time::Duration};

use crate::storage::sqlite::Database;

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_drains_claim_each_pending_event_once() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("outbox-concurrent.db");
    let first_database = Database::open(&path).await.unwrap();
    let second_database = Database::open(&path).await.unwrap();
    let work_id = "work-outbox-concurrent";
    let assignment_id = "assignment-outbox-concurrent";
    let event_id = "event-concurrent";
    seed_work_and_assignment(first_database.pool(), work_id, assignment_id).await;
    enqueue_events(
        first_database.pool(),
        work_id,
        assignment_id,
        &[event_id.to_owned()],
    )
    .await;

    let sink = Arc::new(RecordingSink::default());
    let first = AssignmentEventOutbox::new(first_database.pool().clone(), sink.clone());
    let second = AssignmentEventOutbox::new(second_database.pool().clone(), sink.clone());

    let (first_report, second_report) = tokio::join!(first.drain(), second.drain());
    let first_report = first_report.unwrap();
    let second_report = second_report.unwrap();

    assert_eq!(first_report.attempted + second_report.attempted, 1);
    assert_eq!(sink.event_ids(), vec![event_id.to_owned()]);
    assert!(first.pending_event_deliveries().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_drain_waits_instead_of_stealing_an_inflight_batch() {
    let temporary = tempfile::tempdir().unwrap();
    let database = Database::open(temporary.path().join("outbox-lease.db"))
        .await
        .unwrap();
    let pool = database.pool().clone();
    let work_id = "work-outbox-lease";
    let assignment_id = "assignment-outbox-lease";
    let ids = event_ids(1);
    let sink = Arc::new(BlockingSink::default());
    seed_work_and_assignment(&pool, work_id, assignment_id).await;
    enqueue_events(&pool, work_id, assignment_id, &ids).await;
    let first = AssignmentEventOutbox::new(pool.clone(), sink.clone());
    let second = AssignmentEventOutbox::new(pool.clone(), sink.clone());

    let first_drain = tokio::spawn({
        let first = first.clone();
        async move { first.drain().await }
    });
    wait_until_first_publish_starts(&sink).await;

    let mut second_drain = tokio::spawn(async move { second.drain().await });
    let returned_while_first_publish_was_blocked =
        tokio::time::timeout(Duration::from_millis(250), &mut second_drain).await;
    let returned_early = returned_while_first_publish_was_blocked.is_ok();
    sink.release_first_publish();
    let first_result = first_drain.await.unwrap();
    let second_result = match returned_while_first_publish_was_blocked {
        Ok(result) => result.unwrap(),
        Err(_) => second_drain.await.unwrap(),
    };

    assert!(
        !returned_early,
        "a concurrent drain reclaimed an event whose sink invocation was still in flight"
    );
    first_result.unwrap();
    assert_eq!(second_result.unwrap().attempted, 0);
    assert_eq!(sink.publish_count(), 1);
    assert_eq!(sink.published_ids(), ids);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loser_waits_while_winner_holds_the_batch() {
    const EVENT_COUNT: usize = 20;

    let temporary = tempfile::tempdir().unwrap();
    let database = Database::open(temporary.path().join("outbox-large-wait.db"))
        .await
        .unwrap();
    let pool = database.pool().clone();
    let work_id = "work-outbox-large-wait";
    let assignment_id = "assignment-outbox-large-wait";
    let ids = event_ids(EVENT_COUNT);
    let sink = Arc::new(BlockingSink::default());
    seed_work_and_assignment(&pool, work_id, assignment_id).await;
    enqueue_events(&pool, work_id, assignment_id, &ids).await;
    let winner = AssignmentEventOutbox::new(pool.clone(), sink.clone());
    let loser = AssignmentEventOutbox::new(pool.clone(), sink.clone());

    let winner_drain = tokio::spawn({
        let winner = winner.clone();
        async move { winner.drain().await }
    });
    wait_until_first_publish_starts(&sink).await;

    assert_eq!(
        loser.pending_event_deliveries().await.unwrap().len(),
        EVENT_COUNT
    );
    assert_eq!(
        loser
            .pending_event_deliveries_limited(7)
            .await
            .unwrap()
            .len(),
        7
    );
    let loser_drain = tokio::spawn({
        let loser = loser.clone();
        async move { loser.drain().await }
    });
    tokio::time::sleep(Duration::from_millis(40)).await;
    assert!(!loser_drain.is_finished());

    sink.release_first_publish();
    let winner_report = winner_drain.await.unwrap().unwrap();
    let loser_report = loser_drain.await.unwrap().unwrap();

    assert_eq!(
        winner_report.attempted + loser_report.attempted,
        EVENT_COUNT
    );
    assert!(winner.pending_event_deliveries().await.unwrap().is_empty());
}
