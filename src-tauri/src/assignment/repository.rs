use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::{
    domain::{
        assignment::{AssignmentKind, AssignmentSideEffect, AssignmentStatus, AssignmentSummary},
        event::{WorkEventEnvelope, WorkEventPayload},
        work::{RunStatus, RunSummary},
    },
    error::AppError,
};

const MAX_ID_BYTES: usize = 255;
const MAX_LABEL_BYTES: usize = 255;
const MAX_TEXT_BYTES: usize = 64 * 1024;
const MAX_JSON_BYTES: usize = 1024 * 1024;
const MAX_OUTBOX_ERROR_BYTES: usize = 512;
const ASSIGNMENT_EVENT_VERSION: u32 = 2;
const OUTBOX_BATCH_SIZE: i64 = 64;
const OUTBOX_LEASE_DURATION: chrono::Duration = chrono::Duration::seconds(30);
const OUTBOX_CLAIM_POLL_INTERVAL: Duration = Duration::from_millis(10);
const RECOVERY_RETRY_BASE: Duration = Duration::from_secs(1);
const RECOVERY_RETRY_MAX: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub struct AcceptAssignmentInput {
    pub id: Option<String>,
    pub work_id: String,
    pub parent_assignment_id: Option<String>,
    pub created_by_agent_id: Option<String>,
    pub assigned_agent_id: String,
    pub capability_pack_id: Option<String>,
    pub kind: AssignmentKind,
    pub side_effect: AssignmentSideEffect,
    pub title: String,
    pub instruction: String,
    pub context_manifest: Value,
    pub expected_result_schema: Value,
    pub acceptance_criteria: Value,
    pub permission_scope: Value,
    pub priority: u32,
    pub max_attempts: u32,
    pub not_before: Option<DateTime<Utc>>,
}

/// Receives an at-least-once stream. Implementations must deduplicate the stable `event_id` before
/// applying externally visible side effects because a crash can occur after publish but before ack.
pub trait AssignmentEventSink: Send + Sync {
    fn publish(&self, event: WorkEventEnvelope) -> Result<(), AppError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryReport {
    pub requeued: Vec<String>,
    pub confirmation_required: Vec<String>,
    pub dead_lettered: Vec<String>,
    pub untouched: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxDrainReport {
    pub attempted: usize,
    pub published: usize,
    pub failed_event_ids: Vec<String>,
}

#[derive(Debug, Clone, FromRow, PartialEq, Eq)]
pub struct PendingEventDelivery {
    pub ordinal: i64,
    pub event_id: String,
    pub assignment_id: String,
    pub status: String,
    pub attempt_count: i64,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub lease_expires_at: Option<DateTime<Utc>>,
}

struct UnavailableEventSink;

impl AssignmentEventSink for UnavailableEventSink {
    fn publish(&self, _event: WorkEventEnvelope) -> Result<(), AppError> {
        Err(AppError::event_publish(
            "assignment event sink is unavailable",
        ))
    }
}

#[derive(Clone)]
pub struct AssignmentRepository {
    pool: SqlitePool,
    event_sink: Arc<dyn AssignmentEventSink>,
}

struct OutboxClaimGuard {
    pool: SqlitePool,
    lease_token: Option<String>,
}

impl OutboxClaimGuard {
    fn new(pool: SqlitePool, lease_token: String) -> Self {
        Self {
            pool,
            lease_token: Some(lease_token),
        }
    }

    fn disarm(&mut self) {
        self.lease_token = None;
    }

    fn lease_token(&self) -> &str {
        self.lease_token
            .as_deref()
            .expect("armed outbox claim guards have a lease token")
    }
}

impl Drop for OutboxClaimGuard {
    fn drop(&mut self) {
        let Some(lease_token) = self.lease_token.take() else {
            return;
        };
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let pool = self.pool.clone();
        drop(runtime.spawn(async move {
            let now = Utc::now();
            let _ = sqlx::query("UPDATE assignment_event_outbox SET status = 'pending', lease_token = NULL, lease_expires_at = NULL, last_error = 'delivery interrupted before acknowledgement; details redacted', updated_at = ? WHERE status = 'delivering' AND lease_token = ?")
                .bind(now)
                .bind(lease_token)
                .execute(&pool)
                .await;
        }));
    }
}

impl AssignmentRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self::with_event_sink(pool, Arc::new(UnavailableEventSink))
    }

    pub fn with_event_sink(pool: SqlitePool, event_sink: Arc<dyn AssignmentEventSink>) -> Self {
        Self { pool, event_sink }
    }

    /// Constructs a repository for an exclusive startup lifecycle and recovers deliveries claimed
    /// by the previous process. The frontend-ready command drains pending events only after its
    /// live listener is registered, so a successful Tauri emit cannot be acknowledged too early.
    pub async fn initialize_with_event_sink(
        pool: SqlitePool,
        event_sink: Arc<dyn AssignmentEventSink>,
    ) -> Result<Self, AppError> {
        let repository = Self::with_event_sink(pool, event_sink);
        repository.recover_startup_deliveries().await?;
        Ok(repository)
    }

    pub async fn accept(
        &self,
        input: AcceptAssignmentInput,
    ) -> Result<AssignmentSummary, AppError> {
        let id = input
            .id
            .as_deref()
            .map(|value| validate_id("id", value))
            .transpose()?
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let work_id = validate_id("workId", &input.work_id)?;
        let parent_assignment_id =
            validate_optional_id("parentAssignmentId", input.parent_assignment_id.as_deref())?;
        let created_by_agent_id =
            validate_optional_id("createdByAgentId", input.created_by_agent_id.as_deref())?;
        let assigned_agent_id = validate_id("assignedAgentId", &input.assigned_agent_id)?;
        let capability_pack_id =
            validate_optional_id("capabilityPackId", input.capability_pack_id.as_deref())?;
        let title = validate_text("title", &input.title)?;
        let instruction = validate_text("instruction", &input.instruction)?;
        if input.max_attempts == 0 {
            return Err(AppError::invalid_input(
                "maxAttempts",
                "maxAttempts must be at least 1",
            ));
        }
        let context_manifest = validate_json("contextManifest", &input.context_manifest, "object")?;
        let expected_result_schema = validate_json(
            "expectedResultSchema",
            &input.expected_result_schema,
            "object",
        )?;
        let acceptance_criteria =
            validate_json("acceptanceCriteria", &input.acceptance_criteria, "array")?;
        let permission_scope = validate_json("permissionScope", &input.permission_scope, "object")?;

        let now = Utc::now();
        let assignment = AssignmentSummary {
            id,
            work_id,
            parent_assignment_id,
            created_by_agent_id,
            assigned_agent_id,
            capability_pack_id,
            kind: input.kind,
            side_effect: input.side_effect,
            title,
            instruction,
            context_manifest: input.context_manifest,
            expected_result_schema: input.expected_result_schema,
            acceptance_criteria: input.acceptance_criteria,
            permission_scope: input.permission_scope,
            priority: input.priority,
            status: AssignmentStatus::Queued,
            attempt_count: 0,
            max_attempts: input.max_attempts,
            not_before: input.not_before,
            result_summary: None,
            last_error: None,
            next_attempt_at: None,
            recovery_reason: None,
            created_at: now,
            claimed_at: None,
            started_at: None,
            completed_at: None,
            updated_at: now,
        };
        let event = WorkEventEnvelope {
            version: ASSIGNMENT_EVENT_VERSION,
            event_id: Some(Uuid::new_v4().to_string()),
            work_id: assignment.work_id.clone(),
            run_id: None,
            turn_id: None,
            session_id: None,
            agent_id: Some(assignment.assigned_agent_id.clone()),
            assignment_id: Some(assignment.id.clone()),
            causation_id: None,
            correlation_id: None,
            sequence: 1,
            occurred_at: now,
            payload: WorkEventPayload::AssignmentQueued {
                assignment_id: assignment.id.clone(),
                assigned_agent_id: assignment.assigned_agent_id.clone(),
                title: assignment.title.clone(),
                priority: assignment.priority,
            },
        };

        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query(
            "INSERT INTO assignments (id, work_id, parent_assignment_id, created_by_agent_id, assigned_agent_id, capability_pack_id, kind, side_effect, title, instruction, context_manifest_json, expected_result_schema_json, acceptance_criteria_json, permission_scope_json, priority, status, attempt_count, max_attempts, not_before, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'queued', 0, ?, ?, ?, ?)",
        )
        .bind(&assignment.id)
        .bind(&assignment.work_id)
        .bind(&assignment.parent_assignment_id)
        .bind(&assignment.created_by_agent_id)
        .bind(&assignment.assigned_agent_id)
        .bind(&assignment.capability_pack_id)
        .bind(assignment.kind)
        .bind(assignment.side_effect)
        .bind(&assignment.title)
        .bind(&assignment.instruction)
        .bind(context_manifest)
        .bind(expected_result_schema)
        .bind(acceptance_criteria)
        .bind(permission_scope)
        .bind(i64::from(assignment.priority))
        .bind(i64::from(assignment.max_attempts))
        .bind(assignment.not_before)
        .bind(assignment.created_at)
        .bind(assignment.updated_at)
        .execute(&mut *transaction)
        .await?;
        insert_event(&mut transaction, &event).await?;
        transaction.commit().await?;
        self.drain_after_commit().await;
        Ok(assignment)
    }

