use std::{
    sync::{Arc, Condvar, Mutex, atomic::AtomicUsize, atomic::Ordering},
    time::Duration,
};

use chrono::{TimeZone, Utc};
use piwork_lib::{
    assignment::event_outbox::AssignmentEventSink,
    assignment::repository::{AcceptAssignmentInput, AssignmentRepository},
    assignment::state_machine::{
        AssignmentAction, RecoveryDecision, recovery_decision, retry_delay, transition,
    },
    domain::assignment::{AssignmentSideEffect, AssignmentStatus},
    domain::{
        assignment::AssignmentKind,
        event::{WorkEventEnvelope, WorkEventPayload},
    },
    error::AppError,
    storage::sqlite::Database,
};
use serde_json::json;

#[derive(Default)]
struct RecordingSink(Mutex<Vec<WorkEventEnvelope>>);

impl AssignmentEventSink for RecordingSink {
    fn publish(&self, event: WorkEventEnvelope) -> Result<(), AppError> {
        self.0.lock().unwrap().push(event);
        Ok(())
    }
}

impl RecordingSink {
    fn event_ids(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| event.event_id.clone())
            .collect()
    }
}

#[derive(Default)]
struct BlockingSink {
    call_count: AtomicUsize,
    first_started: (Mutex<bool>, Condvar),
    release_first: (Mutex<bool>, Condvar),
    published: Mutex<Vec<WorkEventEnvelope>>,
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
        self.published.lock().unwrap().push(event);
        Ok(())
    }
}

async fn seed_work(pool: &sqlx::SqlitePool, work_id: &str) {
    let now = Utc.with_ymd_and_hms(2026, 8, 15, 10, 0, 0).unwrap();
    sqlx::query("INSERT INTO works (id, title, goal, root_path, permission_mode, status, created_at, updated_at) VALUES (?, 'Assignment test', 'test', '.', 'balanced', 'draft', ?, ?)")
        .bind(work_id).bind(now).bind(now).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO work_agents (work_id, agent_instance_id, role_kind, status, permission_policy, joined_at, updated_at) VALUES (?, 'agent-instance:piwork-lead', 'lead', 'joined', 'inherit_work', ?, ?)")
        .bind(work_id).bind(now).bind(now).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO work_leads (work_id, agent_instance_id, created_at) VALUES (?, 'agent-instance:piwork-lead', ?)")
        .bind(work_id).bind(now).execute(pool).await.unwrap();
}

fn accept_input(work_id: &str, title: &str) -> AcceptAssignmentInput {
    AcceptAssignmentInput {
        id: None,
        work_id: work_id.into(),
        parent_assignment_id: None,
        created_by_agent_id: None,
        assigned_agent_id: "agent-instance:piwork-lead".into(),
        capability_pack_id: None,
        kind: AssignmentKind::Lead,
        side_effect: AssignmentSideEffect::ReadOnly,
        title: title.into(),
        instruction: "Perform the assignment".into(),
        context_manifest: json!({}),
        expected_result_schema: json!({}),
        acceptance_criteria: json!([]),
        permission_scope: json!({"mode": "inherit_work"}),
        priority: 10,
        max_attempts: 3,
        not_before: None,
    }
}

fn json_object_with_serialized_bytes(bytes: usize) -> serde_json::Value {
    let value = json!({"v": "x".repeat(bytes - 8)});
    assert_eq!(serde_json::to_string(&value).unwrap().len(), bytes);
    value
}

fn assert_too_large(error: AppError, expected_field: &str) {
    match error {
        AppError::InvalidInput { field, message } => {
            assert_eq!(field, expected_field);
            assert!(
                message.contains("too large"),
                "unexpected message: {message}"
            );
        }
        other => panic!("expected bounded input error, got {other:?}"),
    }
}

