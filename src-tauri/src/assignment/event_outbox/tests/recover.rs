use std::{sync::Arc, time::Duration};

use crate::{engine::activity_observer::ActivityObserverHandle, storage::sqlite::Database};

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn startup_recover_reclaims_delivering_and_drain_succeeds() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("outbox-recover.db");
    let work_id = "work-outbox-recover";
    let assignment_id = "assignment-outbox-recover";
    let event_id = "event-recover";
    let blocked = Arc::new(BlockingSink::default());

    {
        let database = Database::open(&path).await.unwrap();
        let pool = database.pool().clone();
        seed_work_and_assignment(&pool, work_id, assignment_id).await;
        enqueue_events(&pool, work_id, assignment_id, &[event_id.to_owned()]).await;
        let live = AssignmentEventOutbox::new(pool.clone(), blocked.clone());
        let live_drain = tokio::spawn(async move { live.drain().await });
        wait_until_first_publish_starts(&blocked).await;
        pool.close().await;
        blocked.release_first_publish();
        let _ = live_drain.await;
    }

    let reopened = Database::open(&path).await.unwrap();
    let recording = Arc::new(RecordingSink::default());
    let recovered = AssignmentEventOutbox::new(reopened.pool().clone(), recording.clone());
    let stuck = recovered.clone();
    let drain_without_recover = tokio::spawn(async move { stuck.drain().await });
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        !drain_without_recover.is_finished(),
        "drain without recover claimed a delivering row left by the crashed process"
    );
    drain_without_recover.abort();

    recovered.recover().await.unwrap();
    assert_eq!(pending_statuses(&recovered).await, vec!["pending"]);
    let report = recovered.drain().await.unwrap();

    assert_eq!(report.published, 1);
    assert_eq!(recording.event_ids(), vec![event_id.to_owned()]);
    assert!(
        recovered
            .pending_event_deliveries()
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn panicking_sink_can_be_recovered_and_drained() {
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let work_id = "work-outbox-panic";
    let assignment_id = "assignment-outbox-panic";
    let event_id = "event-panic";
    seed_work_and_assignment(&pool, work_id, assignment_id).await;
    enqueue_events(&pool, work_id, assignment_id, &[event_id.to_owned()]).await;
    let panicking = AssignmentEventOutbox::new(pool.clone(), Arc::new(PanickingSink));

    let drain = tokio::spawn(async move { panicking.drain().await });
    assert!(drain.await.unwrap_err().is_panic());

    let sink = Arc::new(RecordingSink::default());
    let recovered = AssignmentEventOutbox::new(pool, sink.clone());
    recovered.recover().await.unwrap();
    assert_eq!(recovered.drain().await.unwrap().published, 1);
    assert_eq!(sink.event_ids(), vec![event_id.to_owned()]);
}

#[tokio::test]
async fn publish_crash_before_ack_replays_at_least_once() {
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let work_id = "work-outbox-replay";
    let assignment_id = "assignment-outbox-replay";
    let event_id = "event-replay";
    let sink = Arc::new(RecordingSink::default());
    let outbox = open_seeded_outbox(
        pool.clone(),
        work_id,
        assignment_id,
        PanicAfterFirstPublish::wrap(sink.clone()),
    )
    .await;
    enqueue_events(&pool, work_id, assignment_id, &[event_id.to_owned()]).await;

    let drain = tokio::spawn({
        let outbox = outbox.clone();
        async move { outbox.drain().await }
    });
    assert!(drain.await.unwrap_err().is_panic());
    assert_eq!(sink.event_ids(), vec![event_id.to_owned()]);
    let pending = outbox.pending_event_deliveries().await.unwrap();
    assert_eq!(pending[0].event_id, event_id);

    outbox.recover().await.unwrap();
    let report = outbox.drain().await.unwrap();

    assert_eq!(report.published, 1);
    assert_eq!(
        sink.event_ids(),
        vec![event_id.to_owned(), event_id.to_owned()]
    );
    assert!(outbox.pending_event_deliveries().await.unwrap().is_empty());
}

#[tokio::test]
async fn observer_deduplicates_replayed_event_id() {
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let work_id = "work-outbox-observer";
    let assignment_id = "assignment-outbox-observer";
    let event_id = "event-observer";
    let observer = ActivityObserverHandle::in_process();
    let outbox = open_seeded_outbox(
        pool.clone(),
        work_id,
        assignment_id,
        PanicAfterFirstPublish::wrap(Arc::new(observer.clone())),
    )
    .await;
    enqueue_events(&pool, work_id, assignment_id, &[event_id.to_owned()]).await;

    let drain = tokio::spawn({
        let outbox = outbox.clone();
        async move { outbox.drain().await }
    });
    assert!(drain.await.unwrap_err().is_panic());
    assert_eq!(observer.snapshot().len(), 1);

    outbox.recover().await.unwrap();
    outbox.drain().await.unwrap();

    let visible = observer.snapshot();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].event_id.as_deref(), Some(event_id));
}

#[tokio::test]
async fn delivery_confirmed_only_after_durable_ack() {
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let work_id = "work-outbox-confirm";
    let assignment_id = "assignment-outbox-confirm";
    let event_id = "event-confirm";
    let sink = Arc::new(ConfirmingSink::default());
    let outbox = open_seeded_outbox(
        pool.clone(),
        work_id,
        assignment_id,
        PanicAfterFirstPublish::wrap(sink.clone()),
    )
    .await;
    enqueue_events(&pool, work_id, assignment_id, &[event_id.to_owned()]).await;

    let drain = tokio::spawn({
        let outbox = outbox.clone();
        async move { outbox.drain().await }
    });
    assert!(drain.await.unwrap_err().is_panic());
    assert_eq!(sink.published_ids(), vec![event_id.to_owned()]);
    assert!(sink.confirmed_ids().is_empty());

    outbox.recover().await.unwrap();
    outbox.drain().await.unwrap();

    assert_eq!(
        sink.published_ids(),
        vec![event_id.to_owned(), event_id.to_owned()]
    );
    assert_eq!(sink.confirmed_ids(), vec![event_id.to_owned()]);
}
