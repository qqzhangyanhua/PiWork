use std::collections::HashSet;

use chrono::{DateTime, Utc};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::{
    document_runtime::DocumentResult,
    domain::resource::{ResourceOrigin, ResourceStatus, ResourceSummary},
    error::AppError,
    resource::blob_store::StorageRef,
};

use super::service::{LOCAL_BLOB_STORE_ID, LOCAL_PERSONAL_SPACE_ID};

#[derive(Debug, Clone)]
pub struct DocumentDerivative {
    pub resource_id: String,
    pub cache_key: String,
    pub content_chars: u64,
}

#[derive(Debug, Clone)]
pub struct RecoverableDocument {
    pub resource_id: String,
    pub media_type: String,
    pub object_key: String,
    pub cache_key: String,
}

pub struct DocumentDerivativeCompletion<'a> {
    pub cache_key: &'a str,
    pub result: &'a DocumentResult,
}

#[derive(Clone)]
pub struct ResourceRepository {
    pool: SqlitePool,
}

#[derive(Debug, Clone)]
pub struct EngineResource {
    pub id: String,
    pub original_name: String,
    pub media_type: String,
    pub size: u64,
    pub object_key: String,
}

impl ResourceRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create_staging(
        &self,
        resource_id: &str,
        original_name: &str,
        size: u64,
        draft_id: Option<&str>,
        work_id: Option<&str>,
    ) -> Result<ResourceSummary, AppError> {
        let target_count = usize::from(draft_id.is_some()) + usize::from(work_id.is_some());
        if target_count != 1
            || draft_id.is_some_and(|value| value.trim().is_empty())
            || work_id.is_some_and(|value| value.trim().is_empty())
        {
            return Err(AppError::invalid_input(
                "resourceTarget",
                "exactly one Work or draft target is required",
            ));
        }
        let size = i64::try_from(size)
            .map_err(|_| AppError::invalid_input("sourcePaths", "attachment is too large"))?;
        let now = Utc::now();
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let space_id = if let Some(work_id) = work_id {
            sqlx::query_scalar::<_, String>("SELECT space_id FROM works WHERE id = ?")
                .bind(work_id)
                .fetch_optional(&mut *transaction)
                .await?
                .ok_or_else(|| AppError::work_not_found(work_id))?
        } else {
            LOCAL_PERSONAL_SPACE_ID.to_owned()
        };
        sqlx::query(
            "INSERT INTO managed_resources \
             (id, space_id, blob_id, original_name, media_type, size, origin, status, \
              failure_code, created_at, updated_at) \
             VALUES (?, ?, NULL, ?, 'application/octet-stream', ?, 'user_upload', \
                     'staging', NULL, ?, ?)",
        )
        .bind(resource_id)
        .bind(&space_id)
        .bind(original_name)
        .bind(size)
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO resource_links \
             (id, resource_id, work_id, draft_id, message_id, run_id, role, created_at) \
             VALUES (?, ?, ?, ?, NULL, NULL, 'attached', ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(resource_id)
        .bind(work_id)
        .bind(draft_id)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        self.find(resource_id)
            .await?
            .ok_or_else(|| AppError::resource_not_found(resource_id))
    }

    pub async fn complete_import(
        &self,
        resource_id: &str,
        media_type: &str,
        sha256: &str,
        size: u64,
        object_key: &str,
    ) -> Result<ResourceSummary, AppError> {
        let size_i64 = i64::try_from(size)
            .map_err(|_| AppError::invalid_input("sourcePaths", "attachment is too large"))?;
        let now = Utc::now();
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let blob_id = sqlx::query_scalar::<_, String>(
            "SELECT id FROM resource_blobs WHERE plaintext_sha256 = ? AND size = ?",
        )
        .bind(sha256)
        .bind(size_i64)
        .fetch_optional(&mut *transaction)
        .await?;
        let blob_id = match blob_id {
            Some(blob_id) => blob_id,
            None => {
                let blob_id = Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO resource_blobs (id, plaintext_sha256, size, created_at) \
                     VALUES (?, ?, ?, ?)",
                )
                .bind(&blob_id)
                .bind(sha256)
                .bind(size_i64)
                .bind(now)
                .execute(&mut *transaction)
                .await?;
                blob_id
            }
        };
        sqlx::query(
            "INSERT INTO blob_replicas \
             (blob_id, store_id, object_key, state, created_at, updated_at) \
             VALUES (?, ?, ?, 'ready', ?, ?) \
             ON CONFLICT(blob_id, store_id) DO UPDATE SET \
               object_key = excluded.object_key, state = 'ready', updated_at = excluded.updated_at",
        )
        .bind(&blob_id)
        .bind(LOCAL_BLOB_STORE_ID)
        .bind(object_key)
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        let updated = sqlx::query(
            "UPDATE managed_resources \
             SET blob_id = ?, media_type = ?, size = ?, status = 'ready', \
                 failure_code = NULL, updated_at = ? \
             WHERE id = ? AND status IN ('staging', 'processing')",
        )
        .bind(&blob_id)
        .bind(media_type)
        .bind(size_i64)
        .bind(now)
        .bind(resource_id)
        .execute(&mut *transaction)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(AppError::resource_not_found(resource_id));
        }
        transaction.commit().await?;
        self.find(resource_id)
            .await?
            .ok_or_else(|| AppError::resource_not_found(resource_id))
    }

    pub async fn commit_blob_for_processing(
        &self,
        resource_id: &str,
        media_type: &str,
        sha256: &str,
        size: u64,
        object_key: &str,
    ) -> Result<(), AppError> {
        let size = i64::try_from(size)
            .map_err(|_| AppError::invalid_input("sourcePaths", "attachment is too large"))?;
        let now = Utc::now();
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let blob_id = match sqlx::query_scalar::<_, String>(
            "SELECT id FROM resource_blobs WHERE plaintext_sha256 = ? AND size = ?",
        )
        .bind(sha256)
        .bind(size)
        .fetch_optional(&mut *transaction)
        .await?
        {
            Some(id) => id,
            None => {
                let id = Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO resource_blobs (id, plaintext_sha256, size, created_at) VALUES (?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(sha256)
                .bind(size)
                .bind(now)
                .execute(&mut *transaction)
                .await?;
                id
            }
        };
        sqlx::query(
            "INSERT INTO blob_replicas (blob_id, store_id, object_key, state, created_at, updated_at) \
             VALUES (?, ?, ?, 'ready', ?, ?) ON CONFLICT(blob_id, store_id) DO UPDATE SET \
             object_key = excluded.object_key, state = 'ready', updated_at = excluded.updated_at",
        )
        .bind(&blob_id)
        .bind(LOCAL_BLOB_STORE_ID)
        .bind(object_key)
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        let updated = sqlx::query(
            "UPDATE managed_resources SET blob_id = ?, media_type = ?, size = ?, status = 'processing', \
             failure_code = NULL, updated_at = ? WHERE id = ? AND status = 'staging'",
        )
        .bind(&blob_id)
        .bind(media_type)
        .bind(size)
        .bind(now)
        .bind(resource_id)
        .execute(&mut *transaction)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(AppError::resource_not_found(resource_id));
        }
        sqlx::query(
            "INSERT INTO resource_derivatives (resource_id, kind, state, extractor, extractor_version, \
             used_ocr, created_at, updated_at) VALUES (?, 'canonical_markdown', 'processing', \
             'xberg', '1.0.5', 0, ?, ?)",
        )
        .bind(resource_id)
        .bind(now)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn complete_document_import(
        &self,
        resource_id: &str,
        completion: DocumentDerivativeCompletion<'_>,
    ) -> Result<ResourceSummary, AppError> {
        let content_chars = i64::try_from(completion.result.content_chars)
            .map_err(|_| AppError::resource_import("document_output_too_large"))?;
        let now = Utc::now();
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let derivative = sqlx::query(
            "UPDATE resource_derivatives SET state = 'ready', cache_key = ?, content_sha256 = ?, \
             content_chars = ?, used_ocr = ?, extractor = ?, extractor_version = ?, failure_code = NULL, \
             updated_at = ? WHERE resource_id = ? AND kind = 'canonical_markdown'",
        )
        .bind(completion.cache_key)
        .bind(&completion.result.content_sha256)
        .bind(content_chars)
        .bind(completion.result.used_ocr)
        .bind(&completion.result.extractor)
        .bind(&completion.result.extractor_version)
        .bind(now)
        .bind(resource_id)
        .execute(&mut *transaction)
        .await?;
        let resource = sqlx::query(
            "UPDATE managed_resources SET status = 'ready', failure_code = NULL, updated_at = ? \
             WHERE id = ? AND status = 'processing'",
        )
        .bind(now)
        .bind(resource_id)
        .execute(&mut *transaction)
        .await?;
        if derivative.rows_affected() == 0 || resource.rows_affected() == 0 {
            return Err(AppError::resource_not_found(resource_id));
        }
        transaction.commit().await?;
        self.find(resource_id)
            .await?
            .ok_or_else(|| AppError::resource_not_found(resource_id))
    }

    pub async fn fail_document_import(
        &self,
        resource_id: &str,
        failure_code: &str,
    ) -> Result<ResourceSummary, AppError> {
        let now = Utc::now();
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let derivative = sqlx::query(
            "UPDATE resource_derivatives SET state = 'failed', failure_code = ?, updated_at = ? \
             WHERE resource_id = ? AND kind = 'canonical_markdown'",
        )
        .bind(failure_code)
        .bind(now)
        .bind(resource_id)
        .execute(&mut *transaction)
        .await?;
        let resource = sqlx::query(
            "UPDATE managed_resources SET status = 'failed', failure_code = ?, updated_at = ? \
             WHERE id = ? AND status = 'processing'",
        )
        .bind(failure_code)
        .bind(now)
        .bind(resource_id)
        .execute(&mut *transaction)
        .await?;
        if derivative.rows_affected() == 0 || resource.rows_affected() == 0 {
            return Err(AppError::resource_not_found(resource_id));
        }
        transaction.commit().await?;
        self.find(resource_id)
            .await?
            .ok_or_else(|| AppError::resource_not_found(resource_id))
    }

    pub async fn document_derivatives_for_engine(
        &self,
        work_id: &str,
        resource_ids: &[String],
    ) -> Result<Vec<DocumentDerivative>, AppError> {
        if resource_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = serde_json::to_string(resource_ids)
            .map_err(|error| AppError::Database(sqlx::Error::Encode(Box::new(error))))?;
        let rows = sqlx::query_as::<_, (String, String, i64)>(
            "SELECT managed_resources.id, resource_derivatives.cache_key, resource_derivatives.content_chars \
             FROM managed_resources INNER JOIN resource_links ON resource_links.resource_id = managed_resources.id \
             INNER JOIN resource_derivatives ON resource_derivatives.resource_id = managed_resources.id \
             WHERE managed_resources.id IN (SELECT value FROM json_each(?)) AND managed_resources.status = 'ready' \
             AND resource_links.work_id = ? AND resource_links.message_id IS NULL \
             AND resource_derivatives.kind = 'canonical_markdown' AND resource_derivatives.state = 'ready'",
        )
        .bind(ids)
        .bind(work_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|(resource_id, cache_key, content_chars)| {
                Ok(DocumentDerivative {
                    resource_id,
                    cache_key,
                    content_chars: u64::try_from(content_chars)
                        .map_err(|_| AppError::ResourceStorage)?,
                })
            })
            .collect()
    }

    pub async fn recoverable_documents(&self) -> Result<Vec<RecoverableDocument>, AppError> {
        sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT managed_resources.id, managed_resources.media_type, blob_replicas.object_key, \
                    resource_derivatives.cache_key \
             FROM managed_resources \
             INNER JOIN blob_replicas ON blob_replicas.blob_id = managed_resources.blob_id \
             INNER JOIN resource_derivatives ON resource_derivatives.resource_id = managed_resources.id \
             WHERE managed_resources.status = 'ready' \
               AND resource_derivatives.kind = 'canonical_markdown' \
               AND resource_derivatives.state = 'ready' \
               AND blob_replicas.store_id = ? AND blob_replicas.state = 'ready'",
        )
        .bind(LOCAL_BLOB_STORE_ID)
        .fetch_all(&self.pool)
        .await
        .map(|rows| {
            rows.into_iter()
                .map(
                    |(resource_id, media_type, object_key, cache_key)| RecoverableDocument {
                        resource_id,
                        media_type,
                        object_key,
                        cache_key,
                    },
                )
                .collect()
        })
        .map_err(AppError::from)
    }

    pub async fn begin_document_rebuild(&self, resource_id: &str) -> Result<(), AppError> {
        let now = Utc::now();
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let resource = sqlx::query(
            "UPDATE managed_resources SET status = 'processing', updated_at = ? \
             WHERE id = ? AND status = 'ready'",
        )
        .bind(now)
        .bind(resource_id)
        .execute(&mut *transaction)
        .await?;
        let derivative = sqlx::query(
            "UPDATE resource_derivatives SET state = 'processing', failure_code = NULL, updated_at = ? \
             WHERE resource_id = ? AND kind = 'canonical_markdown' AND state = 'ready'",
        )
        .bind(now)
        .bind(resource_id)
        .execute(&mut *transaction)
        .await?;
        if resource.rows_affected() == 0 || derivative.rows_affected() == 0 {
            return Err(AppError::resource_not_found(resource_id));
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn fail_import(
        &self,
        resource_id: &str,
        failure_code: &str,
    ) -> Result<ResourceSummary, AppError> {
        let updated = sqlx::query(
            "UPDATE managed_resources \
             SET status = 'failed', failure_code = ?, updated_at = ? \
             WHERE id = ? AND status IN ('staging', 'processing')",
        )
        .bind(failure_code)
        .bind(Utc::now())
        .bind(resource_id)
        .execute(&self.pool)
        .await?;
        if updated.rows_affected() == 0 {
            return Err(AppError::resource_not_found(resource_id));
        }
        self.find(resource_id)
            .await?
            .ok_or_else(|| AppError::resource_not_found(resource_id))
    }

    pub async fn find(&self, resource_id: &str) -> Result<Option<ResourceSummary>, AppError> {
        sqlx::query_as::<_, ResourceRow>(
            "SELECT id, original_name, media_type, size, origin, status, failure_code, created_at \
             FROM managed_resources WHERE id = ?",
        )
        .bind(resource_id)
        .fetch_optional(&self.pool)
        .await?
        .map(ResourceSummary::try_from)
        .transpose()
    }

    pub async fn list_for_work(&self, work_id: &str) -> Result<Vec<ResourceSummary>, AppError> {
        sqlx::query_as::<_, ResourceRow>(
            "SELECT DISTINCT managed_resources.id, managed_resources.original_name, \
                    managed_resources.media_type, managed_resources.size, \
                    managed_resources.origin, managed_resources.status, \
                    managed_resources.failure_code, managed_resources.created_at \
             FROM managed_resources \
             INNER JOIN resource_links ON resource_links.resource_id = managed_resources.id \
             WHERE resource_links.work_id = ? \
             ORDER BY managed_resources.created_at ASC, managed_resources.id ASC",
        )
        .bind(work_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(ResourceSummary::try_from)
        .collect()
    }

    pub async fn storage_ref(&self, resource_id: &str) -> Result<StorageRef, AppError> {
        let object_key = sqlx::query_scalar::<_, String>(
            "SELECT blob_replicas.object_key \
             FROM managed_resources \
             INNER JOIN blob_replicas ON blob_replicas.blob_id = managed_resources.blob_id \
             WHERE managed_resources.id = ? AND managed_resources.status = 'ready' \
               AND blob_replicas.store_id = ? AND blob_replicas.state = 'ready'",
        )
        .bind(resource_id)
        .bind(LOCAL_BLOB_STORE_ID)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::resource_not_found(resource_id))?;
        Ok(StorageRef { object_key })
    }

    pub async fn engine_resources(
        &self,
        work_id: &str,
        resource_ids: &[String],
    ) -> Result<Vec<EngineResource>, AppError> {
        if resource_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = serde_json::to_string(resource_ids)
            .map_err(|error| AppError::Database(sqlx::Error::Encode(Box::new(error))))?;
        sqlx::query_as::<_, EngineResourceRow>(
            "SELECT DISTINCT managed_resources.id, managed_resources.original_name, managed_resources.media_type, \
                    managed_resources.size, blob_replicas.object_key \
             FROM managed_resources \
             INNER JOIN resource_links ON resource_links.resource_id = managed_resources.id \
             INNER JOIN blob_replicas ON blob_replicas.blob_id = managed_resources.blob_id \
             WHERE managed_resources.id IN (SELECT value FROM json_each(?)) \
               AND managed_resources.status = 'ready' \
               AND resource_links.work_id = ? \
               AND resource_links.message_id IS NULL \
               AND resource_links.role IN ('attached', 'pinned') \
               AND blob_replicas.store_id = ? AND blob_replicas.state = 'ready'",
        )
        .bind(ids)
        .bind(work_id)
        .bind(LOCAL_BLOB_STORE_ID)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(EngineResource::try_from)
        .collect()
    }

    pub async fn detach_draft_resource(
        &self,
        draft_id: &str,
        resource_id: &str,
    ) -> Result<(), AppError> {
        let deleted = sqlx::query(
            "DELETE FROM resource_links \
             WHERE draft_id = ? AND resource_id = ? AND work_id IS NULL",
        )
        .bind(draft_id)
        .bind(resource_id)
        .execute(&self.pool)
        .await?;
        if deleted.rows_affected() == 0 {
            return Err(AppError::resource_not_found(resource_id));
        }
        Ok(())
    }

    pub async fn local_replica_refs(&self) -> Result<HashSet<StorageRef>, AppError> {
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT object_key FROM blob_replicas WHERE store_id = ? AND state = 'ready'",
        )
        .bind(LOCAL_BLOB_STORE_ID)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|object_key| StorageRef { object_key })
        .collect())
    }

    pub async fn recover_stale_staging(&self, cutoff: DateTime<Utc>) -> Result<u64, AppError> {
        let now = Utc::now();
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let document_derivatives = sqlx::query(
            "UPDATE resource_derivatives SET state = 'failed', \
                    failure_code = 'document_runtime_unavailable', updated_at = ? \
             WHERE state = 'processing' AND updated_at < ?",
        )
        .bind(now)
        .bind(cutoff)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        let documents = sqlx::query(
            "UPDATE managed_resources SET status = 'failed', \
                    failure_code = 'document_runtime_unavailable', updated_at = ? \
             WHERE status = 'processing' AND updated_at < ? \
               AND EXISTS (SELECT 1 FROM resource_derivatives \
                           WHERE resource_derivatives.resource_id = managed_resources.id)",
        )
        .bind(now)
        .bind(cutoff)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        let generic = sqlx::query(
            "UPDATE managed_resources SET status = 'failed', \
                    failure_code = 'import_interrupted', updated_at = ? \
             WHERE status IN ('staging', 'processing') AND updated_at < ? \
               AND NOT EXISTS (SELECT 1 FROM resource_derivatives \
                               WHERE resource_derivatives.resource_id = managed_resources.id)",
        )
        .bind(now)
        .bind(cutoff)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        transaction.commit().await?;
        let _ = document_derivatives;
        Ok(documents + generic)
    }

    pub async fn collect_abandoned_drafts(
        &self,
        cutoff: DateTime<Utc>,
    ) -> Result<Vec<StorageRef>, AppError> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query("DELETE FROM resource_links WHERE draft_id IS NOT NULL AND created_at < ?")
            .bind(cutoff)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "DELETE FROM managed_resources \
             WHERE NOT EXISTS (\
                 SELECT 1 FROM resource_links \
                 WHERE resource_links.resource_id = managed_resources.id\
             )",
        )
        .execute(&mut *transaction)
        .await?;
        let orphaned = sqlx::query_scalar::<_, String>(
            "SELECT blob_replicas.object_key \
             FROM blob_replicas \
             WHERE blob_replicas.store_id = ? \
               AND NOT EXISTS (\
                   SELECT 1 FROM managed_resources \
                   WHERE managed_resources.blob_id = blob_replicas.blob_id\
               )",
        )
        .bind(LOCAL_BLOB_STORE_ID)
        .fetch_all(&mut *transaction)
        .await?;
        sqlx::query(
            "DELETE FROM resource_blobs \
             WHERE NOT EXISTS (\
                 SELECT 1 FROM managed_resources \
                 WHERE managed_resources.blob_id = resource_blobs.id\
             )",
        )
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(orphaned
            .into_iter()
            .map(|object_key| StorageRef { object_key })
            .collect())
    }
}