#[tokio::test]
async fn repository_accept_commits_queued_assignment_and_journal_before_publish_without_run() {
    let temporary = tempfile::tempdir().unwrap();
    let database = Database::open(temporary.path().join("assignment.db"))
        .await
        .unwrap();
    seed_work(database.pool(), "work-accept").await;
    let sink = Arc::new(RecordingSink::default());
    let repository = AssignmentRepository::with_event_sink(database.pool().clone(), sink.clone());

    let assignment = repository
        .accept(accept_input("work-accept", "Accepted"))
        .await
        .unwrap();

    assert_eq!(assignment.status, AssignmentStatus::Queued);
    assert_eq!(assignment.attempt_count, 0);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM runs WHERE assignment_id = ?")
            .bind(&assignment.id)
            .fetch_one(database.pool())
            .await
            .unwrap(),
        0
    );
    let events = repository
        .events_for_assignment(&assignment.id)
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].run_id, None);
    assert_eq!(events[0].sequence, 1);
    assert!(matches!(
        events[0].payload,
        WorkEventPayload::AssignmentQueued { .. }
    ));
    assert_eq!(
        sink.event_ids(),
        events
            .iter()
            .filter_map(|event| event.event_id.clone())
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn work_detail_hydration_includes_durable_runless_assignment_events() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-assignment-hydration").await;
    let assignments = AssignmentRepository::new(database.pool().clone());
    let assignment = assignments
        .accept(accept_input(
            "work-assignment-hydration",
            "Hydrated assignment",
        ))
        .await
        .unwrap();

    let works = piwork_lib::work::repository::WorkRepository::new(database.pool().clone());
    let detail = works
        .get("work-assignment-hydration")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(detail.events.len(), 1);
    assert_eq!(
        detail.events[0].assignment_id.as_deref(),
        Some(assignment.id.as_str())
    );
    assert!(detail.events[0].run_id.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repository_commit_during_active_drain_is_published_without_an_external_trigger() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("assignment-outbox-active-drain.db");
    let first_database = Database::open(&path).await.unwrap();
    let second_database = Database::open(&path).await.unwrap();
    seed_work(first_database.pool(), "work-outbox-active-drain").await;
    let sink = Arc::new(BlockingSink::default());
    let first = AssignmentRepository::with_event_sink(first_database.pool().clone(), sink.clone());
    let second =
        AssignmentRepository::with_event_sink(second_database.pool().clone(), sink.clone());

    let first_accept = tokio::spawn(async move {
        first
            .accept(accept_input("work-outbox-active-drain", "First"))
            .await
            .unwrap()
    });
    let started_sink = sink.clone();
    tokio::task::spawn_blocking(move || started_sink.wait_until_first_publish_starts())
        .await
        .unwrap();

    let mut second_accept = tokio::spawn(async move {
        second
            .accept(accept_input("work-outbox-active-drain", "Second"))
            .await
            .unwrap()
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let row_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM assignments")
                .fetch_one(first_database.pool())
                .await
                .unwrap();
            if row_count == 2 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the second mutation did not commit while the first sink was blocked");

    let returned_while_first_publish_was_blocked =
        tokio::time::timeout(Duration::from_millis(250), &mut second_accept).await;
    sink.release_first_publish();
    first_accept.await.unwrap();
    match returned_while_first_publish_was_blocked {
        Ok(result) => {
            result.unwrap();
            panic!("the second mutation returned without draining its newly committed event");
        }
        Err(_) => {
            second_accept.await.unwrap();
        }
    }

    let published_ids: Vec<String> = sink
        .published
        .lock()
        .unwrap()
        .iter()
        .filter_map(|event| event.event_id.clone())
        .collect();
    assert_eq!(published_ids.len(), 2);
}

#[tokio::test]
async fn repository_state_mutations_return_committed_facts_when_every_publication_fails() {
    let database = Database::open_in_memory().await.unwrap();
    for work_id in [
        "work-publish-waiting",
        "work-publish-complete",
        "work-publish-dead",
        "work-publish-confirm",
    ] {
        seed_work(database.pool(), work_id).await;
    }
    let repository = AssignmentRepository::new(database.pool().clone());

    let waiting = repository
        .accept(accept_input("work-publish-waiting", "Waiting"))
        .await
        .unwrap();
    let waiting_now = Utc::now();
    repository
        .claim(&waiting.id, "owner-w", waiting_now)
        .await
        .unwrap();
    let waiting_run = repository
        .begin_attempt(&waiting.id, "pi", "model")
        .await
        .unwrap();
    repository
        .mark_running(
            &waiting.id,
            &waiting_run.id,
            "session-w",
            "owner-w",
            waiting_now,
        )
        .await
        .unwrap();
    assert_eq!(
        repository
            .mark_waiting(
                &waiting.id,
                &waiting_run.id,
                "session-w",
                "owner-w",
                "approval",
                waiting_now,
            )
            .await
            .unwrap()
            .status,
        AssignmentStatus::Waiting
    );

    let completed = repository
        .accept(accept_input("work-publish-complete", "Complete"))
        .await
        .unwrap();
    let completed_now = Utc::now();
    repository
        .claim(&completed.id, "owner-c", completed_now)
        .await
        .unwrap();
    let completed_run = repository
        .begin_attempt(&completed.id, "pi", "model")
        .await
        .unwrap();
    repository
        .mark_running(
            &completed.id,
            &completed_run.id,
            "session-c",
            "owner-c",
            completed_now,
        )
        .await
        .unwrap();
    assert_eq!(
        repository
            .complete(
                &completed.id,
                &completed_run.id,
                "session-c",
                "owner-c",
                "done",
                completed_now,
            )
            .await
            .unwrap()
            .status,
        AssignmentStatus::Completed
    );

    let dead = repository
        .accept(accept_input("work-publish-dead", "Dead letter"))
        .await
        .unwrap();
    let dead_now = Utc::now();
    repository
        .claim(&dead.id, "owner-d", dead_now)
        .await
        .unwrap();
    let dead_run = repository
        .begin_attempt(&dead.id, "pi", "model")
        .await
        .unwrap();
    repository
        .mark_running(&dead.id, &dead_run.id, "session-d", "owner-d", dead_now)
        .await
        .unwrap();
    assert_eq!(
        repository
            .dead_letter(
                &dead.id,
                &dead_run.id,
                "session-d",
                "owner-d",
                "terminal",
                dead_now,
            )
            .await
            .unwrap()
            .status,
        AssignmentStatus::DeadLetter
    );

    let mut confirmation_input = accept_input("work-publish-confirm", "Confirmation");
    confirmation_input.side_effect = AssignmentSideEffect::NonIdempotentWrite;
    let confirmation = repository.accept(confirmation_input).await.unwrap();
    let confirmation_now = Utc::now();
    repository
        .claim(&confirmation.id, "owner-r", confirmation_now)
        .await
        .unwrap();
    let confirmation_run = repository
        .begin_attempt(&confirmation.id, "pi", "model")
        .await
        .unwrap();
    repository
        .mark_running(
            &confirmation.id,
            &confirmation_run.id,
            "session-r",
            "owner-r",
            confirmation_now,
        )
        .await
        .unwrap();
    repository.recover_orphans(&[]).await.unwrap();
    assert_eq!(
        repository
            .confirm_recovery(&confirmation.id, false, Utc::now())
            .await
            .unwrap()
            .status,
        AssignmentStatus::Cancelled
    );
}

#[tokio::test]
async fn repository_assignment_events_use_v2_in_database_live_sink_and_outbox() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-event-v2").await;
    let live_sink = Arc::new(RecordingSink::default());
    let repository =
        AssignmentRepository::with_event_sink(database.pool().clone(), live_sink.clone());
    let assignment = repository
        .accept(accept_input("work-event-v2", "Event v2"))
        .await
        .unwrap();
    let now = Utc::now();
    repository
        .claim(&assignment.id, "owner", now)
        .await
        .unwrap();
    let run = repository
        .begin_attempt(&assignment.id, "pi", "model")
        .await
        .unwrap();
    repository
        .mark_running(&assignment.id, &run.id, "session", "owner", now)
        .await
        .unwrap();

    let stored_versions = sqlx::query_scalar::<_, i64>(
        "SELECT version FROM events WHERE assignment_id = ? ORDER BY occurred_at, id",
    )
    .bind(&assignment.id)
    .fetch_all(database.pool())
    .await
    .unwrap();
    assert_eq!(stored_versions, vec![2, 2, 2]);
    assert_eq!(live_sink.event_ids().len(), 3);
}

#[tokio::test]
async fn repository_claim_is_atomic_due_dependency_aware_and_one_per_work() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("claim.db");
    let database = Database::open(&path).await.unwrap();
    seed_work(database.pool(), "work-claim").await;
    let repository = AssignmentRepository::new(database.pool().clone());
    let first = repository
        .accept(accept_input("work-claim", "First"))
        .await
        .unwrap();
    let now = Utc::now();
    let mut later_input = accept_input("work-claim", "Later");
    later_input.not_before = Some(now + chrono::Duration::minutes(5));
    let later = repository.accept(later_input).await.unwrap();
    let second = repository
        .accept(accept_input("work-claim", "Second"))
        .await
        .unwrap();

    let schedulable = repository.load_schedulable(now, 10).await.unwrap();
    assert_eq!(
        schedulable
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec![first.id.as_str(), second.id.as_str()]
    );
    assert!(
        repository
            .claim(&later.id, "owner-later", now)
            .await
            .is_err()
    );

    let second_database = Database::open(&path).await.unwrap();
    let second_repository = AssignmentRepository::new(second_database.pool().clone());
    let (left, right) = tokio::join!(
        repository.claim(&first.id, "owner-a", now),
        second_repository.claim(&second.id, "owner-b", now),
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
}

#[tokio::test]
async fn repository_attempt_running_and_complete_keep_real_identity_and_are_idempotent() {
    let temporary = tempfile::tempdir().unwrap();
    let database = Database::open(temporary.path().join("complete.db"))
        .await
        .unwrap();
    seed_work(database.pool(), "work-complete").await;
    let repository = AssignmentRepository::new(database.pool().clone());
    let assignment = repository
        .accept(accept_input("work-complete", "Complete"))
        .await
        .unwrap();
    let now = Utc::now();
    repository
        .claim(&assignment.id, "runtime-owner", now)
        .await
        .unwrap();
    let claim_event = repository
        .events_for_assignment(&assignment.id)
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(claim_event.session_id, None);
    assert!(matches!(
        claim_event.payload,
        WorkEventPayload::AssignmentClaimed {
            agent_session_id: None,
            ..
        }
    ));
    let run = repository
        .begin_attempt(&assignment.id, "pi", "test-model")
        .await
        .unwrap();
    assert_eq!(run.work_id, assignment.work_id);
    assert_eq!(run.assignment_id.as_deref(), Some(assignment.id.as_str()));
    assert_eq!(
        run.agent_instance_id.as_deref(),
        Some(assignment.assigned_agent_id.as_str())
    );
    repository
        .mark_running(
            &assignment.id,
            &run.id,
            "session-real",
            "runtime-owner",
            now,
        )
        .await
        .unwrap();
    let completed = repository
        .complete(
            &assignment.id,
            &run.id,
            "session-real",
            "runtime-owner",
            "done",
            now,
        )
        .await
        .unwrap();
    assert_eq!(completed.status, AssignmentStatus::Completed);
    repository
        .complete(
            &assignment.id,
            &run.id,
            "session-real",
            "runtime-owner",
            "done",
            now,
        )
        .await
        .unwrap();
    assert!(
        repository
            .complete(
                &assignment.id,
                &run.id,
                "session-real",
                "runtime-owner",
                "different",
                now
            )
            .await
            .is_err()
    );
    let event_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE run_id = ? AND json_extract(payload, '$.type') = 'assignmentCompleted'").bind(&run.id).fetch_one(database.pool()).await.unwrap();
    assert_eq!(event_count, 1);
}

#[tokio::test]
async fn repository_begin_attempt_rejects_a_second_active_run_without_leaving_a_row() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-active-attempt").await;
    let repository = AssignmentRepository::new(database.pool().clone());
    let assignment = repository
        .accept(accept_input("work-active-attempt", "Attempt guard"))
        .await
        .unwrap();
    repository
        .claim(&assignment.id, "owner", Utc::now())
        .await
        .unwrap();
    repository
        .begin_attempt(&assignment.id, "pi", "model")
        .await
        .unwrap();

    assert!(
        repository
            .begin_attempt(&assignment.id, "pi", "model")
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM runs WHERE assignment_id = ?")
            .bind(&assignment.id)
            .fetch_one(database.pool())
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn repository_mark_running_rejects_an_old_attempt_even_if_its_run_looks_queued() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-old-attempt").await;
    let repository = AssignmentRepository::new(database.pool().clone());
    let assignment = repository
        .accept(accept_input("work-old-attempt", "Old attempt"))
        .await
        .unwrap();
    let now = Utc::now();
    repository
        .claim(&assignment.id, "owner", now)
        .await
        .unwrap();
    let old_run = repository
        .begin_attempt(&assignment.id, "pi", "model")
        .await
        .unwrap();
    repository
        .mark_running(&assignment.id, &old_run.id, "old-session", "owner", now)
        .await
        .unwrap();
    let queued = repository
        .fail_and_schedule_retry(
            &assignment.id,
            &old_run.id,
            "old-session",
            "owner",
            "retry",
            now,
            Duration::from_secs(1),
            Duration::from_secs(10),
        )
        .await
        .unwrap();
    let retry_at = queued.next_attempt_at.unwrap();
    repository
        .claim(&assignment.id, "owner", retry_at)
        .await
        .unwrap();
    let current_run = repository
        .begin_attempt(&assignment.id, "pi", "model")
        .await
        .unwrap();
    sqlx::query("UPDATE runs SET status = 'queued', engine_session_id = NULL, completed_at = NULL WHERE id = ?")
        .bind(&old_run.id)
        .execute(database.pool())
        .await
        .unwrap();

    assert!(
        repository
            .mark_running(
                &assignment.id,
                &old_run.id,
                "forged-session",
                "owner",
                retry_at,
            )
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM runs WHERE id = ?")
            .bind(&current_run.id)
            .fetch_one(database.pool())
            .await
            .unwrap(),
        "queued"
    );
}

#[tokio::test]
async fn repository_startup_failure_retries_then_dead_letters_and_never_strands() {
    let database = Database::open_in_memory().await.unwrap();
    let pool = database.pool().clone();
    let repository = AssignmentRepository::new(pool.clone());
    seed_work(&pool, "work-startup-fail").await;
    let mut input = accept_input("work-startup-fail", "Startup failure");
    input.max_attempts = 2;
    let max_attempts = input.max_attempts;
    let assignment = repository.accept(input).await.unwrap();

    // Each startup failure happens while the Assignment is claimed and its Run
    // is still queued (the engine never started).
    let mut claim_at = Utc::now();
    for attempt in 1..=max_attempts {
        repository
            .claim(&assignment.id, "owner", claim_at)
            .await
            .unwrap();
        let run = repository
            .begin_attempt(&assignment.id, "pi", "model")
            .await
            .unwrap();
        let finalized = repository
            .fail_startup_attempt(
                &assignment.id,
                &run.id,
                "session-startup",
                "owner",
                "engine failed to start",
                claim_at,
                Duration::from_secs(1),
                Duration::from_secs(10),
            )
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT status FROM runs WHERE id = ?")
                .bind(&run.id)
                .fetch_one(&pool)
                .await
                .unwrap(),
            "failed",
            "startup-failed Run must leave a durable failed row"
        );
        if attempt < max_attempts {
            assert_eq!(finalized.status, AssignmentStatus::Queued);
            assert_eq!(
                finalized.last_error.as_deref(),
                Some("engine failed to start")
            );
            assert!(
                finalized.next_attempt_at.is_some(),
                "retry must be scheduled after a startup failure"
            );
            // Advance past the scheduled retry before the next claim.
            claim_at = finalized.next_attempt_at.unwrap() + chrono::Duration::seconds(1);
        } else {
            assert_eq!(finalized.status, AssignmentStatus::DeadLetter);
        }
    }

    let rows: Vec<String> =
        sqlx::query_scalar("SELECT payload FROM events WHERE work_id = ? ORDER BY rowid")
            .bind("work-startup-fail")
            .fetch_all(&pool)
            .await
            .unwrap();
    let payloads: Vec<String> = rows
        .iter()
        .map(|row| {
            serde_json::from_str::<serde_json::Value>(row)
                .unwrap()
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown")
                .to_owned()
        })
        .collect();
    let failed = payloads
        .iter()
        .filter(|kind| kind.as_str() == "assignmentFailed")
        .count();
    let retried = payloads
        .iter()
        .filter(|kind| kind.as_str() == "assignmentRetryScheduled")
        .count();
    let dead_lettered = payloads
        .iter()
        .filter(|kind| kind.as_str() == "assignmentDeadLettered")
        .count();
    assert_eq!(
        failed, 2,
        "every startup failure journals assignmentFailed: {payloads:?}"
    );
    assert_eq!(
        retried, 1,
        "the first startup failure schedules a retry: {payloads:?}"
    );
    assert_eq!(
        dead_lettered, 1,
        "the exhausted startup failure dead-letters: {payloads:?}"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM assignments WHERE id = ?")
            .bind(&assignment.id)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "dead_letter",
        "the Assignment must never stay claimed after startup failures"
    );
}

#[tokio::test]
async fn repository_retry_backoff_and_dead_letter_respect_attempt_limit() {
    let temporary = tempfile::tempdir().unwrap();
    let database = Database::open(temporary.path().join("retry.db"))
        .await
        .unwrap();
    seed_work(database.pool(), "work-retry").await;
    let repository = AssignmentRepository::new(database.pool().clone());
    let mut input = accept_input("work-retry", "Retry");
    input.max_attempts = 2;
    let assignment = repository.accept(input).await.unwrap();
    let now = Utc::now();
    repository
        .claim(&assignment.id, "owner", now)
        .await
        .unwrap();
    let run = repository
        .begin_attempt(&assignment.id, "pi", "model")
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT attempt_number FROM runs WHERE id = ?")
            .bind(&run.id)
            .fetch_one(database.pool())
            .await
            .unwrap(),
        1
    );
    repository
        .mark_running(&assignment.id, &run.id, "session-1", "owner", now)
        .await
        .unwrap();
    let queued = repository
        .fail_and_schedule_retry(
            &assignment.id,
            &run.id,
            "session-1",
            "owner",
            "temporary",
            now,
            Duration::from_secs(2),
            Duration::from_secs(30),
        )
        .await
        .unwrap();
    assert_eq!(queued.status, AssignmentStatus::Queued);
    assert!(queued.next_attempt_at.unwrap() >= now);
    let next = queued.next_attempt_at.unwrap();
    repository
        .claim(&assignment.id, "owner", next)
        .await
        .unwrap();
    let second_run = repository
        .begin_attempt(&assignment.id, "pi", "model")
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT attempt_number FROM runs WHERE id = ?")
            .bind(&second_run.id)
            .fetch_one(database.pool())
            .await
            .unwrap(),
        2
    );
    repository
        .mark_running(&assignment.id, &second_run.id, "session-2", "owner", next)
        .await
        .unwrap();
    assert!(
        repository
            .fail_and_schedule_retry(
                &assignment.id,
                &second_run.id,
                "session-2",
                "owner",
                "again",
                next,
                Duration::from_secs(2),
                Duration::from_secs(30)
            )
            .await
            .is_err()
    );
    let dead = repository
        .dead_letter(
            &assignment.id,
            &second_run.id,
            "session-2",
            "owner",
            "exhausted",
            next,
        )
        .await
        .unwrap();
    assert_eq!(dead.status, AssignmentStatus::DeadLetter);
    assert_eq!(dead.attempt_count, dead.max_attempts);
    assert!(
        repository
            .dead_letter(
                &assignment.id,
                &second_run.id,
                "session-2",
                "owner",
                "exhausted",
                next
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn repository_dependencies_require_same_work_and_terminal_predecessor() {
    let temporary = tempfile::tempdir().unwrap();
    let database = Database::open(temporary.path().join("deps.db"))
        .await
        .unwrap();
    seed_work(database.pool(), "work-deps").await;
    seed_work(database.pool(), "other-work").await;
    let repository = AssignmentRepository::new(database.pool().clone());
    let predecessor = repository
        .accept(accept_input("work-deps", "Pred"))
        .await
        .unwrap();
    let dependent = repository
        .accept(accept_input("work-deps", "Dependent"))
        .await
        .unwrap();
    let cross = repository
        .accept(accept_input("other-work", "Cross"))
        .await
        .unwrap();
    assert!(
        repository
            .add_dependency(&dependent.id, &dependent.id)
            .await
            .is_err()
    );
    assert!(
        repository
            .add_dependency(&dependent.id, &cross.id)
            .await
            .is_err()
    );
    repository
        .add_dependency(&dependent.id, &predecessor.id)
        .await
        .unwrap();
    assert!(
        !repository
            .dependencies_terminal(&dependent.id)
            .await
            .unwrap()
    );
    assert!(
        repository
            .claim(&dependent.id, "owner", Utc::now())
            .await
            .is_err()
    );
    let now = Utc::now();
    repository
        .claim(&predecessor.id, "pred-owner", now)
        .await
        .unwrap();
    let run = repository
        .begin_attempt(&predecessor.id, "pi", "model")
        .await
        .unwrap();
    repository
        .mark_running(&predecessor.id, &run.id, "pred-session", "pred-owner", now)
        .await
        .unwrap();
    repository
        .complete(
            &predecessor.id,
            &run.id,
            "pred-session",
            "pred-owner",
            "done",
            now,
        )
        .await
        .unwrap();
    assert!(
        repository
            .dependencies_terminal(&dependent.id)
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn repository_dependency_rejects_a_direct_cycle_without_partial_edge() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-direct-cycle").await;
    let repository = AssignmentRepository::new(database.pool().clone());
    let first = repository
        .accept(accept_input("work-direct-cycle", "First"))
        .await
        .unwrap();
    let second = repository
        .accept(accept_input("work-direct-cycle", "Second"))
        .await
        .unwrap();
    repository
        .add_dependency(&first.id, &second.id)
        .await
        .unwrap();

    assert!(
        repository
            .add_dependency(&second.id, &first.id)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM assignment_dependencies")
            .fetch_one(database.pool())
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn repository_dependency_rejects_a_multi_hop_cycle_without_partial_edge() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-multi-cycle").await;
    let repository = AssignmentRepository::new(database.pool().clone());
    let first = repository
        .accept(accept_input("work-multi-cycle", "First"))
        .await
        .unwrap();
    let second = repository
        .accept(accept_input("work-multi-cycle", "Second"))
        .await
        .unwrap();
    let third = repository
        .accept(accept_input("work-multi-cycle", "Third"))
        .await
        .unwrap();
    repository
        .add_dependency(&first.id, &second.id)
        .await
        .unwrap();
    repository
        .add_dependency(&second.id, &third.id)
        .await
        .unwrap();

    assert!(
        repository
            .add_dependency(&third.id, &first.id)
            .await
            .is_err()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM assignment_dependencies")
            .fetch_one(database.pool())
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn repository_dependency_concurrent_cycle_allows_exactly_one_edge() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("concurrent-cycle.db");
    let first_database = Database::open(&path).await.unwrap();
    let second_database = Database::open(&path).await.unwrap();
    seed_work(first_database.pool(), "work-concurrent-cycle").await;
    let setup = AssignmentRepository::new(first_database.pool().clone());
    let first = setup
        .accept(accept_input("work-concurrent-cycle", "First"))
        .await
        .unwrap();
    let second = setup
        .accept(accept_input("work-concurrent-cycle", "Second"))
        .await
        .unwrap();
    let first_repository = AssignmentRepository::new(first_database.pool().clone());
    let second_repository = AssignmentRepository::new(second_database.pool().clone());

    let (forward, reverse) = tokio::join!(
        first_repository.add_dependency(&first.id, &second.id),
        second_repository.add_dependency(&second.id, &first.id),
    );

    assert_eq!(
        usize::from(forward.is_ok()) + usize::from(reverse.is_ok()),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM assignment_dependencies")
            .fetch_one(first_database.pool())
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn repository_owner_session_and_run_mismatches_fail_closed() {
    let temporary = tempfile::tempdir().unwrap();
    let database = Database::open(temporary.path().join("identity.db"))
        .await
        .unwrap();
    seed_work(database.pool(), "work-identity").await;
    let repository = AssignmentRepository::new(database.pool().clone());
    let assignment = repository
        .accept(accept_input("work-identity", "Identity"))
        .await
        .unwrap();
    let now = Utc::now();
    repository
        .claim(&assignment.id, "owner", now)
        .await
        .unwrap();
    let run = repository
        .begin_attempt(&assignment.id, "pi", "model")
        .await
        .unwrap();
    assert!(
        repository
            .mark_running(&assignment.id, &run.id, "session", "wrong-owner", now)
            .await
            .is_err()
    );
    repository
        .mark_running(&assignment.id, &run.id, "session", "owner", now)
        .await
        .unwrap();
    assert!(
        repository
            .complete(
                &assignment.id,
                &run.id,
                "wrong-session",
                "owner",
                "done",
                now
            )
            .await
            .is_err()
    );
    assert!(
        repository
            .complete(&assignment.id, "wrong-run", "session", "owner", "done", now)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn repository_mark_waiting_updates_assignment_run_and_real_identity_event() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-waiting").await;
    let repository = AssignmentRepository::new(database.pool().clone());
    let assignment = repository
        .accept(accept_input("work-waiting", "Waiting"))
        .await
        .unwrap();
    let now = Utc::now();
    repository
        .claim(&assignment.id, "owner", now)
        .await
        .unwrap();
    let run = repository
        .begin_attempt(&assignment.id, "pi", "model")
        .await
        .unwrap();
    repository
        .mark_running(&assignment.id, &run.id, "real-session", "owner", now)
        .await
        .unwrap();
    let waiting = repository
        .mark_waiting(
            &assignment.id,
            &run.id,
            "real-session",
            "owner",
            "approval",
            now,
        )
        .await
        .unwrap();
    assert_eq!(waiting.status, AssignmentStatus::Waiting);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM runs WHERE id = ?")
            .bind(&run.id)
            .fetch_one(database.pool())
            .await
            .unwrap(),
        "waiting"
    );
    let payload: String = sqlx::query_scalar(
        "SELECT payload FROM events WHERE run_id = ? ORDER BY sequence DESC LIMIT 1",
    )
    .bind(&run.id)
    .fetch_one(database.pool())
    .await
    .unwrap();
    let payload: WorkEventPayload = serde_json::from_str(&payload).unwrap();
    assert!(
        matches!(payload, WorkEventPayload::AssignmentWaiting { agent_session_id, reason, .. } if agent_session_id == "real-session" && reason == "approval")
    );
}

#[tokio::test]
async fn repository_accept_rejects_invalid_or_unbounded_json_without_rows() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-validation").await;
    let repository = AssignmentRepository::new(database.pool().clone());
    let mut invalid = accept_input("work-validation", "Invalid");
    invalid.context_manifest = json!([]);
    assert!(repository.accept(invalid).await.is_err());
    let mut oversized = accept_input("work-validation", "Oversized");
    oversized.permission_scope = json!({ "value": "x".repeat(1024 * 1024) });
    assert!(repository.accept(oversized).await.is_err());
    assert_eq!(
        repository
            .list_for_work("work-validation")
            .await
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn repository_string_byte_limits_accept_exact_boundaries() {
    const SHORT: usize = 255;
    const TEXT: usize = 64 * 1024;
    const JSON: usize = 1024 * 1024;

    let database = Database::open_in_memory().await.unwrap();
    let work_id = "w".repeat(SHORT);
    seed_work(database.pool(), &work_id).await;
    let repository = AssignmentRepository::new(database.pool().clone());
    let mut input = accept_input(&work_id, &"t".repeat(TEXT));
    input.id = Some("a".repeat(SHORT));
    input.instruction = "i".repeat(TEXT);
    input.context_manifest = json_object_with_serialized_bytes(JSON);

    let assignment = repository.accept(input).await.unwrap();
    let now = Utc::now();
    repository
        .claim(&assignment.id, &"o".repeat(SHORT), now)
        .await
        .unwrap();
    let run = repository
        .begin_attempt(&assignment.id, &"e".repeat(SHORT), &"m".repeat(SHORT))
        .await
        .unwrap();
    repository
        .mark_running(
            &assignment.id,
            &run.id,
            &"s".repeat(SHORT),
            &"o".repeat(SHORT),
            now,
        )
        .await
        .unwrap();
    let completed = repository
        .complete(
            &assignment.id,
            &run.id,
            &"s".repeat(SHORT),
            &"o".repeat(SHORT),
            &"r".repeat(TEXT),
            now,
        )
        .await
        .unwrap();

    assert_eq!(completed.status, AssignmentStatus::Completed);
    assert_eq!(completed.id.len(), SHORT);
    assert_eq!(completed.title.len(), TEXT);
    assert_eq!(completed.result_summary.unwrap().len(), TEXT);
}

#[tokio::test]
async fn repository_accept_rejects_boundary_plus_one_and_unicode_bytes_without_writes() {
    const SHORT: usize = 255;
    const TEXT: usize = 64 * 1024;
    const JSON: usize = 1024 * 1024;

    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-string-limits").await;
    let sink = Arc::new(RecordingSink::default());
    let repository = AssignmentRepository::with_event_sink(database.pool().clone(), sink.clone());
    let mut cases = Vec::new();

    let mut assignment_id = accept_input("work-string-limits", "Assignment ID");
    assignment_id.id = Some("a".repeat(SHORT + 1));
    cases.push((assignment_id, "id"));

    let unicode_work_id = accept_input(&"é".repeat(128), "Unicode Work ID");
    cases.push((unicode_work_id, "workId"));

    let mut parent_id = accept_input("work-string-limits", "Parent ID");
    parent_id.parent_assignment_id = Some("p".repeat(SHORT + 1));
    cases.push((parent_id, "parentAssignmentId"));

    let mut creator_id = accept_input("work-string-limits", "Creator ID");
    creator_id.created_by_agent_id = Some("c".repeat(SHORT + 1));
    cases.push((creator_id, "createdByAgentId"));

    let mut assigned_id = accept_input("work-string-limits", "Assigned ID");
    assigned_id.assigned_agent_id = "g".repeat(SHORT + 1);
    cases.push((assigned_id, "assignedAgentId"));

    let mut capability_id = accept_input("work-string-limits", "Capability ID");
    capability_id.capability_pack_id = Some("k".repeat(SHORT + 1));
    cases.push((capability_id, "capabilityPackId"));

    let title = accept_input("work-string-limits", &"t".repeat(TEXT + 1));
    cases.push((title, "title"));

    let mut unicode_instruction = accept_input("work-string-limits", "Instruction");
    unicode_instruction.instruction = "é".repeat(TEXT / 2 + 1);
    cases.push((unicode_instruction, "instruction"));

    let mut context = accept_input("work-string-limits", "Context");
    context.context_manifest = json_object_with_serialized_bytes(JSON + 1);
    cases.push((context, "contextManifest"));

    for (input, field) in cases {
        assert_too_large(repository.accept(input).await.unwrap_err(), field);
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM assignments")
            .fetch_one(database.pool())
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM events")
            .fetch_one(database.pool())
            .await
            .unwrap(),
        0
    );
    assert!(sink.event_ids().is_empty());
}

#[tokio::test]
async fn repository_runtime_string_limits_reject_before_database_or_event_writes() {
    const SHORT: usize = 255;
    const TEXT: usize = 64 * 1024;

    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-owner-limit").await;
    seed_work(database.pool(), "work-runtime-limits").await;
    let sink = Arc::new(RecordingSink::default());
    let repository = AssignmentRepository::with_event_sink(database.pool().clone(), sink.clone());

    let owner_assignment = repository
        .accept(accept_input("work-owner-limit", "Owner limit"))
        .await
        .unwrap();
    let owner_event_count = sink.event_ids().len();
    assert_too_large(
        repository
            .claim(&owner_assignment.id, &"o".repeat(SHORT + 1), Utc::now())
            .await
            .unwrap_err(),
        "runtimeOwner",
    );
    assert_eq!(
        repository.list_for_work("work-owner-limit").await.unwrap()[0].status,
        AssignmentStatus::Queued
    );
    assert_eq!(sink.event_ids().len(), owner_event_count);

    let assignment = repository
        .accept(accept_input("work-runtime-limits", "Runtime limits"))
        .await
        .unwrap();
    let now = Utc::now();
    repository
        .claim(&assignment.id, "owner", now)
        .await
        .unwrap();
    assert_too_large(
        repository
            .begin_attempt(&assignment.id, &"e".repeat(SHORT + 1), "model")
            .await
            .unwrap_err(),
        "engineKind",
    );
    assert_too_large(
        repository
            .begin_attempt(&assignment.id, "engine", &"é".repeat(128))
            .await
            .unwrap_err(),
        "modelLabel",
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM runs WHERE assignment_id = ?")
            .bind(&assignment.id)
            .fetch_one(database.pool())
            .await
            .unwrap(),
        0
    );

    let run = repository
        .begin_attempt(&assignment.id, "engine", "model")
        .await
        .unwrap();
    assert_too_large(
        repository
            .mark_running(&"a".repeat(SHORT + 1), &run.id, "session", "owner", now)
            .await
            .unwrap_err(),
        "assignmentId",
    );
    assert_too_large(
        repository
            .mark_running(
                &assignment.id,
                &"r".repeat(SHORT + 1),
                "session",
                "owner",
                now,
            )
            .await
            .unwrap_err(),
        "runId",
    );
    assert_too_large(
        repository
            .mark_running(
                &assignment.id,
                &run.id,
                "session",
                &"o".repeat(SHORT + 1),
                now,
            )
            .await
            .unwrap_err(),
        "runtimeOwner",
    );
    assert_too_large(
        repository
            .mark_running(&assignment.id, &run.id, &"é".repeat(128), "owner", now)
            .await
            .unwrap_err(),
        "sessionId",
    );
    repository
        .mark_running(&assignment.id, &run.id, "session", "owner", now)
        .await
        .unwrap();
    let event_count = sink.event_ids().len();

    assert_too_large(
        repository
            .mark_waiting(
                &assignment.id,
                &run.id,
                "session",
                "owner",
                &"q".repeat(TEXT + 1),
                now,
            )
            .await
            .unwrap_err(),
        "reason",
    );
    assert_too_large(
        repository
            .complete(
                &assignment.id,
                &run.id,
                "session",
                "owner",
                &"r".repeat(TEXT + 1),
                now,
            )
            .await
            .unwrap_err(),
        "resultSummary",
    );
    assert_too_large(
        repository
            .fail_and_schedule_retry(
                &assignment.id,
                &run.id,
                "session",
                "owner",
                &"é".repeat(TEXT / 2 + 1),
                now,
                Duration::from_secs(1),
                Duration::from_secs(10),
            )
            .await
            .unwrap_err(),
        "error",
    );
    assert_too_large(
        repository
            .dead_letter(
                &assignment.id,
                &run.id,
                "session",
                "owner",
                &"d".repeat(TEXT + 1),
                now,
            )
            .await
            .unwrap_err(),
        "error",
    );
    assert_eq!(sink.event_ids().len(), event_count);
    assert_eq!(
        repository
            .list_for_work("work-runtime-limits")
            .await
            .unwrap()[0]
            .status,
        AssignmentStatus::Running
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM runs WHERE id = ?")
            .bind(&run.id)
            .fetch_one(database.pool())
            .await
            .unwrap(),
        "running"
    );

    assert_too_large(
        repository
            .recover_orphans(&["o".repeat(SHORT + 1)])
            .await
            .unwrap_err(),
        "activeOwnerId",
    );
    assert_too_large(
        repository
            .list_for_work(&"w".repeat(SHORT + 1))
            .await
            .unwrap_err(),
        "workId",
    );
    assert_eq!(sink.event_ids().len(), event_count);
}

#[tokio::test]
async fn repository_transaction_failure_rolls_back_assignment_event_and_publish() {
    let temporary = tempfile::tempdir().unwrap();
    let database = Database::open(temporary.path().join("rollback.db"))
        .await
        .unwrap();
    seed_work(database.pool(), "work-rollback").await;
    sqlx::query("CREATE TRIGGER reject_assignment_event BEFORE INSERT ON events BEGIN SELECT RAISE(ABORT, 'forced event failure'); END").execute(database.pool()).await.unwrap();
    let sink = Arc::new(RecordingSink::default());
    let repository = AssignmentRepository::with_event_sink(database.pool().clone(), sink.clone());
    let mut input = accept_input("work-rollback", "Rollback");
    input.id = Some("assignment-rollback".into());
    assert!(repository.accept(input).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM assignments WHERE id = 'assignment-rollback'"
        )
        .fetch_one(database.pool())
        .await
        .unwrap(),
        0
    );
    assert!(sink.event_ids().is_empty());
}

#[tokio::test]
async fn repository_recovery_requeues_safe_orphans_and_requires_confirmation_for_uncertain_writes()
{
    let temporary = tempfile::tempdir().unwrap();
    let database = Database::open(temporary.path().join("recovery.db"))
        .await
        .unwrap();
    let repository = AssignmentRepository::new(database.pool().clone());
    let cases = [
        ("claimed", AssignmentSideEffect::Unknown, false),
        ("readonly", AssignmentSideEffect::ReadOnly, true),
        ("idempotent", AssignmentSideEffect::IdempotentWrite, true),
        (
            "nonidempotent",
            AssignmentSideEffect::NonIdempotentWrite,
            true,
        ),
        ("unknown", AssignmentSideEffect::Unknown, true),
        ("active", AssignmentSideEffect::ReadOnly, true),
    ];
    let mut ids = std::collections::HashMap::new();
    for (name, side_effect, started) in cases {
        let work_id = format!("work-{name}");
        seed_work(database.pool(), &work_id).await;
        let mut input = accept_input(&work_id, name);
        input.side_effect = side_effect;
        let assignment = repository.accept(input).await.unwrap();
        let now = Utc::now();
        let owner = if name == "active" {
            "active-owner"
        } else {
            "orphan-owner"
        };
        repository.claim(&assignment.id, owner, now).await.unwrap();
        if started {
            let run = repository
                .begin_attempt(&assignment.id, "pi", "model")
                .await
                .unwrap();
            repository
                .mark_running(
                    &assignment.id,
                    &run.id,
                    &format!("session-{name}"),
                    owner,
                    now,
                )
                .await
                .unwrap();
        }
        ids.insert(name, assignment.id);
    }

    let report = repository
        .recover_orphans(&["active-owner".into()])
        .await
        .unwrap();
    assert_eq!(report.requeued.len(), 3);
    assert_eq!(report.confirmation_required.len(), 2);
    assert_eq!(report.untouched.len(), 1);
    let now = Utc::now();
    let all = repository
        .list_for_work("work-nonidempotent")
        .await
        .unwrap();
    assert_eq!(
        all[0].status,
        AssignmentStatus::RecoveryConfirmationRequired
    );
    assert_eq!(
        repository
            .confirm_recovery(ids["nonidempotent"].as_str(), true, now)
            .await
            .unwrap()
            .status,
        AssignmentStatus::Queued
    );
    assert_eq!(
        repository
            .confirm_recovery(ids["unknown"].as_str(), false, now)
            .await
            .unwrap()
            .status,
        AssignmentStatus::Cancelled
    );
    assert_eq!(
        repository.list_for_work("work-active").await.unwrap()[0].status,
        AssignmentStatus::Running
    );
}

#[tokio::test]
async fn repository_recovery_dead_letters_a_claimed_queued_run_when_budget_is_exhausted() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-crash-exhausted").await;
    let repository = AssignmentRepository::new(database.pool().clone());
    let mut input = accept_input("work-crash-exhausted", "Crash exhausted");
    input.max_attempts = 1;
    let assignment = repository.accept(input).await.unwrap();
    repository
        .claim(&assignment.id, "orphan-owner", Utc::now())
        .await
        .unwrap();
    let run = repository
        .begin_attempt(&assignment.id, "pi", "model")
        .await
        .unwrap();

    let report = repository.recover_orphans(&[]).await.unwrap();

    let stored = repository
        .list_for_work("work-crash-exhausted")
        .await
        .unwrap();
    assert_eq!(stored[0].status, AssignmentStatus::DeadLetter);
    assert_eq!(report.dead_lettered, vec![assignment.id.clone()]);
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM runs WHERE id = ?")
            .bind(&run.id)
            .fetch_one(database.pool())
            .await
            .unwrap(),
        "interrupted"
    );
    let row: (Option<String>, Option<String>, String) = sqlx::query_as(
        "SELECT run_id, session_id, payload FROM events WHERE run_id = ? ORDER BY sequence LIMIT 1",
    )
    .bind(&run.id)
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(row.0.as_deref(), Some(run.id.as_str()));
    assert_eq!(row.1, None);
    assert!(matches!(
        serde_json::from_str::<WorkEventPayload>(&row.2).unwrap(),
        WorkEventPayload::AssignmentInterrupted {
            agent_session_id: None,
            ..
        }
    ));
}

#[tokio::test]
async fn repository_recovery_retries_a_claimed_queued_run_with_budget_and_audit() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-crash-retry").await;
    let repository = AssignmentRepository::new(database.pool().clone());
    let assignment = repository
        .accept(accept_input("work-crash-retry", "Crash retry"))
        .await
        .unwrap();
    repository
        .claim(&assignment.id, "orphan-owner", Utc::now())
        .await
        .unwrap();
    let run = repository
        .begin_attempt(&assignment.id, "pi", "model")
        .await
        .unwrap();

    repository.recover_orphans(&[]).await.unwrap();

    let stored = &repository.list_for_work("work-crash-retry").await.unwrap()[0];
    assert_eq!(stored.status, AssignmentStatus::Queued);
    assert_eq!(stored.attempt_count, 1);
    assert!(stored.next_attempt_at.is_some());
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM runs WHERE id = ?")
            .bind(&run.id)
            .fetch_one(database.pool())
            .await
            .unwrap(),
        "interrupted"
    );
    let events: Vec<(i64, Option<String>, String)> = sqlx::query_as(
        "SELECT sequence, session_id, payload FROM events WHERE run_id = ? ORDER BY sequence",
    )
    .bind(&run.id)
    .fetch_all(database.pool())
    .await
    .unwrap();
    assert_eq!(
        events.iter().map(|event| event.0).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(events[0].1, None);
    assert!(matches!(
        serde_json::from_str::<WorkEventPayload>(&events[0].2).unwrap(),
        WorkEventPayload::AssignmentInterrupted {
            agent_session_id: None,
            ..
        }
    ));
    assert!(matches!(
        serde_json::from_str::<WorkEventPayload>(&events[1].2).unwrap(),
        WorkEventPayload::AssignmentRetryScheduled {
            attempt_count: 1,
            ..
        }
    ));
}

#[tokio::test]
async fn repository_confirmed_resume_restores_budget_and_becomes_schedulable() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-confirm-resume").await;
    let repository = AssignmentRepository::new(database.pool().clone());
    let mut input = accept_input("work-confirm-resume", "Confirm resume");
    input.max_attempts = 1;
    input.side_effect = AssignmentSideEffect::NonIdempotentWrite;
    let assignment = repository.accept(input).await.unwrap();
    let now = Utc::now();
    repository
        .claim(&assignment.id, "orphan-owner", now)
        .await
        .unwrap();
    let run = repository
        .begin_attempt(&assignment.id, "pi", "model")
        .await
        .unwrap();
    repository
        .mark_running(
            &assignment.id,
            &run.id,
            "uncertain-session",
            "orphan-owner",
            now,
        )
        .await
        .unwrap();
    repository.recover_orphans(&[]).await.unwrap();
    let confirmed_at = Utc::now();

    let resumed = repository
        .confirm_recovery(&assignment.id, true, confirmed_at)
        .await
        .unwrap();

    assert_eq!(resumed.status, AssignmentStatus::Queued);
    assert_eq!(resumed.attempt_count, 1);
    assert_eq!(resumed.max_attempts, 2);
    assert_eq!(
        repository
            .load_schedulable(confirmed_at, 10)
            .await
            .unwrap()
            .iter()
            .map(|candidate| candidate.id.as_str())
            .collect::<Vec<_>>(),
        vec![assignment.id.as_str()]
    );
}

#[tokio::test]
async fn repository_confirmed_cancel_commits_typed_event_with_real_recovery_identity() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-confirm-cancel").await;
    let repository = AssignmentRepository::new(database.pool().clone());
    let mut input = accept_input("work-confirm-cancel", "Confirm cancel");
    input.side_effect = AssignmentSideEffect::Unknown;
    let assignment = repository.accept(input).await.unwrap();
    let now = Utc::now();
    repository
        .claim(&assignment.id, "orphan-owner", now)
        .await
        .unwrap();
    let run = repository
        .begin_attempt(&assignment.id, "pi", "model")
        .await
        .unwrap();
    repository
        .mark_running(
            &assignment.id,
            &run.id,
            "uncertain-session",
            "orphan-owner",
            now,
        )
        .await
        .unwrap();
    repository.recover_orphans(&[]).await.unwrap();
    let confirmed_at = Utc::now();

    let cancelled = repository
        .confirm_recovery(&assignment.id, false, confirmed_at)
        .await
        .unwrap();

    assert_eq!(cancelled.status, AssignmentStatus::Cancelled);
    assert_eq!(cancelled.completed_at, Some(confirmed_at));
    let event: (String, Option<String>, Option<String>, Option<String>, String) = sqlx::query_as(
        "SELECT run_id, session_id, agent_id, assignment_id, payload FROM events WHERE run_id = ? ORDER BY sequence DESC LIMIT 1",
    )
    .bind(&run.id)
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(event.0, run.id);
    assert_eq!(event.1.as_deref(), Some("uncertain-session"));
    assert_eq!(
        event.2.as_deref(),
        Some(assignment.assigned_agent_id.as_str())
    );
    assert_eq!(event.3.as_deref(), Some(assignment.id.as_str()));
    assert!(matches!(
        serde_json::from_str::<WorkEventPayload>(&event.4).unwrap(),
        WorkEventPayload::AssignmentCancelled {
            agent_session_id: Some(session),
            run_id: Some(payload_run),
            ..
        } if session == "uncertain-session" && payload_run == run.id
    ));
}

#[tokio::test]
async fn repository_running_safe_orphan_dead_letters_on_the_last_attempt() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-running-last").await;
    let repository = AssignmentRepository::new(database.pool().clone());
    let mut input = accept_input("work-running-last", "Running last");
    input.max_attempts = 1;
    let assignment = repository.accept(input).await.unwrap();
    let now = Utc::now();
    repository
        .claim(&assignment.id, "orphan-owner", now)
        .await
        .unwrap();
    let run = repository
        .begin_attempt(&assignment.id, "pi", "model")
        .await
        .unwrap();
    repository
        .mark_running(&assignment.id, &run.id, "safe-session", "orphan-owner", now)
        .await
        .unwrap();

    repository.recover_orphans(&[]).await.unwrap();

    let stored = &repository.list_for_work("work-running-last").await.unwrap()[0];
    assert_eq!(stored.status, AssignmentStatus::DeadLetter);
    assert_eq!(stored.attempt_count, stored.max_attempts);
    assert!(
        repository
            .load_schedulable(Utc::now(), 10)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM runs WHERE id = ?")
            .bind(&run.id)
            .fetch_one(database.pool())
            .await
            .unwrap(),
        "interrupted"
    );
}

#[tokio::test]
async fn repository_running_safe_orphan_schedules_deterministic_retry_with_budget() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-running-retry").await;
    let repository = AssignmentRepository::new(database.pool().clone());
    let assignment = repository
        .accept(accept_input("work-running-retry", "Running retry"))
        .await
        .unwrap();
    let now = Utc::now();
    repository
        .claim(&assignment.id, "orphan-owner", now)
        .await
        .unwrap();
    let run = repository
        .begin_attempt(&assignment.id, "pi", "model")
        .await
        .unwrap();
    repository
        .mark_running(&assignment.id, &run.id, "safe-session", "orphan-owner", now)
        .await
        .unwrap();

    repository.recover_orphans(&[]).await.unwrap();

    let stored = &repository
        .list_for_work("work-running-retry")
        .await
        .unwrap()[0];
    let retry_at = stored.next_attempt_at.unwrap();
    assert_eq!(stored.status, AssignmentStatus::Queued);
    assert!(
        repository
            .load_schedulable(retry_at - chrono::Duration::nanoseconds(1), 10)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        repository
            .load_schedulable(retry_at, 10)
            .await
            .unwrap()
            .iter()
            .map(|candidate| candidate.id.as_str())
            .collect::<Vec<_>>(),
        vec![assignment.id.as_str()]
    );
    let payload: String = sqlx::query_scalar(
        "SELECT payload FROM events WHERE run_id = ? ORDER BY sequence DESC LIMIT 1",
    )
    .bind(&run.id)
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert!(matches!(
        serde_json::from_str::<WorkEventPayload>(&payload).unwrap(),
        WorkEventPayload::AssignmentRetryScheduled { next_attempt_at, .. }
            if next_attempt_at == retry_at
    ));
}