    pub async fn list_for_work(&self, work_id: &str) -> Result<Vec<AssignmentSummary>, AppError> {
        let work_id = validate_id("workId", work_id)?;
        load_assignments(
            sqlx::query_as::<_, AssignmentRow>(&format!(
                "{} WHERE work_id = ? ORDER BY created_at, id",
                ASSIGNMENT_SELECT
            ))
            .bind(work_id)
            .fetch_all(&self.pool)
            .await?,
        )
    }

    pub async fn load_schedulable(
        &self,
        now: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<AssignmentSummary>, AppError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        load_assignments(
            sqlx::query_as::<_, AssignmentRow>(&format!(
                "{} WHERE status = 'queued' AND attempt_count < max_attempts \
                 AND (not_before IS NULL OR not_before <= ?) \
                 AND (next_attempt_at IS NULL OR next_attempt_at <= ?) \
                 AND NOT EXISTS (SELECT 1 FROM assignment_dependencies dependencies \
                    INNER JOIN assignments predecessors ON predecessors.id = dependencies.depends_on_assignment_id \
                    WHERE dependencies.assignment_id = assignments.id \
                      AND predecessors.status NOT IN ('completed', 'cancelled', 'dead_letter')) \
                 ORDER BY priority DESC, created_at, id LIMIT ?",
                ASSIGNMENT_SELECT
            ))
            .bind(now)
            .bind(now)
            .bind(i64::from(limit))
            .fetch_all(&self.pool)
            .await?,
        )
    }

    pub async fn claim(
        &self,
        assignment_id: &str,
        runtime_owner: &str,
        now: DateTime<Utc>,
    ) -> Result<AssignmentSummary, AppError> {
        let assignment_id = validate_id("assignmentId", assignment_id)?;
        let runtime_owner = validate_label("runtimeOwner", runtime_owner)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let mut assignment = load_assignment(&mut transaction, &assignment_id).await?;
        if assignment.status != AssignmentStatus::Queued
            || assignment.attempt_count >= assignment.max_attempts
            || assignment.not_before.is_some_and(|due| due > now)
            || assignment.next_attempt_at.is_some_and(|due| due > now)
            || !dependencies_terminal_in(&mut transaction, &assignment_id).await?
        {
            return Err(invalid_assignment_state("assignment is not schedulable"));
        }
        sqlx::query("UPDATE assignments SET status = 'claimed', runtime_owner_id = ?, claimed_at = ?, updated_at = ? WHERE id = ? AND status = 'queued'")
            .bind(&runtime_owner).bind(now).bind(now).bind(&assignment_id)
            .execute(&mut *transaction).await?;
        assignment.status = AssignmentStatus::Claimed;
        assignment.claimed_at = Some(now);
        assignment.updated_at = now;
        assignment_event(
            &mut transaction,
            &assignment,
            None,
            None,
            now,
            WorkEventPayload::AssignmentClaimed {
                assignment_id: assignment.id.clone(),
                agent_instance_id: assignment.assigned_agent_id.clone(),
                agent_session_id: None,
            },
        )
        .await?;
        transaction.commit().await?;
        self.drain_after_commit().await;
        Ok(assignment.summary)
    }

