mod policy;
mod repository;
mod snapshot;

pub use policy::{
    ApprovalRequest, AuditReceipt, CapabilityDecision, CapabilityOperation, DenialReason,
};
pub use snapshot::{RunCapabilityRequest, RunCapabilitySnapshot};

use chrono::Utc;
use sqlx::SqlitePool;

use crate::error::AppError;

use self::repository::CapabilitySnapshotRepository;

#[derive(Clone)]
pub struct CapabilityBroker {
    snapshots: CapabilitySnapshotRepository,
}

impl CapabilityBroker {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            snapshots: CapabilitySnapshotRepository::new(pool),
        }
    }

    pub async fn snapshot(
        &self,
        request: RunCapabilityRequest,
    ) -> Result<RunCapabilitySnapshot, AppError> {
        let snapshot = RunCapabilitySnapshot::compile(request)?;
        self.snapshots.insert_or_get(snapshot).await
    }

    pub fn authorize(
        &self,
        snapshot: &RunCapabilitySnapshot,
        operation: &CapabilityOperation,
    ) -> CapabilityDecision {
        policy::authorize_at(snapshot, operation, Utc::now())
    }

    pub async fn authorize_and_record(
        &self,
        snapshot: &RunCapabilitySnapshot,
        operation: &CapabilityOperation,
    ) -> Result<CapabilityDecision, AppError> {
        let now = Utc::now();
        let decision = policy::authorize_at(snapshot, operation, now);
        self.snapshots
            .record_decision(snapshot, operation, &decision, now)
            .await?;
        Ok(decision)
    }

    pub async fn pending_approvals(
        &self,
        snapshot_id: &str,
    ) -> Result<Vec<ApprovalRequest>, AppError> {
        self.snapshots.pending_approvals(snapshot_id).await
    }

    pub async fn begin_execution(&self, audit: &AuditReceipt) -> Result<String, AppError> {
        self.snapshots
            .begin_execution(&audit.decision_id, Utc::now())
            .await
    }

    pub async fn finish_execution(
        &self,
        execution_id: &str,
        succeeded: bool,
    ) -> Result<(), AppError> {
        self.snapshots
            .finish_execution(execution_id, succeeded, Utc::now())
            .await
    }

    pub async fn inspect(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<RunCapabilitySnapshot>, AppError> {
        self.snapshots.get(snapshot_id).await
    }

    pub async fn revoke(&self, snapshot_id: &str) -> Result<bool, AppError> {
        self.snapshots.revoke(snapshot_id, Utc::now()).await
    }
}