#[tokio::test]
async fn repository_schedulable_ignores_legacy_interrupted_unknown_assignments() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-legacy").await;
    let now = Utc::now();
    sqlx::query("INSERT INTO assignments (id, work_id, assigned_agent_id, kind, side_effect, title, instruction, context_manifest_json, expected_result_schema_json, acceptance_criteria_json, permission_scope_json, priority, status, attempt_count, max_attempts, created_at, updated_at) VALUES ('legacy', 'work-legacy', 'agent-instance:piwork-lead', 'lead', 'unknown', 'Legacy', 'Legacy', '{}', '{}', '[]', '{}', 0, 'interrupted', 0, 1, ?, ?)")
        .bind(now).bind(now).execute(database.pool()).await.unwrap();
    let repository = AssignmentRepository::new(database.pool().clone());
    assert!(
        repository
            .load_schedulable(now, 10)
            .await
            .unwrap()
            .is_empty()
    );
}

#[test]
fn queued_can_be_claimed() {
    assert_eq!(
        transition(AssignmentStatus::Queued, AssignmentAction::Claim).unwrap(),
        AssignmentStatus::Claimed,
    );
}

#[test]
fn claimed_can_start_running() {
    assert_eq!(
        transition(AssignmentStatus::Claimed, AssignmentAction::Start).unwrap(),
        AssignmentStatus::Running,
    );
}