    pub async fn begin_attempt(
        &self,
        assignment_id: &str,
        engine_kind: &str,
        model_label: &str,
    ) -> Result<RunSummary, AppError> {
        let assignment_id = validate_id("assignmentId", assignment_id)?;
        let engine_kind = validate_label("engineKind", engine_kind)?;
        let model_label = validate_label("modelLabel", model_label)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let assignment = load_assignment(&mut transaction, &assignment_id).await?;
        if assignment.status != AssignmentStatus::Claimed
            || assignment.attempt_count >= assignment.max_attempts
        {
            return Err(invalid_assignment_state(
                "assignment cannot begin an attempt",
            ));
        }
        let active_attempts: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM runs WHERE assignment_id = ? AND status IN ('queued', 'running', 'waiting')",
        )
        .bind(&assignment_id)
        .fetch_one(&mut *transaction)
        .await?;
        if active_attempts != 0 {
            return Err(invalid_assignment_state(
                "assignment already has an active attempt",
            ));
        }
        let now = Utc::now();
        let attempt_number = assignment.attempt_count + 1;
        let run = RunSummary {
            id: Uuid::new_v4().to_string(),
            work_id: assignment.work_id.clone(),
            assignment_id: Some(assignment.id.clone()),
            agent_instance_id: Some(assignment.assigned_agent_id.clone()),
            engine_kind,
            engine_session_id: None,
            model_label,
            status: RunStatus::Queued,
            created_at: now,
            started_at: None,
            completed_at: None,
        };
        sqlx::query("UPDATE assignments SET attempt_count = ?, updated_at = ? WHERE id = ? AND status = 'claimed'")
            .bind(i64::from(attempt_number)).bind(now).bind(&assignment_id)
            .execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO runs (id, work_id, engine_kind, model_label, status, created_at, updated_at, assignment_id, agent_instance_id, attempt_number) VALUES (?, ?, ?, ?, 'queued', ?, ?, ?, ?, ?)")
            .bind(&run.id).bind(&run.work_id).bind(&run.engine_kind).bind(&run.model_label)
            .bind(now).bind(now).bind(&assignment_id).bind(&assignment.assigned_agent_id)
            .bind(i64::from(attempt_number)).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(run)
    }

    pub async fn mark_running(
        &self,
        assignment_id: &str,
        run_id: &str,
        session_id: &str,
        runtime_owner: &str,
        now: DateTime<Utc>,
    ) -> Result<AssignmentSummary, AppError> {
        let assignment_id = validate_id("assignmentId", assignment_id)?;
        let run_id = validate_id("runId", run_id)?;
        let session_id = validate_label("sessionId", session_id)?;
        let runtime_owner = validate_label("runtimeOwner", runtime_owner)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let mut assignment = load_assignment(&mut transaction, &assignment_id).await?;
        let run = load_run(&mut transaction, &run_id).await?;
        validate_active_identity(&assignment, &run, &runtime_owner, None)?;
        if assignment.status != AssignmentStatus::Claimed || run.status != RunStatus::Queued {
            return Err(invalid_assignment_state(
                "assignment attempt cannot start running",
            ));
        }
        sqlx::query("UPDATE assignments SET status = 'running', started_at = ?, updated_at = ? WHERE id = ?")
            .bind(now).bind(now).bind(&assignment_id).execute(&mut *transaction).await?;
        sqlx::query("UPDATE runs SET status = 'running', engine_session_id = ?, started_at = ?, updated_at = ? WHERE id = ?")
            .bind(&session_id).bind(now).bind(now).bind(&run_id).execute(&mut *transaction).await?;
        assignment.status = AssignmentStatus::Running;
        assignment.started_at = Some(now);
        assignment.updated_at = now;
        assignment_event(
            &mut transaction,
            &assignment,
            Some(&run_id),
            Some(session_id.clone()),
            now,
            WorkEventPayload::AssignmentStarted {
                assignment_id: assignment.id.clone(),
                agent_instance_id: assignment.assigned_agent_id.clone(),
                agent_session_id: session_id,
                run_id: run_id.clone(),
            },
        )
        .await?;
        transaction.commit().await?;
        self.drain_after_commit().await;
        Ok(assignment.summary)
    }

    pub async fn mark_waiting(
        &self,
        assignment_id: &str,
        run_id: &str,
        session_id: &str,
        runtime_owner: &str,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<AssignmentSummary, AppError> {
        let assignment_id = validate_id("assignmentId", assignment_id)?;
        let run_id = validate_id("runId", run_id)?;
        let session_id = validate_label("sessionId", session_id)?;
        let runtime_owner = validate_label("runtimeOwner", runtime_owner)?;
        let reason = validate_text("reason", reason)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let mut assignment = load_assignment(&mut transaction, &assignment_id).await?;
        let run = load_run(&mut transaction, &run_id).await?;
        validate_active_identity(&assignment, &run, &runtime_owner, Some(&session_id))?;
        if assignment.status != AssignmentStatus::Running || run.status != RunStatus::Running {
            return Err(invalid_assignment_state("assignment is not running"));
        }
        sqlx::query("UPDATE assignments SET status = 'waiting', updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(&assignment_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("UPDATE runs SET status = 'waiting', updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(&run_id)
            .execute(&mut *transaction)
            .await?;
        assignment.status = AssignmentStatus::Waiting;
        assignment.updated_at = now;
        assignment_event(
            &mut transaction,
            &assignment,
            Some(&run_id),
            Some(session_id.clone()),
            now,
            WorkEventPayload::AssignmentWaiting {
                assignment_id: assignment.id.clone(),
                agent_instance_id: assignment.assigned_agent_id.clone(),
                agent_session_id: session_id,
                reason,
            },
        )
        .await?;
        transaction.commit().await?;
        self.drain_after_commit().await;
        Ok(assignment.summary)
    }

    pub async fn complete(
        &self,
        assignment_id: &str,
        run_id: &str,
        session_id: &str,
        runtime_owner: &str,
        result_summary: &str,
        now: DateTime<Utc>,
    ) -> Result<AssignmentSummary, AppError> {
        let assignment_id = validate_id("assignmentId", assignment_id)?;
        let run_id = validate_id("runId", run_id)?;
        let session_id = validate_label("sessionId", session_id)?;
        let runtime_owner = validate_label("runtimeOwner", runtime_owner)?;
        let result_summary = validate_text("resultSummary", result_summary)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let mut assignment = load_assignment(&mut transaction, &assignment_id).await?;
        let run = load_run(&mut transaction, &run_id).await?;
        validate_active_identity(&assignment, &run, &runtime_owner, Some(&session_id))?;
        if assignment.status == AssignmentStatus::Completed && run.status == RunStatus::Completed {
            if assignment.result_summary.as_deref() == Some(result_summary.as_str()) {
                transaction.commit().await?;
                return Ok(assignment.summary);
            }
            return Err(invalid_assignment_state(
                "terminal completion does not match",
            ));
        }
        if assignment.status != AssignmentStatus::Running || run.status != RunStatus::Running {
            return Err(invalid_assignment_state("assignment is not running"));
        }
        sqlx::query("UPDATE assignments SET status = 'completed', result_summary = ?, completed_at = ?, updated_at = ? WHERE id = ?")
            .bind(&result_summary).bind(now).bind(now).bind(&assignment_id).execute(&mut *transaction).await?;
        sqlx::query(
            "UPDATE runs SET status = 'completed', completed_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(now)
        .bind(now)
        .bind(&run_id)
        .execute(&mut *transaction)
        .await?;
        assignment.status = AssignmentStatus::Completed;
        assignment.result_summary = Some(result_summary.clone());
        assignment.completed_at = Some(now);
        assignment.updated_at = now;
        assignment_event(
            &mut transaction,
            &assignment,
            Some(&run_id),
            Some(session_id.clone()),
            now,
            WorkEventPayload::AssignmentCompleted {
                assignment_id: assignment.id.clone(),
                agent_instance_id: assignment.assigned_agent_id.clone(),
                agent_session_id: session_id,
                result_summary,
            },
        )
        .await?;
        transaction.commit().await?;
        self.drain_after_commit().await;
        Ok(assignment.summary)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn fail_and_schedule_retry(
        &self,
        assignment_id: &str,
        run_id: &str,
        session_id: &str,
        runtime_owner: &str,
        error: &str,
        now: DateTime<Utc>,
        base: Duration,
        max: Duration,
    ) -> Result<AssignmentSummary, AppError> {
        let assignment_id = validate_id("assignmentId", assignment_id)?;
        let run_id = validate_id("runId", run_id)?;
        let session_id = validate_label("sessionId", session_id)?;
        let runtime_owner = validate_label("runtimeOwner", runtime_owner)?;
        let error = validate_text("error", error)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let mut assignment = load_assignment(&mut transaction, &assignment_id).await?;
        let run = load_run(&mut transaction, &run_id).await?;
        validate_active_identity(&assignment, &run, &runtime_owner, Some(&session_id))?;
        if assignment.status != AssignmentStatus::Running || run.status != RunStatus::Running {
            return Err(invalid_assignment_state("assignment is not running"));
        }
        if assignment.attempt_count >= assignment.max_attempts {
            return Err(invalid_assignment_state(
                "assignment attempt limit is exhausted",
            ));
        }
        let delay = super::state_machine::retry_delay(
            assignment.attempt_count,
            base,
            max,
            stable_seed(&assignment.id),
        );
        let chrono_delay = chrono::Duration::from_std(delay).unwrap_or(chrono::Duration::MAX);
        let next_attempt_at = now
            .checked_add_signed(chrono_delay)
            .unwrap_or(DateTime::<Utc>::MAX_UTC);
        if assignment
            .next_attempt_at
            .is_some_and(|previous| next_attempt_at < previous)
        {
            return Err(invalid_assignment_state(
                "next attempt time must be monotonic",
            ));
        }
        sqlx::query(
            "UPDATE runs SET status = 'failed', completed_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(now)
        .bind(now)
        .bind(&run_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("UPDATE assignments SET status = 'queued', runtime_owner_id = NULL, claimed_at = NULL, started_at = NULL, completed_at = NULL, last_error = ?, next_attempt_at = ?, updated_at = ? WHERE id = ?")
            .bind(&error).bind(next_attempt_at).bind(now).bind(&assignment_id).execute(&mut *transaction).await?;
        assignment.status = AssignmentStatus::Queued;
        assignment.claimed_at = None;
        assignment.started_at = None;
        assignment.last_error = Some(error.clone());
        assignment.next_attempt_at = Some(next_attempt_at);
        assignment.updated_at = now;
        assignment_event(
            &mut transaction,
            &assignment,
            Some(&run_id),
            Some(session_id.clone()),
            now,
            WorkEventPayload::AssignmentFailed {
                assignment_id: assignment.id.clone(),
                agent_instance_id: assignment.assigned_agent_id.clone(),
                agent_session_id: session_id.clone(),
                error: error.clone(),
            },
        )
        .await?;
        assignment_event(
            &mut transaction,
            &assignment,
            Some(&run_id),
            Some(session_id.clone()),
            now,
            WorkEventPayload::AssignmentRetryScheduled {
                assignment_id: assignment.id.clone(),
                agent_instance_id: assignment.assigned_agent_id.clone(),
                attempt_count: assignment.attempt_count,
                next_attempt_at,
                reason: error,
            },
        )
        .await?;
        transaction.commit().await?;
        self.drain_after_commit().await;
        Ok(assignment.summary)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn dead_letter(
        &self,
        assignment_id: &str,
        run_id: &str,
        session_id: &str,
        runtime_owner: &str,
        error: &str,
        now: DateTime<Utc>,
    ) -> Result<AssignmentSummary, AppError> {
        let assignment_id = validate_id("assignmentId", assignment_id)?;
        let run_id = validate_id("runId", run_id)?;
        let session_id = validate_label("sessionId", session_id)?;
        let runtime_owner = validate_label("runtimeOwner", runtime_owner)?;
        let error = validate_text("error", error)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let mut assignment = load_assignment(&mut transaction, &assignment_id).await?;
        let run = load_run(&mut transaction, &run_id).await?;
        validate_active_identity(&assignment, &run, &runtime_owner, Some(&session_id))?;
        if assignment.status != AssignmentStatus::Running || run.status != RunStatus::Running {
            return Err(invalid_assignment_state("assignment is not running"));
        }
        sqlx::query(
            "UPDATE runs SET status = 'failed', completed_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(now)
        .bind(now)
        .bind(&run_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("UPDATE assignments SET status = 'dead_letter', last_error = ?, completed_at = ?, updated_at = ? WHERE id = ?")
            .bind(&error).bind(now).bind(now).bind(&assignment_id).execute(&mut *transaction).await?;
        assignment.status = AssignmentStatus::DeadLetter;
        assignment.last_error = Some(error.clone());
        assignment.completed_at = Some(now);
        assignment.updated_at = now;
        assignment_event(
            &mut transaction,
            &assignment,
            Some(&run_id),
            Some(session_id.clone()),
            now,
            WorkEventPayload::AssignmentFailed {
                assignment_id: assignment.id.clone(),
                agent_instance_id: assignment.assigned_agent_id.clone(),
                agent_session_id: session_id.clone(),
                error: error.clone(),
            },
        )
        .await?;
        assignment_event(
            &mut transaction,
            &assignment,
            Some(&run_id),
            Some(session_id.clone()),
            now,
            WorkEventPayload::AssignmentDeadLettered {
                assignment_id: assignment.id.clone(),
                agent_instance_id: assignment.assigned_agent_id.clone(),
                attempt_count: assignment.attempt_count,
                error,
            },
        )
        .await?;
        transaction.commit().await?;
        self.drain_after_commit().await;
        Ok(assignment.summary)
    }

    pub async fn recover_orphans(
        &self,
        active_owner_ids: &[String],
    ) -> Result<RecoveryReport, AppError> {
        let active = active_owner_ids
            .iter()
            .map(|owner| validate_label("activeOwnerId", owner))
            .collect::<Result<std::collections::HashSet<_>, _>>()?;
        let now = Utc::now();
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let rows = sqlx::query_as::<_, AssignmentRow>(&format!(
            "{} WHERE status IN ('claimed', 'running') ORDER BY created_at, id",
            ASSIGNMENT_SELECT
        ))
        .fetch_all(&mut *transaction)
        .await?;
        let mut report = RecoveryReport {
            requeued: Vec::new(),
            confirmation_required: Vec::new(),
            dead_lettered: Vec::new(),
            untouched: Vec::new(),
        };
        for row in rows {
            let mut assignment = AssignmentRecord::try_from(row)?;
            if assignment
                .runtime_owner_id
                .as_deref()
                .is_some_and(|owner| active.contains(owner))
            {
                report.untouched.push(assignment.id.clone());
                continue;
            }
            let reason = if assignment.status == AssignmentStatus::Claimed {
                "runtime owner disappeared before the attempt started"
            } else {
                "runtime owner disappeared during the active attempt"
            }
            .to_owned();

            let active_run = sqlx::query_as::<_, RunRow>("SELECT id, work_id, assignment_id, agent_instance_id, engine_session_id, status, attempt_number FROM runs WHERE assignment_id = ? AND status IN ('queued', 'running', 'waiting') ORDER BY attempt_number DESC LIMIT 1")
                .bind(&assignment.id).fetch_optional(&mut *transaction).await?;

            if assignment.status == AssignmentStatus::Claimed && active_run.is_none() {
                sqlx::query("UPDATE assignments SET status = 'queued', runtime_owner_id = NULL, claimed_at = NULL, recovery_reason = ?, updated_at = ? WHERE id = ?")
                    .bind(&reason).bind(now).bind(&assignment.id).execute(&mut *transaction).await?;
                assignment.status = AssignmentStatus::Queued;
                assignment.claimed_at = None;
                assignment.recovery_reason = Some(reason.clone());
                assignment.updated_at = now;
                assignment_event(
                    &mut transaction,
                    &assignment,
                    None,
                    None,
                    now,
                    WorkEventPayload::AssignmentInterrupted {
                        assignment_id: assignment.id.clone(),
                        agent_instance_id: assignment.assigned_agent_id.clone(),
                        agent_session_id: None,
                        reason,
                    },
                )
                .await?;
                assignment_event(
                    &mut transaction,
                    &assignment,
                    None,
                    None,
                    now,
                    WorkEventPayload::AssignmentQueued {
                        assignment_id: assignment.id.clone(),
                        assigned_agent_id: assignment.assigned_agent_id.clone(),
                        title: assignment.title.clone(),
                        priority: assignment.priority,
                    },
                )
                .await?;
                report.requeued.push(assignment.id.clone());
                continue;
            }

            let run = active_run
                .ok_or_else(|| invalid_assignment_state("active assignment has no active Run"))?;
            sqlx::query("UPDATE runs SET status = 'interrupted', completed_at = ?, updated_at = ? WHERE id = ?")
                .bind(now).bind(now).bind(&run.id).execute(&mut *transaction).await?;
            let decision = super::state_machine::recovery_decision(
                assignment.side_effect,
                assignment.started_at.is_some(),
            );
            let final_status = match decision {
                super::state_machine::RecoveryDecision::Requeue
                    if assignment.attempt_count >= assignment.max_attempts =>
                {
                    AssignmentStatus::DeadLetter
                }
                super::state_machine::RecoveryDecision::Requeue => AssignmentStatus::Queued,
                super::state_machine::RecoveryDecision::RequireConfirmation => {
                    AssignmentStatus::RecoveryConfirmationRequired
                }
            };
            let status = match final_status {
                AssignmentStatus::Queued => "queued",
                AssignmentStatus::RecoveryConfirmationRequired => "recovery_confirmation_required",
                AssignmentStatus::DeadLetter => "dead_letter",
                _ => unreachable!(),
            };
            let terminal_at = (final_status == AssignmentStatus::DeadLetter).then_some(now);
            let terminal_error =
                (final_status == AssignmentStatus::DeadLetter).then_some(reason.clone());
            let next_attempt_at = if final_status == AssignmentStatus::Queued {
                let delay = super::state_machine::retry_delay(
                    assignment.attempt_count,
                    RECOVERY_RETRY_BASE,
                    RECOVERY_RETRY_MAX,
                    stable_seed(&assignment.id),
                );
                Some(
                    now.checked_add_signed(
                        chrono::Duration::from_std(delay).unwrap_or(chrono::Duration::MAX),
                    )
                    .unwrap_or(DateTime::<Utc>::MAX_UTC),
                )
            } else {
                None
            };
            sqlx::query("UPDATE assignments SET status = ?, runtime_owner_id = NULL, claimed_at = NULL, started_at = NULL, completed_at = ?, last_error = ?, next_attempt_at = ?, recovery_reason = ?, updated_at = ? WHERE id = ?")
                .bind(status).bind(terminal_at).bind(&terminal_error).bind(next_attempt_at).bind(&reason).bind(now).bind(&assignment.id).execute(&mut *transaction).await?;
            assignment.status = final_status;
            assignment.claimed_at = None;
            assignment.started_at = None;
            assignment.completed_at = terminal_at;
            assignment.last_error = terminal_error;
            assignment.next_attempt_at = next_attempt_at;
            assignment.recovery_reason = Some(reason.clone());
            assignment.updated_at = now;
            assignment_event(
                &mut transaction,
                &assignment,
                Some(&run.id),
                run.engine_session_id.clone(),
                now,
                WorkEventPayload::AssignmentInterrupted {
                    assignment_id: assignment.id.clone(),
                    agent_instance_id: assignment.assigned_agent_id.clone(),
                    agent_session_id: run.engine_session_id.clone(),
                    reason: reason.clone(),
                },
            )
            .await?;
            match decision {
                super::state_machine::RecoveryDecision::Requeue
                    if final_status == AssignmentStatus::DeadLetter =>
                {
                    assignment_event(
                        &mut transaction,
                        &assignment,
                        Some(&run.id),
                        run.engine_session_id.clone(),
                        now,
                        WorkEventPayload::AssignmentDeadLettered {
                            assignment_id: assignment.id.clone(),
                            agent_instance_id: assignment.assigned_agent_id.clone(),
                            attempt_count: assignment.attempt_count,
                            error: reason,
                        },
                    )
                    .await?;
                    report.dead_lettered.push(assignment.id.clone());
                }
                super::state_machine::RecoveryDecision::Requeue => {
                    assignment_event(
                        &mut transaction,
                        &assignment,
                        Some(&run.id),
                        run.engine_session_id.clone(),
                        now,
                        WorkEventPayload::AssignmentRetryScheduled {
                            assignment_id: assignment.id.clone(),
                            agent_instance_id: assignment.assigned_agent_id.clone(),
                            attempt_count: assignment.attempt_count,
                            next_attempt_at: next_attempt_at
                                .expect("queued recovery has a next attempt"),
                            reason: reason.clone(),
                        },
                    )
                    .await?;
                    report.requeued.push(assignment.id.clone())
                }
                super::state_machine::RecoveryDecision::RequireConfirmation => {
                    assignment_event(
                        &mut transaction,
                        &assignment,
                        Some(&run.id),
                        run.engine_session_id.clone(),
                        now,
                        WorkEventPayload::AssignmentRecoveryRequired {
                            assignment_id: assignment.id.clone(),
                            agent_instance_id: assignment.assigned_agent_id.clone(),
                            recovery_reason: reason,
                        },
                    )
                    .await?;
                    report.confirmation_required.push(assignment.id.clone());
                }
            }
        }
        transaction.commit().await?;
        self.drain_after_commit().await;
        Ok(report)
    }

    pub async fn confirm_recovery(
        &self,
        assignment_id: &str,
        resume: bool,
        now: DateTime<Utc>,
    ) -> Result<AssignmentSummary, AppError> {
        let assignment_id = validate_id("assignmentId", assignment_id)?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let mut assignment = load_assignment(&mut transaction, &assignment_id).await?;
        if assignment.status != AssignmentStatus::RecoveryConfirmationRequired {
            return Err(invalid_assignment_state(
                "assignment does not require recovery confirmation",
            ));
        }
        let (status, status_value) = if resume {
            (AssignmentStatus::Queued, "queued")
        } else {
            (AssignmentStatus::Cancelled, "cancelled")
        };
        let max_attempts = if resume && assignment.attempt_count >= assignment.max_attempts {
            assignment.attempt_count.checked_add(1).ok_or_else(|| {
                AppError::invalid_input(
                    "maxAttempts",
                    "assignment attempt limit cannot be extended",
                )
            })?
        } else {
            assignment.max_attempts
        };
        sqlx::query("UPDATE assignments SET status = ?, max_attempts = ?, next_attempt_at = NULL, recovery_reason = NULL, completed_at = ?, updated_at = ? WHERE id = ?")
            .bind(status_value).bind(i64::from(max_attempts)).bind(if resume { None } else { Some(now) }).bind(now).bind(&assignment_id)
            .execute(&mut *transaction).await?;
        assignment.status = status;
        assignment.max_attempts = max_attempts;
        assignment.next_attempt_at = None;
        assignment.recovery_reason = None;
        assignment.completed_at = if resume { None } else { Some(now) };
        assignment.updated_at = now;
        let (event_run_id, event_session_id, payload) = if resume {
            (
                None,
                None,
                WorkEventPayload::AssignmentQueued {
                    assignment_id: assignment.id.clone(),
                    assigned_agent_id: assignment.assigned_agent_id.clone(),
                    title: assignment.title.clone(),
                    priority: assignment.priority,
                },
            )
        } else {
            let run = sqlx::query_as::<_, RunRow>("SELECT id, work_id, assignment_id, agent_instance_id, engine_session_id, status, attempt_number FROM runs WHERE assignment_id = ? ORDER BY attempt_number DESC LIMIT 1")
                .bind(&assignment_id)
                .fetch_optional(&mut *transaction)
                .await?
                .ok_or_else(|| invalid_assignment_state("recovery confirmation has no audited Run"))?;
            let session_id = run.engine_session_id.clone();
            let run_id = run.id;
            (
                Some(run_id.clone()),
                session_id.clone(),
                WorkEventPayload::AssignmentCancelled {
                    assignment_id: assignment.id.clone(),
                    agent_instance_id: assignment.assigned_agent_id.clone(),
                    agent_session_id: session_id,
                    run_id: Some(run_id),
                    reason: "recovery was cancelled by explicit confirmation".into(),
                },
            )
        };
        assignment_event(
            &mut transaction,
            &assignment,
            event_run_id.as_deref(),
            event_session_id,
            now,
            payload,
        )
        .await?;
        transaction.commit().await?;
        self.drain_after_commit().await;
        Ok(assignment.summary)
    }

    pub async fn add_dependency(
        &self,
        assignment_id: &str,
        depends_on_assignment_id: &str,
    ) -> Result<(), AppError> {
        let assignment_id = validate_id("assignmentId", assignment_id)?;
        let depends_on_assignment_id =
            validate_id("dependsOnAssignmentId", depends_on_assignment_id)?;
        if assignment_id == depends_on_assignment_id {
            return Err(AppError::invalid_input(
                "dependency",
                "assignment cannot depend on itself",
            ));
        }
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let work_ids: Vec<String> =
            sqlx::query_scalar("SELECT work_id FROM assignments WHERE id IN (?, ?) ORDER BY id")
                .bind(&assignment_id)
                .bind(&depends_on_assignment_id)
                .fetch_all(&mut *transaction)
                .await?;
        if work_ids.len() != 2 || work_ids[0] != work_ids[1] {
            return Err(AppError::invalid_input(
                "dependency",
                "dependency must belong to the same Work",
            ));
        }
        let creates_cycle: i64 = sqlx::query_scalar(
            "WITH RECURSIVE reachable(id) AS (VALUES (?) UNION SELECT dependencies.depends_on_assignment_id FROM assignment_dependencies dependencies INNER JOIN reachable ON dependencies.assignment_id = reachable.id) SELECT EXISTS(SELECT 1 FROM reachable WHERE id = ?)",
        )
        .bind(&depends_on_assignment_id)
        .bind(&assignment_id)
        .fetch_one(&mut *transaction)
        .await?;
        if creates_cycle != 0 {
            return Err(AppError::invalid_input(
                "dependency",
                "dependency would create a cycle",
            ));
        }
        sqlx::query("INSERT INTO assignment_dependencies (assignment_id, depends_on_assignment_id) VALUES (?, ?)")
            .bind(&assignment_id).bind(&depends_on_assignment_id).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn dependencies_terminal(&self, assignment_id: &str) -> Result<bool, AppError> {
        let assignment_id = validate_id("assignmentId", assignment_id)?;
        let mut connection = self.pool.acquire().await?;
        dependencies_terminal_in(&mut connection, &assignment_id).await
    }

    pub async fn events_for_assignment(
        &self,
        assignment_id: &str,
    ) -> Result<Vec<WorkEventEnvelope>, AppError> {
        let assignment_id = validate_id("assignmentId", assignment_id)?;
        sqlx::query_as::<_, EventRow>(
            "SELECT id, work_id, run_id, turn_id, session_id, agent_id, assignment_id, causation_id, correlation_id, sequence, version, occurred_at, payload FROM events WHERE assignment_id = ? AND run_id IS NULL ORDER BY sequence",
        )
        .bind(&assignment_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(WorkEventEnvelope::try_from)
        .collect()
    }

    pub async fn pending_event_deliveries(&self) -> Result<Vec<PendingEventDelivery>, AppError> {
        Ok(sqlx::query_as::<_, PendingEventDelivery>(
            "SELECT ordinal, event_id, assignment_id, status, attempt_count, last_attempt_at, last_error, lease_expires_at FROM assignment_event_outbox WHERE status <> 'delivered' ORDER BY ordinal",
        )
        .fetch_all(&self.pool)
        .await?)
    }

    /// Drains globally ordered, fixed-size batches of undelivered Assignment events.
    ///
    /// Delivery is at-least-once across the publish/ack crash boundary. Every v2 Assignment event
    /// has a stable `event_id`; sinks and consumers must use it as their idempotency key. A batch
    /// process-owned lease serializes concurrent repository instances without holding a SQLite
    /// lock while the external sink runs. Only the exclusive startup recovery path reclaims a
    /// delivery, so a slow in-flight sink is never stolen after a wall-clock deadline.
    pub async fn drain_pending_events(&self) -> Result<OutboxDrainReport, AppError> {
        let mut report = OutboxDrainReport {
            attempted: 0,
            published: 0,
            failed_event_ids: Vec::new(),
        };
        loop {
            let (mut claim_guard, events) = loop {
                if let Some(claimed) = self.claim_pending_batch().await? {
                    break claimed;
                }
                if self.pending_event_deliveries().await?.is_empty() {
                    return Ok(report);
                }
                tokio::time::sleep(OUTBOX_CLAIM_POLL_INTERVAL).await;
            };
            let lease_token = claim_guard.lease_token().to_owned();
            let mut publish_failed = false;
            for event in events {
                let event_id = event
                    .event_id
                    .as_deref()
                    .expect("outbox events have stable IDs");
                if let Err(error) = self.record_delivery_attempt(event_id, &lease_token).await {
                    let _ = self.release_claimed_batch_after_error(&lease_token).await;
                    return Err(error);
                }
                report.attempted += 1;
                match self.event_sink.publish(event.clone()) {
                    Ok(()) => {
                        if let Err(error) = self.acknowledge_delivery(event_id, &lease_token).await
                        {
                            let _ = self.release_claimed_batch_after_error(&lease_token).await;
                            return Err(error);
                        }
                        report.published += 1;
                    }
                    Err(error) => {
                        if let Err(database_error) = self
                            .record_delivery_failure(event_id, &lease_token, &error)
                            .await
                        {
                            let _ = self.release_claimed_batch_after_error(&lease_token).await;
                            return Err(database_error);
                        }
                        report.failed_event_ids.push(event_id.to_owned());
                        self.release_unattempted_batch(&lease_token).await?;
                        publish_failed = true;
                        break;
                    }
                }
            }
            claim_guard.disarm();
            if publish_failed {
                return Ok(report);
            }
        }
    }

    async fn drain_after_commit(&self) {
        let _ = self.drain_pending_events().await;
    }

    async fn recover_startup_deliveries(&self) -> Result<(), AppError> {
        let now = Utc::now();
        sqlx::query("UPDATE assignment_event_outbox SET status = 'pending', lease_token = NULL, lease_expires_at = NULL, last_error = 'delivery interrupted before acknowledgement; details redacted', updated_at = ? WHERE status = 'delivering'")
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn claim_pending_batch(
        &self,
    ) -> Result<Option<(OutboxClaimGuard, Vec<WorkEventEnvelope>)>, AppError> {
        let now = Utc::now();
        let lease_token = Uuid::new_v4().to_string();
        let lease_expires_at = now
            .checked_add_signed(OUTBOX_LEASE_DURATION)
            .unwrap_or(DateTime::<Utc>::MAX_UTC);
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let active_claims: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM assignment_event_outbox WHERE status = 'delivering'",
        )
        .fetch_one(&mut *transaction)
        .await?;
        if active_claims != 0 {
            transaction.commit().await?;
            return Ok(None);
        }
        let rows = sqlx::query_as::<_, EventRow>(
            "SELECT events.id, events.work_id, events.run_id, events.turn_id, events.session_id, events.agent_id, events.assignment_id, events.causation_id, events.correlation_id, events.sequence, events.version, events.occurred_at, events.payload FROM assignment_event_outbox outbox INNER JOIN events ON events.id = outbox.event_id WHERE outbox.status = 'pending' ORDER BY outbox.ordinal LIMIT ?",
        )
        .bind(OUTBOX_BATCH_SIZE)
        .fetch_all(&mut *transaction)
        .await?;
        if rows.is_empty() {
            transaction.commit().await?;
            return Ok(None);
        }
        let events = rows
            .into_iter()
            .map(WorkEventEnvelope::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        let claimed = sqlx::query("UPDATE assignment_event_outbox SET status = 'delivering', lease_token = ?, lease_expires_at = ?, updated_at = ? WHERE status = 'pending' AND ordinal IN (SELECT ordinal FROM assignment_event_outbox WHERE status = 'pending' ORDER BY ordinal LIMIT ?)")
            .bind(&lease_token)
            .bind(lease_expires_at)
            .bind(now)
            .bind(OUTBOX_BATCH_SIZE)
            .execute(&mut *transaction)
            .await?;
        if claimed.rows_affected() != events.len() as u64 {
            return Err(AppError::event_publish(
                "outbox delivery claim changed concurrently",
            ));
        }
        let claim_guard = OutboxClaimGuard::new(self.pool.clone(), lease_token);
        transaction.commit().await?;
        Ok(Some((claim_guard, events)))
    }

    async fn record_delivery_attempt(
        &self,
        event_id: &str,
        lease_token: &str,
    ) -> Result<(), AppError> {
        let now = Utc::now();
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let updated = sqlx::query("UPDATE assignment_event_outbox SET attempt_count = attempt_count + 1, last_attempt_at = ?, last_error = NULL, updated_at = ? WHERE event_id = ? AND status = 'delivering' AND lease_token = ?")
            .bind(now)
            .bind(now)
            .bind(event_id)
            .bind(lease_token)
            .execute(&mut *transaction)
            .await?;
        if updated.rows_affected() != 1 {
            return Err(AppError::event_publish(
                "outbox delivery attempt lease was lost",
            ));
        }
        transaction.commit().await?;
        Ok(())
    }

    async fn acknowledge_delivery(
        &self,
        event_id: &str,
        lease_token: &str,
    ) -> Result<(), AppError> {
        let now = Utc::now();
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let updated = sqlx::query("UPDATE assignment_event_outbox SET status = 'delivered', lease_token = NULL, lease_expires_at = NULL, last_error = NULL, delivered_at = ?, updated_at = ? WHERE event_id = ? AND status = 'delivering' AND lease_token = ?")
            .bind(now)
            .bind(now)
            .bind(event_id)
            .bind(lease_token)
            .execute(&mut *transaction)
            .await?;
        if updated.rows_affected() != 1 {
            return Err(AppError::event_publish(
                "outbox delivery acknowledgement lease was lost",
            ));
        }
        transaction.commit().await?;
        Ok(())
    }

    async fn record_delivery_failure(
        &self,
        event_id: &str,
        lease_token: &str,
        error: &AppError,
    ) -> Result<(), AppError> {
        let now = Utc::now();
        let redacted_error = redacted_delivery_error(error);
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let updated = sqlx::query("UPDATE assignment_event_outbox SET status = 'pending', lease_token = NULL, lease_expires_at = NULL, last_error = ?, updated_at = ? WHERE event_id = ? AND status = 'delivering' AND lease_token = ?")
            .bind(redacted_error)
            .bind(now)
            .bind(event_id)
            .bind(lease_token)
            .execute(&mut *transaction)
            .await?;
        if updated.rows_affected() != 1 {
            return Err(AppError::event_publish(
                "outbox delivery failure lease was lost",
            ));
        }
        transaction.commit().await?;
        Ok(())
    }

    async fn release_claimed_batch_after_error(&self, lease_token: &str) -> Result<(), AppError> {
        let now = Utc::now();
        sqlx::query("UPDATE assignment_event_outbox SET status = 'pending', lease_token = NULL, lease_expires_at = NULL, last_error = 'delivery acknowledgement failed; details redacted', updated_at = ? WHERE status = 'delivering' AND lease_token = ?")
            .bind(now)
            .bind(lease_token)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn release_unattempted_batch(&self, lease_token: &str) -> Result<(), AppError> {
        let now = Utc::now();
        sqlx::query("UPDATE assignment_event_outbox SET status = 'pending', lease_token = NULL, lease_expires_at = NULL, updated_at = ? WHERE status = 'delivering' AND lease_token = ?")
            .bind(now)
            .bind(lease_token)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

const ASSIGNMENT_SELECT: &str = "SELECT id, work_id, parent_assignment_id, created_by_agent_id, assigned_agent_id, capability_pack_id, kind, side_effect, title, instruction, context_manifest_json, expected_result_schema_json, acceptance_criteria_json, permission_scope_json, priority, status, attempt_count, max_attempts, not_before, result_summary, last_error, next_attempt_at, recovery_reason, runtime_owner_id, created_at, claimed_at, started_at, completed_at, updated_at FROM assignments";

#[derive(FromRow)]
struct AssignmentRow {
    id: String,
    work_id: String,
    parent_assignment_id: Option<String>,
    created_by_agent_id: Option<String>,
    assigned_agent_id: String,
    capability_pack_id: Option<String>,
    kind: AssignmentKind,
    side_effect: AssignmentSideEffect,
    title: String,
    instruction: String,
    context_manifest_json: String,
    expected_result_schema_json: String,
    acceptance_criteria_json: String,
    permission_scope_json: String,
    priority: i64,
    status: AssignmentStatus,
    attempt_count: i64,
    max_attempts: i64,
    not_before: Option<DateTime<Utc>>,
    result_summary: Option<String>,
    last_error: Option<String>,
    next_attempt_at: Option<DateTime<Utc>>,
    recovery_reason: Option<String>,
    runtime_owner_id: Option<String>,
    created_at: DateTime<Utc>,
    claimed_at: Option<DateTime<Utc>>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
}

struct AssignmentRecord {
    summary: AssignmentSummary,
    runtime_owner_id: Option<String>,
}

impl std::ops::Deref for AssignmentRecord {
    type Target = AssignmentSummary;
    fn deref(&self) -> &Self::Target {
        &self.summary
    }
}

impl std::ops::DerefMut for AssignmentRecord {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.summary
    }
}

impl TryFrom<AssignmentRow> for AssignmentRecord {
    type Error = AppError;

    fn try_from(row: AssignmentRow) -> Result<Self, Self::Error> {
        let parse = |field: &'static str, value: &str| {
            serde_json::from_str(value).map_err(|error| {
                AppError::Database(sqlx::Error::Decode(
                    format!("invalid {field}: {error}").into(),
                ))
            })
        };
        let summary = AssignmentSummary {
            id: row.id,
            work_id: row.work_id,
            parent_assignment_id: row.parent_assignment_id,
            created_by_agent_id: row.created_by_agent_id,
            assigned_agent_id: row.assigned_agent_id,
            capability_pack_id: row.capability_pack_id,
            kind: row.kind,
            side_effect: row.side_effect,
            title: row.title,
            instruction: row.instruction,
            context_manifest: parse("context manifest", &row.context_manifest_json)?,
            expected_result_schema: parse(
                "expected result schema",
                &row.expected_result_schema_json,
            )?,
            acceptance_criteria: parse("acceptance criteria", &row.acceptance_criteria_json)?,
            permission_scope: parse("permission scope", &row.permission_scope_json)?,
            priority: u32::try_from(row.priority)
                .map_err(|_| invalid_assignment_state("stored priority is invalid"))?,
            status: row.status,
            attempt_count: u32::try_from(row.attempt_count)
                .map_err(|_| invalid_assignment_state("stored attempt count is invalid"))?,
            max_attempts: u32::try_from(row.max_attempts)
                .map_err(|_| invalid_assignment_state("stored attempt limit is invalid"))?,
            not_before: row.not_before,
            result_summary: row.result_summary,
            last_error: row.last_error,
            next_attempt_at: row.next_attempt_at,
            recovery_reason: row.recovery_reason,
            created_at: row.created_at,
            claimed_at: row.claimed_at,
            started_at: row.started_at,
            completed_at: row.completed_at,
            updated_at: row.updated_at,
        };
        Ok(Self {
            summary,
            runtime_owner_id: row.runtime_owner_id,
        })
    }
}

fn load_assignments(rows: Vec<AssignmentRow>) -> Result<Vec<AssignmentSummary>, AppError> {
    rows.into_iter()
        .map(AssignmentRecord::try_from)
        .map(|result| result.map(|record| record.summary))
        .collect()
}

async fn load_assignment(
    connection: &mut sqlx::SqliteConnection,
    assignment_id: &str,
) -> Result<AssignmentRecord, AppError> {
    let row = sqlx::query_as::<_, AssignmentRow>(&format!("{} WHERE id = ?", ASSIGNMENT_SELECT))
        .bind(assignment_id)
        .fetch_optional(connection)
        .await?
        .ok_or_else(|| AppError::invalid_input("assignmentId", "assignment was not found"))?;
    AssignmentRecord::try_from(row)
}

#[derive(FromRow)]
struct RunRow {
    id: String,
    work_id: String,
    assignment_id: Option<String>,
    agent_instance_id: Option<String>,
    engine_session_id: Option<String>,
    status: RunStatus,
    attempt_number: Option<i64>,
}

async fn load_run(
    connection: &mut sqlx::SqliteConnection,
    run_id: &str,
) -> Result<RunRow, AppError> {
    sqlx::query_as("SELECT id, work_id, assignment_id, agent_instance_id, engine_session_id, status, attempt_number FROM runs WHERE id = ?")
        .bind(run_id).fetch_optional(connection).await?
        .ok_or_else(|| AppError::run_not_found(run_id))
}

fn validate_active_identity(
    assignment: &AssignmentRecord,
    run: &RunRow,
    runtime_owner: &str,
    session_id: Option<&str>,
) -> Result<(), AppError> {
    if assignment.runtime_owner_id.as_deref() != Some(runtime_owner)
        || run.assignment_id.as_deref() != Some(assignment.id.as_str())
        || run.work_id != assignment.work_id
        || run.agent_instance_id.as_deref() != Some(assignment.assigned_agent_id.as_str())
        || run.attempt_number != Some(i64::from(assignment.attempt_count))
        || session_id.is_some_and(|session| run.engine_session_id.as_deref() != Some(session))
    {
        return Err(invalid_assignment_state(
            "assignment, Run, Agent, session, or owner identity mismatch",
        ));
    }
    Ok(())
}

async fn dependencies_terminal_in(
    connection: &mut sqlx::SqliteConnection,
    assignment_id: &str,
) -> Result<bool, AppError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM assignment_dependencies dependencies INNER JOIN assignments predecessors ON predecessors.id = dependencies.depends_on_assignment_id WHERE dependencies.assignment_id = ? AND predecessors.status NOT IN ('completed', 'cancelled', 'dead_letter')",
    ).bind(assignment_id).fetch_one(connection).await?;
    Ok(count == 0)
}

async fn assignment_event(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    assignment: &AssignmentSummary,
    run_id: Option<&str>,
    session_id: Option<String>,
    occurred_at: DateTime<Utc>,
    payload: WorkEventPayload,
) -> Result<WorkEventEnvelope, AppError> {
    let current: Option<i64> = if let Some(run_id) = run_id {
        sqlx::query_scalar("SELECT MAX(sequence) FROM events WHERE run_id = ?")
            .bind(run_id)
            .fetch_one(&mut **transaction)
            .await?
    } else {
        sqlx::query_scalar(
            "SELECT MAX(sequence) FROM events WHERE assignment_id = ? AND run_id IS NULL",
        )
        .bind(&assignment.id)
        .fetch_one(&mut **transaction)
        .await?
    };
    let next = current
        .unwrap_or(0)
        .checked_add(1)
        .filter(|value| *value <= i64::from(u32::MAX))
        .ok_or_else(|| AppError::invalid_input("sequence", "event sequence limit exceeded"))?;
    let event = WorkEventEnvelope {
        version: ASSIGNMENT_EVENT_VERSION,
        event_id: Some(Uuid::new_v4().to_string()),
        work_id: assignment.work_id.clone(),
        run_id: run_id.map(str::to_owned),
        turn_id: None,
        session_id,
        agent_id: Some(assignment.assigned_agent_id.clone()),
        assignment_id: Some(assignment.id.clone()),
        causation_id: None,
        correlation_id: Some(assignment.id.clone()),
        sequence: u32::try_from(next).expect("bounded sequence"),
        occurred_at,
        payload,
    };
    insert_event(transaction, &event).await?;
    Ok(event)
}

fn stable_seed(id: &str) -> u64 {
    let digest = Sha256::digest(id.as_bytes());
    u64::from_le_bytes(digest[..8].try_into().expect("SHA-256 prefix"))
}

fn bounded_non_empty(field: &str, value: &str, max_bytes: usize) -> Result<String, AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::invalid_input(
            field,
            format!("{field} must not be empty"),
        ));
    }
    if value.len() > max_bytes {
        return Err(AppError::invalid_input(
            field,
            format!("{field} is too large"),
        ));
    }
    Ok(value.to_owned())
}

fn validate_id(field: &str, value: &str) -> Result<String, AppError> {
    bounded_non_empty(field, value, MAX_ID_BYTES)
}

fn validate_optional_id(field: &str, value: Option<&str>) -> Result<Option<String>, AppError> {
    value.map(|value| validate_id(field, value)).transpose()
}

fn validate_label(field: &str, value: &str) -> Result<String, AppError> {
    bounded_non_empty(field, value, MAX_LABEL_BYTES)
}

fn invalid_assignment_state(message: &str) -> AppError {
    AppError::invalid_input("assignmentStatus", message)
}

fn validate_text(field: &str, value: &str) -> Result<String, AppError> {
    bounded_non_empty(field, value, MAX_TEXT_BYTES)
}

fn validate_json(field: &str, value: &Value, expected: &str) -> Result<String, AppError> {
    let correct_type = matches!(
        (expected, value),
        ("object", Value::Object(_)) | ("array", Value::Array(_))
    );
    if !correct_type {
        return Err(AppError::invalid_input(
            field,
            format!("{field} must be a JSON {expected}"),
        ));
    }
    let serialized = serde_json::to_string(value)
        .map_err(|error| AppError::Database(sqlx::Error::Encode(Box::new(error))))?;
    if serialized.len() > MAX_JSON_BYTES {
        return Err(AppError::invalid_input(
            field,
            format!("{field} is too large"),
        ));
    }
    Ok(serialized)
}

fn redacted_delivery_error(error: &AppError) -> String {
    let category = match error {
        AppError::EventPublish { .. } => "event_publish",
        _ => "sink_error",
    };
    let message = format!("{category}: delivery failed; details redacted");
    debug_assert!(message.len() <= MAX_OUTBOX_ERROR_BYTES);
    message
}

async fn insert_event(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    event: &WorkEventEnvelope,
) -> Result<(), AppError> {
    let payload = serde_json::to_string(&event.payload)
        .map_err(|error| AppError::Database(sqlx::Error::Encode(Box::new(error))))?;
    let event_id = event
        .event_id
        .as_deref()
        .expect("repository events have IDs");
    let assignment_id = event.assignment_id.as_deref().ok_or_else(|| {
        AppError::invalid_input("assignmentId", "Assignment events require an assignment id")
    })?;
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
        .await?;
    sqlx::query("INSERT INTO assignment_event_outbox (event_id, assignment_id, created_at, updated_at) VALUES (?, ?, ?, ?)")
        .bind(event_id)
        .bind(assignment_id)
        .bind(event.occurred_at)
        .bind(event.occurred_at)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

#[derive(FromRow)]
struct EventRow {
    id: String,
    work_id: String,
    run_id: Option<String>,
    turn_id: Option<String>,
    session_id: Option<String>,
    agent_id: Option<String>,
    assignment_id: Option<String>,
    causation_id: Option<String>,
    correlation_id: Option<String>,
    sequence: i64,
    version: i64,
    occurred_at: DateTime<Utc>,
    payload: String,
}

impl TryFrom<EventRow> for WorkEventEnvelope {
    type Error = AppError;

    fn try_from(row: EventRow) -> Result<Self, Self::Error> {
        let payload = serde_json::from_str(&row.payload)
            .map_err(|error| AppError::Database(sqlx::Error::Decode(Box::new(error))))?;
        Ok(Self {
            version: u32::try_from(row.version).map_err(|_| {
                AppError::invalid_input("version", "stored event version is invalid")
            })?,
            event_id: Some(row.id),
            work_id: row.work_id,
            run_id: row.run_id,
            turn_id: row.turn_id,
            session_id: row.session_id,
            agent_id: row.agent_id,
            assignment_id: row.assignment_id,
            causation_id: row.causation_id,
            correlation_id: row.correlation_id,
            sequence: u32::try_from(row.sequence).map_err(|_| {
                AppError::invalid_input("sequence", "stored event sequence is invalid")
            })?,
            occurred_at: row.occurred_at,
            payload,
        })
    }
}
