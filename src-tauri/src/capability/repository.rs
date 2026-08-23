use chrono::{DateTime, Utc};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::{
    domain::{agent::RoleKind, work::PermissionMode},
    error::AppError,
};

use super::{ApprovalRequest, CapabilityDecision, CapabilityOperation, RunCapabilitySnapshot};

#[derive(Clone)]
pub(crate) struct CapabilitySnapshotRepository {
    pool: SqlitePool,
}

impl CapabilitySnapshotRepository {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub(crate) async fn insert_or_get(
        &self,
        snapshot: RunCapabilitySnapshot,
    ) -> Result<RunCapabilitySnapshot, AppError> {
        let expert_pack_ids = serde_json::to_string(&snapshot.expert_pack_ids)
            .map_err(|error| AppError::engine(error.to_string()))?;
        let host_tool_ids = serde_json::to_string(&snapshot.host_tool_ids)
            .map_err(|error| AppError::engine(error.to_string()))?;
        let extension_tool_ids = serde_json::to_string(&snapshot.extension_tool_ids)
            .map_err(|error| AppError::engine(error.to_string()))?;

        sqlx::query(
            "INSERT INTO run_capability_snapshots (id, schema_version, run_id, work_id, assignment_id, agent_instance_id, role_kind, permission_mode, workspace_root, expert_pack_ids_json, host_tool_ids_json, extension_tool_ids_json, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(run_id) DO NOTHING",
        )
        .bind(&snapshot.id)
        .bind(snapshot.schema_version)
        .bind(&snapshot.run_id)
        .bind(&snapshot.work_id)
        .bind(&snapshot.assignment_id)
        .bind(&snapshot.agent_instance_id)
        .bind(snapshot.role_kind)
        .bind(snapshot.permission_mode)
        .bind(snapshot.workspace_root.to_string_lossy().as_ref())
        .bind(expert_pack_ids)
        .bind(host_tool_ids)
        .bind(extension_tool_ids)
        .bind(snapshot.created_at)
        .bind(snapshot.expires_at)
        .execute(&self.pool)
        .await?;

        self.get_by_run(&snapshot.run_id)
            .await?
            .ok_or_else(|| AppError::run_not_found(snapshot.run_id))
    }