#[test]
fn running_can_complete() {
    assert_eq!(
        transition(AssignmentStatus::Running, AssignmentAction::Complete).unwrap(),
        AssignmentStatus::Completed,
    );
}

#[test]
fn running_can_fail() {
    assert_eq!(
        transition(AssignmentStatus::Running, AssignmentAction::Fail).unwrap(),
        AssignmentStatus::Failed
    );
}

#[test]
fn running_can_wait() {
    assert_eq!(
        transition(AssignmentStatus::Running, AssignmentAction::Wait).unwrap(),
        AssignmentStatus::Waiting
    );
}

#[test]
fn running_can_be_cancelled() {
    assert_eq!(
        transition(AssignmentStatus::Running, AssignmentAction::Cancel).unwrap(),
        AssignmentStatus::Cancelled
    );
}

#[test]
fn running_can_be_interrupted() {
    assert_eq!(
        transition(AssignmentStatus::Running, AssignmentAction::Interrupt).unwrap(),
        AssignmentStatus::Interrupted
    );
}

#[test]
fn failed_and_interrupted_can_retry() {
    for current in [AssignmentStatus::Failed, AssignmentStatus::Interrupted] {
        assert_eq!(
            transition(current, AssignmentAction::Retry).unwrap(),
            AssignmentStatus::Queued
        );
    }
}

