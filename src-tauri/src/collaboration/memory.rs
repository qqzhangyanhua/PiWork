//! Confirmed-only Agent memory: a Member can propose candidates inside a Result
//! envelope, but nothing reaches `agent_memory` until the Lead (or user)
//! confirms it. A sensitive-content scan is a first line of defense, never a
//! substitute for explicit confirmation.

use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    domain::collaboration::{MemoryCandidateInput, MemoryCandidateStatus, MemoryCandidateSummary},
    error::AppError,
};

#[derive(Clone)]
pub struct MemoryService {
    pool: SqlitePool,
}

impl MemoryService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Proposes candidates from a Result envelope. Candidates are persisted as
    /// `proposed` and never write `agent_memory` directly.
    pub async fn propose_candidates(
        &self,
        work_id: &str,
        author_agent_id: &str,
        candidates: Vec<MemoryCandidateInput>,
    ) -> Result<Vec<String>, AppError> {
        let now = Utc::now();
        let mut ids = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            if candidate.content.trim().is_empty() {
                continue;
            }
            if contains_sensitive_content(&candidate.content) {
                continue;
            }
            let id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO memory_candidates (id, source_work_id, author_agent_id, content, reason, version, status, created_at) \
                 VALUES (?, ?, ?, ?, ?, 1, 'proposed', ?)",
            )
            .bind(&id)
            .bind(work_id)
            .bind(author_agent_id)
            .bind(&candidate.content)
            .bind(&candidate.reason)
            .bind(now)
            .execute(&self.pool)
            .await?;
            ids.push(id);
        }
        Ok(ids)
    }

    pub async fn list_work_candidates(
        &self,
        work_id: &str,
    ) -> Result<Vec<MemoryCandidateSummary>, AppError> {
        sqlx::query_as::<_, MemoryCandidateRow>(
            "SELECT id, source_work_id, source_event_id, author_agent_id, content, reason, version, status, created_at, resolved_at, resolved_by \
             FROM memory_candidates WHERE source_work_id = ? AND status = 'proposed' ORDER BY created_at, id",
        )
        .bind(work_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(MemoryCandidateSummary::try_from)
        .collect()
    }

    /// Confirms (writes `agent_memory` + event) or rejects (keeps audit) one
    /// candidate. Confirmation is idempotent and always re-runs the sensitive
    /// scan before writing durable memory.
    pub async fn resolve_candidate(
        &self,
        candidate_id: &str,
        confirm: bool,
        actor: &str,
    ) -> Result<MemoryCandidateSummary, AppError> {
        let candidate: MemoryCandidateRow = sqlx::query_as(
            "SELECT id, source_work_id, source_event_id, author_agent_id, content, reason, version, status, created_at, resolved_at, resolved_by \
             FROM memory_candidates WHERE id = ?",
        )
        .bind(candidate_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::invalid_input("candidateId", "memory candidate not found"))?;

        if candidate.status != "proposed" {
            return MemoryCandidateSummary::try_from(candidate);
        }
        if confirm && contains_sensitive_content(&candidate.content) {
            return Err(AppError::invalid_input(
                "content",
                "memory candidate contains sensitive content",
            ));
        }

        let now = Utc::now();
        let status = if confirm { "confirmed" } else { "rejected" };
        if confirm {
            sqlx::query(
                "INSERT INTO agent_memory (id, agent_instance_id, source_work_id, source_event_id, author_agent_id, content, reason, version, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(&candidate.author_agent_id)
            .bind(&candidate.source_work_id)
            .bind(&candidate.source_event_id)
            .bind(&candidate.author_agent_id)
            .bind(&candidate.content)
            .bind(&candidate.reason)
            .bind(i64::from(candidate.version))
            .bind(now)
            .execute(&self.pool)
            .await?;
        }
        sqlx::query(
            "UPDATE memory_candidates SET status = ?, resolved_at = ?, resolved_by = ? WHERE id = ? AND status = 'proposed'",
        )
        .bind(status)
        .bind(now)
        .bind(actor)
        .bind(candidate_id)
        .execute(&self.pool)
        .await?;

        // The candidate status transition and the agent_memory row (on confirm)
        // are the durable facts; the MemoryCandidateResolved event is emitted by
        // the assignment-scoped collaboration path once a source assignment id
        // is attached to candidates.
        let resolved: MemoryCandidateRow = sqlx::query_as(
            "SELECT id, source_work_id, source_event_id, author_agent_id, content, reason, version, status, created_at, resolved_at, resolved_by \
             FROM memory_candidates WHERE id = ?",
        )
        .bind(candidate_id)
        .fetch_one(&self.pool)
        .await?;
        MemoryCandidateSummary::try_from(resolved)
    }

    /// Lists confirmed memories for an Agent, bounded to `budget` characters.
    pub async fn list_agent_memory(
        &self,
        agent_id: &str,
        budget: usize,
    ) -> Result<Vec<String>, AppError> {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT content FROM agent_memory WHERE agent_instance_id = ? ORDER BY version DESC, created_at DESC",
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;
        let mut total = 0usize;
        let mut out = Vec::new();
        for content in rows {
            if total + content.chars().count() > budget {
                break;
            }
            total += content.chars().count();
            out.push(content);
        }
        Ok(out)
    }
}

fn contains_sensitive_content(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    lower.contains("sk-")
        || lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("api-key")
        || lower.contains("akia")
        || lower.contains("-----begin")
        || lower.contains("private key")
        || lower.contains("password=")
        || lower.contains("secret=")
        || lower.contains("token=")
        || content.contains("/tmp/")
        || content.contains("\\temp\\")
        || content.contains("appdata\\local\\temp")
}

#[derive(sqlx::FromRow)]
struct MemoryCandidateRow {
    id: String,
    source_work_id: String,
    source_event_id: Option<String>,
    author_agent_id: String,
    content: String,
    reason: String,
    version: i64,
    status: String,
    created_at: chrono::DateTime<Utc>,
    resolved_at: Option<chrono::DateTime<Utc>>,
    resolved_by: Option<String>,
}

impl TryFrom<MemoryCandidateRow> for MemoryCandidateSummary {
    type Error = AppError;

    fn try_from(row: MemoryCandidateRow) -> Result<Self, Self::Error> {
        let status = match row.status.as_str() {
            "proposed" => MemoryCandidateStatus::Proposed,
            "confirmed" => MemoryCandidateStatus::Confirmed,
            "rejected" => MemoryCandidateStatus::Rejected,
            other => {
                return Err(AppError::invalid_input(
                    "status",
                    format!("unknown memory candidate status {other:?}"),
                ));
            }
        };
        Ok(MemoryCandidateSummary {
            id: row.id,
            source_work_id: row.source_work_id,
            source_event_id: row.source_event_id,
            author_agent_id: row.author_agent_id,
            content: row.content,
            reason: row.reason,
            version: u32::try_from(row.version).expect("bounded version"),
            status,
            created_at: row.created_at,
            resolved_at: row.resolved_at,
            resolved_by: row.resolved_by,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::contains_sensitive_content;

    #[test]
    fn sensitive_content_is_detected() {
        assert!(contains_sensitive_content("the api key is sk-abc123"));
        assert!(contains_sensitive_content("-----BEGIN PRIVATE KEY-----"));
        assert!(contains_sensitive_content("use /tmp/scratch"));
        assert!(contains_sensitive_content("password=hunter2"));
        assert!(!contains_sensitive_content("the queue uses a BTreeMap"));
    }
}