    pub(crate) async fn get(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<RunCapabilitySnapshot>, AppError> {
        let row = sqlx::query_as::<_, SnapshotRow>(
            "SELECT id, schema_version, run_id, work_id, assignment_id, agent_instance_id, role_kind, permission_mode, workspace_root, expert_pack_ids_json, host_tool_ids_json, extension_tool_ids_json, created_at, expires_at, revoked_at FROM run_capability_snapshots WHERE id = ?",
        )
        .bind(snapshot_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(TryInto::try_into).transpose()
    }

    async fn get_by_run(&self, run_id: &str) -> Result<Option<RunCapabilitySnapshot>, AppError> {
        let row = sqlx::query_as::<_, SnapshotRow>(
            "SELECT id, schema_version, run_id, work_id, assignment_id, agent_instance_id, role_kind, permission_mode, workspace_root, expert_pack_ids_json, host_tool_ids_json, extension_tool_ids_json, created_at, expires_at, revoked_at FROM run_capability_snapshots WHERE run_id = ?",
        )
        .bind(run_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(TryInto::try_into).transpose()
    }

    pub(crate) async fn revoke(
        &self,
        snapshot_id: &str,
        now: DateTime<Utc>,
    ) -> Result<bool, AppError> {
        let result = sqlx::query(
            "UPDATE run_capability_snapshots SET revoked_at = COALESCE(revoked_at, ?) WHERE id = ?",
        )
        .bind(now)
        .bind(snapshot_id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub(crate) async fn record_decision(
        &self,
        snapshot: &RunCapabilitySnapshot,
        operation: &CapabilityOperation,
        decision: &CapabilityDecision,
        now: DateTime<Utc>,
    ) -> Result<(), AppError> {
        let operation_json = serde_json::to_string(operation)
            .map_err(|error| AppError::engine(error.to_string()))?;
        let (decision_id, decision_kind, denial_reason, approval_request_id) = match decision {
            CapabilityDecision::Allow { audit } => (audit.decision_id.clone(), "allow", None, None),
            CapabilityDecision::Deny { reason } => (
                Uuid::new_v4().to_string(),
                "deny",
                Some(
                    serde_json::to_value(reason)
                        .map_err(|error| AppError::engine(error.to_string()))?
                        .as_str()
                        .unwrap_or("unknown")
                        .to_owned(),
                ),
                None,
            ),
            CapabilityDecision::Ask { request } => (
                Uuid::new_v4().to_string(),
                "ask",
                None,
                Some(request.request_id.clone()),
            ),
        };

        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO capability_decisions (id, snapshot_id, operation_json, decision, denial_reason, approval_request_id, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&decision_id)
        .bind(&snapshot.id)
        .bind(&operation_json)
        .bind(decision_kind)
        .bind(denial_reason)
        .bind(&approval_request_id)
        .bind(now)
        .execute(&mut *transaction)
        .await?;

        if let Some(request_id) = approval_request_id {
            sqlx::query(
                "INSERT INTO capability_approval_requests (id, decision_id, snapshot_id, operation_json, status, created_at) VALUES (?, ?, ?, ?, 'pending', ?)",
            )
            .bind(request_id)
            .bind(decision_id)
            .bind(&snapshot.id)
            .bind(operation_json)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub(crate) async fn pending_approvals(
        &self,
        snapshot_id: &str,
    ) -> Result<Vec<ApprovalRequest>, AppError> {
        let rows = sqlx::query_as::<_, ApprovalRow>(
            "SELECT id, snapshot_id, operation_json FROM capability_approval_requests WHERE snapshot_id = ? AND status = 'pending' ORDER BY created_at, id",
        )
        .bind(snapshot_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    pub(crate) async fn begin_execution(
        &self,
        decision_id: &str,
        now: DateTime<Utc>,
    ) -> Result<String, AppError> {
        let execution_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO capability_executions (id, decision_id, status, started_at) SELECT ?, id, 'started', ? FROM capability_decisions WHERE id = ? AND decision = 'allow'",
        )
        .bind(&execution_id)
        .bind(now)
        .bind(decision_id)
        .execute(&self.pool)
        .await?
        .rows_affected()
        .eq(&1)
        .then_some(execution_id)
        .ok_or_else(|| {
            AppError::invalid_input(
                "decisionId",
                "execution requires a persisted allow decision",
            )
        })
    }

    pub(crate) async fn finish_execution(
        &self,
        execution_id: &str,
        succeeded: bool,
        now: DateTime<Utc>,
    ) -> Result<(), AppError> {
        let status = if succeeded { "succeeded" } else { "failed" };
        let updated = sqlx::query(
            "UPDATE capability_executions SET status = ?, completed_at = ? WHERE id = ? AND status = 'started'",
        )
        .bind(status)
        .bind(now)
        .bind(execution_id)
        .execute(&self.pool)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(AppError::invalid_input(
                "executionId",
                "capability execution is missing or already terminal",
            ));
        }
        Ok(())
    }
}

#[derive(FromRow)]
struct ApprovalRow {
    id: String,
    snapshot_id: String,
    operation_json: String,
}

impl TryFrom<ApprovalRow> for ApprovalRequest {
    type Error = AppError;

    fn try_from(row: ApprovalRow) -> Result<Self, Self::Error> {
        let operation = serde_json::from_str(&row.operation_json).map_err(|error| {
            AppError::invalid_input(
                "operation",
                format!("stored approval operation is invalid: {error}"),
            )
        })?;
        Ok(Self {
            request_id: row.id,
            snapshot_id: row.snapshot_id,
            operation,
        })
    }
}

#[derive(FromRow)]
struct SnapshotRow {
    id: String,
    schema_version: u32,
    run_id: String,
    work_id: String,
    assignment_id: String,
    agent_instance_id: String,
    role_kind: RoleKind,
    permission_mode: PermissionMode,
    workspace_root: String,
    expert_pack_ids_json: String,
    host_tool_ids_json: String,
    extension_tool_ids_json: String,
    created_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
}

impl TryFrom<SnapshotRow> for RunCapabilitySnapshot {
    type Error = AppError;

    fn try_from(row: SnapshotRow) -> Result<Self, Self::Error> {
        let decode = |field: &'static str, value: &str| {
            serde_json::from_str(value).map_err(|error| {
                AppError::invalid_input(field, format!("stored snapshot is invalid: {error}"))
            })
        };
        Ok(Self {
            id: row.id,
            schema_version: row.schema_version,
            run_id: row.run_id,
            work_id: row.work_id,
            assignment_id: row.assignment_id,
            agent_instance_id: row.agent_instance_id,
            role_kind: row.role_kind,
            permission_mode: row.permission_mode,
            workspace_root: row.workspace_root.into(),
            expert_pack_ids: decode("expertPackIds", &row.expert_pack_ids_json)?,
            host_tool_ids: decode("hostToolIds", &row.host_tool_ids_json)?,
            extension_tool_ids: decode("extensionToolIds", &row.extension_tool_ids_json)?,
            created_at: row.created_at,
            expires_at: row.expires_at,
            revoked_at: row.revoked_at,
        })
    }
}