#[test]
fn failed_and_interrupted_can_dead_letter() {
    for current in [AssignmentStatus::Failed, AssignmentStatus::Interrupted] {
        assert_eq!(
            transition(current, AssignmentAction::DeadLetter).unwrap(),
            AssignmentStatus::DeadLetter
        );
    }
}

#[test]
fn interrupted_can_require_recovery_confirmation() {
    assert_eq!(
        transition(
            AssignmentStatus::Interrupted,
            AssignmentAction::RequireRecoveryConfirmation
        )
        .unwrap(),
        AssignmentStatus::RecoveryConfirmationRequired
    );
}

#[test]
fn recovery_confirmation_can_resume_or_cancel() {
    assert_eq!(
        transition(
            AssignmentStatus::RecoveryConfirmationRequired,
            AssignmentAction::ConfirmResume
        )
        .unwrap(),
        AssignmentStatus::Queued
    );
    assert_eq!(
        transition(
            AssignmentStatus::RecoveryConfirmationRequired,
            AssignmentAction::Cancel
        )
        .unwrap(),
        AssignmentStatus::Cancelled
    );
}

#[test]
fn waiting_can_resume_or_cancel() {
    assert_eq!(
        transition(AssignmentStatus::Waiting, AssignmentAction::Resume).unwrap(),
        AssignmentStatus::Queued
    );
    assert_eq!(
        transition(AssignmentStatus::Waiting, AssignmentAction::Cancel).unwrap(),
        AssignmentStatus::Cancelled
    );
}

