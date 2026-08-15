use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use chrono::{TimeZone, Utc};
use piwork_lib::{
    assignment::repository::{AcceptAssignmentInput, AssignmentEventSink, AssignmentRepository},
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
    assert_eq!(sink.0.lock().unwrap().as_slice(), events.as_slice());
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
    assert!(sink.0.lock().unwrap().is_empty());
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