#[derive(FromRow)]
struct ResourceRow {
    id: String,
    original_name: String,
    media_type: String,
    size: i64,
    origin: ResourceOrigin,
    status: ResourceStatus,
    failure_code: Option<String>,
    created_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct EngineResourceRow {
    id: String,
    original_name: String,
    media_type: String,
    size: i64,
    object_key: String,
}

impl TryFrom<EngineResourceRow> for EngineResource {
    type Error = AppError;

    fn try_from(row: EngineResourceRow) -> Result<Self, Self::Error> {
        let size = u64::try_from(row.size)
            .map_err(|error| AppError::Database(sqlx::Error::Decode(Box::new(error))))?;
        Ok(Self {
            id: row.id,
            original_name: row.original_name,
            media_type: row.media_type,
            size,
            object_key: row.object_key,
        })
    }
}

impl TryFrom<ResourceRow> for ResourceSummary {
    type Error = AppError;

    fn try_from(row: ResourceRow) -> Result<Self, Self::Error> {
        let size = u64::try_from(row.size)
            .map_err(|error| AppError::Database(sqlx::Error::Decode(Box::new(error))))?;
        Ok(Self {
            id: row.id,
            original_name: row.original_name,
            media_type: row.media_type,
            size,
            origin: row.origin,
            status: row.status,
            failure_code: row.failure_code,
            created_at: row.created_at,
        })
    }
}