#[test]
fn terminal_and_unplanned_transitions_are_rejected() {
    for terminal in [
        AssignmentStatus::Completed,
        AssignmentStatus::Cancelled,
        AssignmentStatus::DeadLetter,
    ] {
        assert!(transition(terminal, AssignmentAction::Retry).is_err());
    }
    assert!(transition(AssignmentStatus::Queued, AssignmentAction::Cancel).is_err());
    assert!(transition(AssignmentStatus::Claimed, AssignmentAction::Cancel).is_err());
    assert!(transition(AssignmentStatus::Failed, AssignmentAction::Resume).is_err());
}

#[test]
fn recovery_requeues_safe_or_unstarted_attempts() {
    for side_effect in [
        AssignmentSideEffect::ReadOnly,
        AssignmentSideEffect::IdempotentWrite,
        AssignmentSideEffect::NonIdempotentWrite,
        AssignmentSideEffect::Unknown,
    ] {
        assert_eq!(
            recovery_decision(side_effect, false),
            RecoveryDecision::Requeue
        );
    }
    assert_eq!(
        recovery_decision(AssignmentSideEffect::ReadOnly, true),
        RecoveryDecision::Requeue
    );
    assert_eq!(
        recovery_decision(AssignmentSideEffect::IdempotentWrite, true),
        RecoveryDecision::Requeue
    );
}

