use std::{path::Component, path::Path, path::PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::work::PermissionMode;

use super::snapshot::{RUN_CAPABILITY_SCHEMA_VERSION, RunCapabilitySnapshot};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CapabilityOperation {
    FilesystemRead { path: PathBuf },
    FilesystemWrite { path: PathBuf },
    Process { program: String },
    Network { destination: String },
    Browser { action: String },
    ConnectorRead { connector_id: String },
    ConnectorWrite { connector_id: String },
    Secret { secret_id: String },
    Publish { target: String },
    HostTool { tool_id: String },
    ExtensionTool { tool_id: String },
    Unknown { name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DenialReason {
    InvalidSnapshot,
    Expired,
    Revoked,
    NotGranted,
    OutsideWorkspace,
    UnknownOperation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditReceipt {
    pub decision_id: String,
    pub snapshot_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub request_id: String,
    pub snapshot_id: String,
    pub operation: CapabilityOperation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum CapabilityDecision {
    Allow { audit: AuditReceipt },
    Deny { reason: DenialReason },
    Ask { request: ApprovalRequest },
}

pub(crate) fn authorize_at(
    snapshot: &RunCapabilitySnapshot,
    operation: &CapabilityOperation,
    now: DateTime<Utc>,
) -> CapabilityDecision {
    if snapshot.schema_version != RUN_CAPABILITY_SCHEMA_VERSION {
        return deny(DenialReason::InvalidSnapshot);
    }
    if snapshot.revoked_at.is_some() {
        return deny(DenialReason::Revoked);
    }
    if snapshot
        .expires_at
        .is_some_and(|expires_at| expires_at <= now)
    {
        return deny(DenialReason::Expired);
    }

    match operation {
        CapabilityOperation::FilesystemRead { path } => {
            if inside_workspace(&snapshot.workspace_root, path) {
                allow(snapshot)
            } else {
                deny(DenialReason::OutsideWorkspace)
            }
        }
        CapabilityOperation::FilesystemWrite { path } => {
            if !inside_workspace(&snapshot.workspace_root, path) {
                deny(DenialReason::OutsideWorkspace)
            } else {
                by_mode(snapshot, operation, true)
            }
        }
        CapabilityOperation::HostTool { tool_id } => {
            exact_grant(snapshot, &snapshot.host_tool_ids, tool_id)
        }
        CapabilityOperation::ExtensionTool { tool_id } => {
            exact_grant(snapshot, &snapshot.extension_tool_ids, tool_id)
        }
        CapabilityOperation::Process { .. }
        | CapabilityOperation::Network { .. }
        | CapabilityOperation::Browser { .. }
        | CapabilityOperation::ConnectorWrite { .. } => by_mode(snapshot, operation, true),
        CapabilityOperation::ConnectorRead { connector_id } => {
            exact_grant(snapshot, &snapshot.extension_tool_ids, connector_id)
        }
        CapabilityOperation::Secret { .. } | CapabilityOperation::Publish { .. } => {
            ask(snapshot, operation)
        }
        CapabilityOperation::Unknown { .. } => deny(DenialReason::UnknownOperation),
    }
}

fn exact_grant(
    snapshot: &RunCapabilitySnapshot,
    grants: &[String],
    requested: &str,
) -> CapabilityDecision {
    if grants
        .binary_search_by(|grant| grant.as_str().cmp(requested))
        .is_ok()
    {
        allow(snapshot)
    } else {
        deny(DenialReason::NotGranted)
    }
}

fn by_mode(
    snapshot: &RunCapabilitySnapshot,
    operation: &CapabilityOperation,
    auto_allowed: bool,
) -> CapabilityDecision {
    if auto_allowed && snapshot.permission_mode == PermissionMode::AutoExecute {
        allow(snapshot)
    } else {
        ask(snapshot, operation)
    }
}

fn allow(snapshot: &RunCapabilitySnapshot) -> CapabilityDecision {
    CapabilityDecision::Allow {
        audit: AuditReceipt {
            decision_id: Uuid::new_v4().to_string(),
            snapshot_id: snapshot.id.clone(),
        },
    }
}

fn ask(snapshot: &RunCapabilitySnapshot, operation: &CapabilityOperation) -> CapabilityDecision {
    CapabilityDecision::Ask {
        request: ApprovalRequest {
            request_id: Uuid::new_v4().to_string(),
            snapshot_id: snapshot.id.clone(),
            operation: operation.clone(),
        },
    }
}

fn deny(reason: DenialReason) -> CapabilityDecision {
    CapabilityDecision::Deny { reason }
}

fn inside_workspace(root: &Path, requested: &Path) -> bool {
    if !root.is_absolute()
        || !requested.is_absolute()
        || requested
            .components()
            .any(|component| component == Component::ParentDir)
    {
        return false;
    }

    if requested.exists() {
        return dunce::canonicalize(requested).is_ok_and(|canonical| canonical.starts_with(root));
    }

    requested.starts_with(root)
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use crate::domain::agent::RoleKind;

    use super::*;

    fn snapshot(root: PathBuf) -> RunCapabilitySnapshot {
        RunCapabilitySnapshot {
            id: "snapshot".into(),
            schema_version: RUN_CAPABILITY_SCHEMA_VERSION,
            run_id: "run".into(),
            work_id: "work".into(),
            assignment_id: "assignment".into(),
            agent_instance_id: "agent".into(),
            role_kind: RoleKind::Lead,
            permission_mode: PermissionMode::Balanced,
            workspace_root: root,
            expert_pack_ids: vec![],
            host_tool_ids: vec![],
            extension_tool_ids: vec![],
            created_at: Utc::now(),
            expires_at: None,
            revoked_at: None,
        }
    }

    #[test]
    fn expired_snapshot_denies_before_policy_evaluation() {
        let root = std::env::current_dir().unwrap();
        let mut snapshot = snapshot(root.clone());
        snapshot.expires_at = Some(Utc::now() - Duration::seconds(1));
        assert_eq!(
            authorize_at(
                &snapshot,
                &CapabilityOperation::FilesystemRead { path: root },
                Utc::now()
            ),
            CapabilityDecision::Deny {
                reason: DenialReason::Expired
            }
        );
    }
}
