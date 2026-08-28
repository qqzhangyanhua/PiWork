use std::{sync::Arc, time::Duration};

use chrono::{DateTime, Utc};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::{domain::event::WorkEventEnvelope, error::AppError};

const MAX_OUTBOX_ERROR_BYTES: usize = 512;
const OUTBOX_BATCH_SIZE: i64 = 64;
const OUTBOX_DIAGNOSTIC_LIMIT: usize = 256;
const OUTBOX_LEASE_DURATION: chrono::Duration = chrono::Duration::seconds(30);
const OUTBOX_CLAIM_POLL_INTERVAL: Duration = Duration::from_millis(10);
const OUTBOX_CLAIM_POLL_MAX_INTERVAL: Duration = Duration::from_millis(250);
const HAS_PENDING_EVENT_DELIVERIES_SQL: &str =
    "SELECT EXISTS(SELECT 1 FROM assignment_event_outbox WHERE status <> 'delivered' LIMIT 1)";

/// Receives an at-least-once transport stream. Implementations and end-to-end consumers must
/// deduplicate the stable `event_id` before applying externally visible side effects because a
/// process crash can occur after publish but before database acknowledgement. The confirmation
/// callback is only a process-local lifecycle hint; it does not provide distributed exactly-once
/// delivery.
pub trait AssignmentEventSink: Send + Sync {
    fn publish(&self, event: WorkEventEnvelope) -> Result<(), AppError>;

    fn delivery_confirmed(&self, _event_id: &str) {}
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
pub struct AssignmentEventOutbox {
    pool: SqlitePool,
    event_sink: Arc<dyn AssignmentEventSink>,
}

struct OutboxClaimGuard {
    pool: SqlitePool,
    lease_token: Option<String>,
}

struct OutboxClaimPoll {
    delay: Duration,
}

impl Default for OutboxClaimPoll {
    fn default() -> Self {
        Self {
            delay: OUTBOX_CLAIM_POLL_INTERVAL,
        }
    }
}

impl OutboxClaimPoll {
    fn next_delay(&mut self) -> Duration {
        let current = self.delay;
        self.delay = self
            .delay
            .saturating_mul(2)
            .min(OUTBOX_CLAIM_POLL_MAX_INTERVAL);
        current
    }
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
            release_abandoned_claim(pool, lease_token).await;
        }));
    }
}

async fn release_abandoned_claim(pool: SqlitePool, lease_token: String) {
    let mut retry = OutboxClaimPoll::default();
    let mut failure_reported = false;
    loop {
        if pool.is_closed() {
            return;
        }
        let now = Utc::now();
        let released = sqlx::query("UPDATE assignment_event_outbox SET status = 'pending', lease_token = NULL, lease_expires_at = NULL, last_error = 'delivery interrupted before acknowledgement; details redacted', updated_at = ? WHERE status = 'delivering' AND lease_token = ?")
            .bind(now)
            .bind(&lease_token)
            .execute(&pool)
            .await;
        match released {
            Ok(_) => return,
            Err(_) if pool.is_closed() => return,
            Err(_) => {
                if !failure_reported {
                    eprintln!("PiWork outbox claim cleanup is retrying after a database error.");
                    failure_reported = true;
                }
            }
        }
        let still_owned = sqlx::query_scalar::<_, i64>(
            "SELECT EXISTS(SELECT 1 FROM assignment_event_outbox WHERE status = 'delivering' AND lease_token = ? LIMIT 1)",
        )
        .bind(&lease_token)
        .fetch_one(&pool)
        .await;
        match still_owned {
            Ok(0) => return,
            Err(_) if pool.is_closed() => return,
            Ok(_) | Err(_) => tokio::time::sleep(retry.next_delay()).await,
        }
    }
}

impl AssignmentEventOutbox {
    pub fn new(pool: SqlitePool, event_sink: Arc<dyn AssignmentEventSink>) -> Self {
        Self { pool, event_sink }
    }