#[test]
fn recovery_requires_confirmation_for_uncertain_started_writes() {
    assert_eq!(
        recovery_decision(AssignmentSideEffect::NonIdempotentWrite, true),
        RecoveryDecision::RequireConfirmation
    );
    assert_eq!(
        recovery_decision(AssignmentSideEffect::Unknown, true),
        RecoveryDecision::RequireConfirmation
    );
}

#[test]
fn retry_delay_is_deterministic_bounded_and_fail_safe() {
    let base = Duration::from_secs(10);
    let max = Duration::from_secs(60);
    let first = retry_delay(1, base, max, 42);
    assert_eq!(first, retry_delay(1, base, max, 42));
    assert!(first >= Duration::from_secs(5) && first <= Duration::from_secs(15));
    assert!(retry_delay(4, base, max, 7) <= max);
    assert_eq!(retry_delay(0, base, max, 7), Duration::ZERO);
    assert_eq!(retry_delay(u32::MAX, base, max, 7), max);
    assert_eq!(retry_delay(2, Duration::from_secs(100), max, 7), max);
}

#[test]
fn claimed_event_serializes_null_session_until_a_real_session_exists() {
    let payload = WorkEventPayload::AssignmentClaimed {
        assignment_id: "assignment".into(),
        agent_instance_id: "agent".into(),
        agent_session_id: None,
    };
    assert_eq!(
        serde_json::to_value(payload).unwrap()["agentSessionId"],
        serde_json::Value::Null
    );
}

