use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::{domain::work::PermissionMode, error::AppError};

use super::path_identity::WorkspacePathIdentity;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSummary {
    pub id: String,
    pub canonical_root_path: String,
    pub path_identity: String,
    pub default_permission_mode: PermissionMode,
    pub lifecycle_status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct WorkspaceRepository {
    pool: SqlitePool,
}

impl WorkspaceRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn resolve_or_create(
        &self,
        root: &std::path::Path,
        permission_mode: PermissionMode,
    ) -> Result<WorkspaceSummary, AppError> {
        let resolved = WorkspacePathIdentity::resolve(root)?;
        let now = Utc::now();
        sqlx::query("INSERT INTO workspaces (id, canonical_root_path, path_identity, default_permission_mode, lifecycle_status, created_at, updated_at) VALUES (?, ?, ?, ?, 'active', ?, ?) ON CONFLICT(path_identity) DO NOTHING")
            .bind(Uuid::new_v4().to_string())
            .bind(resolved.canonical_root.to_string_lossy().as_ref())
            .bind(&resolved.identity)
            .bind(permission_mode)
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await?;
        sqlx::query_as::<_, WorkspaceSummary>("SELECT id, canonical_root_path, path_identity, default_permission_mode, lifecycle_status, created_at, updated_at FROM workspaces WHERE path_identity = ?")
            .bind(resolved.identity)
            .fetch_one(&self.pool)
            .await
            .map_err(Into::into)
    }

    pub async fn get(&self, id: &str) -> Result<Option<WorkspaceSummary>, AppError> {
        sqlx::query_as::<_, WorkspaceSummary>("SELECT id, canonical_root_path, path_identity, default_permission_mode, lifecycle_status, created_at, updated_at FROM workspaces WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(Into::into)
    }

    pub async fn reconcile_legacy_paths(&self) -> Result<u64, AppError> {
        let workspaces = sqlx::query_as::<_, (String, String, PermissionMode)>(
            "SELECT id, canonical_root_path, default_permission_mode FROM workspaces ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut reconciled = 0u64;
        for (source_id, root, permission_mode) in workspaces {
            let resolved = match WorkspacePathIdentity::resolve(std::path::Path::new(&root)) {
                Ok(resolved) => resolved,
                Err(_) => {
                    sqlx::query("UPDATE workspaces SET lifecycle_status = 'unavailable', updated_at = ? WHERE id = ?")
                        .bind(Utc::now()).bind(&source_id).execute(&self.pool).await?;
                    continue;
                }
            };
            let source_identity: String =
                sqlx::query_scalar("SELECT path_identity FROM workspaces WHERE id = ?")
                    .bind(&source_id)
                    .fetch_one(&self.pool)
                    .await?;
            if source_identity == resolved.identity {
                sqlx::query("UPDATE workspaces SET canonical_root_path = ?, lifecycle_status = 'active', updated_at = ? WHERE id = ?")
                    .bind(resolved.canonical_root.to_string_lossy().as_ref()).bind(Utc::now()).bind(&source_id)
                    .execute(&self.pool).await?;
                continue;
            }

            let target = self
                .resolve_or_create(&resolved.canonical_root, permission_mode)
                .await?;
            let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
            sqlx::query("INSERT OR IGNORE INTO workspace_memory_bindings (workspace_id, task_id, source_root_path, created_at, updated_at) SELECT ?, task_id, source_root_path, created_at, updated_at FROM workspace_memory_bindings WHERE workspace_id = ?")
                .bind(&target.id).bind(&source_id).execute(&mut *transaction).await?;
            sqlx::query("INSERT INTO extension_workspace_policies (package_id, workspace_id, enabled, tool_allowlist_json, has_conflict, updated_at) SELECT package_id, ?, enabled, tool_allowlist_json, has_conflict, updated_at FROM extension_workspace_policies WHERE workspace_id = ? ON CONFLICT(package_id, workspace_id) DO UPDATE SET enabled = 0, tool_allowlist_json = '[]', has_conflict = 1, updated_at = excluded.updated_at")
                .bind(&target.id).bind(&source_id).execute(&mut *transaction).await?;
            sqlx::query("INSERT INTO connector_workspace_grants (connection_id, workspace_id, permissions_json, has_conflict, created_at, updated_at) SELECT connection_id, ?, permissions_json, has_conflict, created_at, updated_at FROM connector_workspace_grants WHERE workspace_id = ? ON CONFLICT(connection_id, workspace_id) DO UPDATE SET permissions_json = '{}', has_conflict = 1, updated_at = excluded.updated_at")
                .bind(&target.id).bind(&source_id).execute(&mut *transaction).await?;
            sqlx::query("UPDATE works SET workspace_id = ? WHERE workspace_id = ?")
                .bind(&target.id)
                .bind(&source_id)
                .execute(&mut *transaction)
                .await?;
            sqlx::query("DELETE FROM workspaces WHERE id = ?")
                .bind(&source_id)
                .execute(&mut *transaction)
                .await?;
            transaction.commit().await?;
            reconciled = reconciled.saturating_add(1);
        }
        Ok(reconciled)
    }
}