    pub(crate) fn unavailable(pool: SqlitePool) -> Self {
        Self::new(pool, Arc::new(UnavailableEventSink))
    }

    /// Exclusive startup recovery. Reclaims deliveries left in `delivering` by a previous process.
    /// Runtime drains never steal an in-flight lease after a wall-clock deadline.
    pub async fn recover(&self) -> Result<(), AppError> {
        let now = Utc::now();
        sqlx::query("UPDATE assignment_event_outbox SET status = 'pending', lease_token = NULL, lease_expires_at = NULL, last_error = 'delivery interrupted before acknowledgement; details redacted', updated_at = ? WHERE status = 'delivering'")
            .bind(now)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Returns a bounded diagnostic snapshot rather than materializing the entire outbox backlog.
    pub async fn pending_event_deliveries(&self) -> Result<Vec<PendingEventDelivery>, AppError> {
        self.pending_event_deliveries_limited(OUTBOX_DIAGNOSTIC_LIMIT)
            .await
    }

    pub(crate) async fn pending_event_deliveries_limited(
        &self,
        requested_limit: usize,
    ) -> Result<Vec<PendingEventDelivery>, AppError> {
        let limit = requested_limit.min(OUTBOX_DIAGNOSTIC_LIMIT) as i64;
        Ok(sqlx::query_as::<_, PendingEventDelivery>(
            "SELECT ordinal, event_id, assignment_id, status, attempt_count, last_attempt_at, last_error, lease_expires_at FROM assignment_event_outbox WHERE status <> 'delivered' ORDER BY ordinal LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Drains globally ordered, fixed-size batches of undelivered Assignment events.
    ///
    /// Delivery is at-least-once across the publish/ack crash boundary. Every v2 Assignment event
    /// has a stable `event_id`; sinks and consumers must use it as their idempotency key. A batch
    /// process-owned lease serializes concurrent outbox instances without holding a SQLite
    /// lock while the external sink runs. Only the exclusive startup recovery path reclaims a
    /// delivery, so a slow in-flight sink is never stolen after a wall-clock deadline.
    pub async fn drain(&self) -> Result<OutboxDrainReport, AppError> {
        let mut report = OutboxDrainReport {
            attempted: 0,
            published: 0,
            failed_event_ids: Vec::new(),
        };
        loop {
            let mut claim_poll = OutboxClaimPoll::default();
            let (mut claim_guard, events) = loop {
                if let Some(claimed) = self.claim_pending_batch().await? {
                    break claimed;
                }
                if !self.has_pending_event_deliveries().await? {
                    return Ok(report);
                }
                tokio::time::sleep(claim_poll.next_delay()).await;
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
                        self.event_sink.delivery_confirmed(event_id);
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

    /// Enqueues an Assignment event on an already-open transaction so the events journal and
    /// outbox row commit together. Callers must pass the same `event_id` / `assignment_id` already
    /// written (or about to be written) to `events` in this transaction.
    pub(crate) async fn insert(
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        event_id: &str,
        assignment_id: &str,
        occurred_at: DateTime<Utc>,
    ) -> Result<(), AppError> {
        sqlx::query("INSERT INTO assignment_event_outbox (event_id, assignment_id, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(event_id)
            .bind(assignment_id)
            .bind(occurred_at)
            .bind(occurred_at)
            .execute(&mut **transaction)
            .await?;
        Ok(())
    }

    async fn has_pending_event_deliveries(&self) -> Result<bool, AppError> {
        let exists: i64 = sqlx::query_scalar(HAS_PENDING_EVENT_DELIVERIES_SQL)
            .fetch_one(&self.pool)
            .await?;
        Ok(exists != 0)
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

fn redacted_delivery_error(error: &AppError) -> String {
    let category = match error {
        AppError::EventPublish { .. } => "event_publish",
        _ => "sink_error",
    };
    let message = format!("{category}: delivery failed; details redacted");
    debug_assert!(message.len() <= MAX_OUTBOX_ERROR_BYTES);
    message
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

#[cfg(test)]
mod tests;