#[test]
fn cancelled_event_has_a_typed_nullable_runtime_identity() {
    let payload = WorkEventPayload::AssignmentCancelled {
        assignment_id: "assignment".into(),
        agent_instance_id: "agent".into(),
        agent_session_id: None,
        run_id: None,
        reason: "recovery declined".into(),
    };
    let value = serde_json::to_value(payload).unwrap();
    assert_eq!(value["type"], "assignmentCancelled");
    assert_eq!(value["agentSessionId"], serde_json::Value::Null);
    assert_eq!(value["runId"], serde_json::Value::Null);
}

async fn seed_work_member(pool: &sqlx::SqlitePool, work_id: &str, agent_instance_id: &str) {
    let now = Utc.with_ymd_and_hms(2026, 8, 15, 10, 0, 0).unwrap();
    sqlx::query("INSERT INTO work_agents (work_id, agent_instance_id, role_kind, status, permission_policy, joined_at, updated_at) VALUES (?, ?, 'researcher', 'joined', 'inherit_work', ?, ?)")
        .bind(work_id).bind(agent_instance_id).bind(now).bind(now)
        .execute(pool).await.unwrap();
}

#[tokio::test]
async fn session_same_agent_and_work_resume_same_ready_generation() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-session-resume").await;
    let repository = AssignmentRepository::new(database.pool().clone());

    let first = repository
        .claim_or_create_session(
            "agent-instance:piwork-lead",
            "work-session-resume",
            "pi_rpc",
            "owner-1",
        )
        .await
        .unwrap();
    assert_eq!(first.generation, 1);
    assert_eq!(
        first.status,
        piwork_lib::domain::assignment::AgentSessionStatus::Running
    );
    let first_id = first.id.clone();

    repository
        .mark_ready(&first.id, "turn-latest")
        .await
        .unwrap();

    let second = repository
        .claim_or_create_session(
            "agent-instance:piwork-lead",
            "work-session-resume",
            "pi_rpc",
            "owner-1",
        )
        .await
        .unwrap();

    assert_eq!(
        second.id, first_id,
        "ready session must be claimed in place"
    );
    assert_eq!(second.generation, 1, "claim must not bump the generation");
    assert_eq!(
        second.status,
        piwork_lib::domain::assignment::AgentSessionStatus::Running
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM agent_sessions")
            .fetch_one(database.pool())
            .await
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn session_different_agents_never_share_session_or_directory() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-session-agents").await;
    seed_work_member(
        database.pool(),
        "work-session-agents",
        "agent-instance:piwork-researcher",
    )
    .await;
    let repository = AssignmentRepository::new(database.pool().clone());

    let lead = repository
        .claim_or_create_session(
            "agent-instance:piwork-lead",
            "work-session-agents",
            "pi_rpc",
            "owner-1",
        )
        .await
        .unwrap();
    let researcher = repository
        .claim_or_create_session(
            "agent-instance:piwork-researcher",
            "work-session-agents",
            "pi_rpc",
            "owner-1",
        )
        .await
        .unwrap();

    assert_ne!(lead.id, researcher.id);
    assert_ne!(lead.agent_instance_id, researcher.agent_instance_id);
}

#[tokio::test]
async fn session_same_agent_in_different_works_never_shares_session() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-session-a").await;
    seed_work(database.pool(), "work-session-b").await;
    let repository = AssignmentRepository::new(database.pool().clone());

    let in_a = repository
        .claim_or_create_session(
            "agent-instance:piwork-lead",
            "work-session-a",
            "pi_rpc",
            "owner-1",
        )
        .await
        .unwrap();
    let in_b = repository
        .claim_or_create_session(
            "agent-instance:piwork-lead",
            "work-session-b",
            "pi_rpc",
            "owner-1",
        )
        .await
        .unwrap();

    assert_ne!(in_a.id, in_b.id);
    assert_ne!(in_a.work_id, in_b.work_id);
    assert_eq!(in_a.generation, 1);
    assert_eq!(in_b.generation, 1);
}

#[tokio::test]
async fn session_rotate_invalidates_old_generation_without_changing_identity() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-session-rotate").await;
    let repository = AssignmentRepository::new(database.pool().clone());

    let first = repository
        .claim_or_create_session(
            "agent-instance:piwork-lead",
            "work-session-rotate",
            "pi_rpc",
            "owner-1",
        )
        .await
        .unwrap();
    let rotated = repository.rotate(&first.id, "context limit").await.unwrap();

    assert_ne!(rotated.id, first.id);
    assert_eq!(rotated.generation, first.generation + 1);
    assert_eq!(rotated.agent_instance_id, first.agent_instance_id);
    assert_eq!(rotated.work_id, first.work_id);
    assert_eq!(rotated.engine_kind, first.engine_kind);
    assert_eq!(rotated.rotation_reason, Some("context limit".into()));
    assert_eq!(
        rotated.status,
        piwork_lib::domain::assignment::AgentSessionStatus::Ready
    );

    let row: (String, Option<String>) =
        sqlx::query_as("SELECT status, rotation_reason FROM agent_sessions WHERE id = ?")
            .bind(&first.id)
            .fetch_one(database.pool())
            .await
            .unwrap();
    assert_eq!(row.0, "invalidated");
    assert_eq!(row.1.as_deref(), Some("context limit"));
}

#[tokio::test]
async fn session_process_crash_invalidates_all_owned_sessions() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-session-crash").await;
    seed_work_member(
        database.pool(),
        "work-session-crash",
        "agent-instance:piwork-researcher",
    )
    .await;
    let repository = AssignmentRepository::new(database.pool().clone());

    repository
        .claim_or_create_session(
            "agent-instance:piwork-lead",
            "work-session-crash",
            "pi_rpc",
            "owner-crash",
        )
        .await
        .unwrap();
    repository
        .claim_or_create_session(
            "agent-instance:piwork-researcher",
            "work-session-crash",
            "pi_rpc",
            "owner-crash",
        )
        .await
        .unwrap();
    repository
        .claim_or_create_session(
            "agent-instance:piwork-researcher",
            "work-session-crash",
            "codex",
            "owner-other",
        )
        .await
        .unwrap();

    let invalidated = repository
        .invalidate_owner("owner-crash", "process crashed")
        .await
        .unwrap();

    assert_eq!(invalidated.len(), 2);
    assert!(invalidated.iter().all(|session| {
        session.status == piwork_lib::domain::assignment::AgentSessionStatus::Invalidated
    }));
    let still_owned: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_sessions WHERE runtime_owner_id = 'owner-crash'",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(still_owned, 0);
    let untouched: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_sessions WHERE runtime_owner_id = 'owner-other'",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(untouched, 1);
}

#[tokio::test]
async fn session_attach_engine_reference_stores_only_opaque_ref() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-session-ref").await;
    let repository = AssignmentRepository::new(database.pool().clone());

    let session = repository
        .claim_or_create_session(
            "agent-instance:piwork-lead",
            "work-session-ref",
            "pi_rpc",
            "owner-1",
        )
        .await
        .unwrap();

    repository
        .attach_engine_reference(&session.id, "opaque-pi-session-7f06")
        .await
        .unwrap();

    let stored: Option<String> =
        sqlx::query_scalar("SELECT engine_reference FROM agent_sessions WHERE id = ?")
            .bind(&session.id)
            .fetch_one(database.pool())
            .await
            .unwrap();
    assert_eq!(stored.as_deref(), Some("opaque-pi-session-7f06"));
}

#[tokio::test]
async fn session_rejects_non_member_agent_and_missing_identity() {
    let database = Database::open_in_memory().await.unwrap();
    seed_work(database.pool(), "work-session-member").await;
    let repository = AssignmentRepository::new(database.pool().clone());

    let error = repository
        .claim_or_create_session(
            "agent-instance:local:stranger",
            "work-session-member",
            "pi_rpc",
            "owner-1",
        )
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        AppError::Database(_) | AppError::InvalidInput { .. }
    ));

    let error = repository
        .claim_or_create_session(" ", "work-session-member", "pi_rpc", "owner-1")
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::InvalidInput { .. }));
}
