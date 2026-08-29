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
        snapshot_with(root, PermissionMode::Balanced)
    }

    fn snapshot_with(root: PathBuf, permission_mode: PermissionMode) -> RunCapabilitySnapshot {
        RunCapabilitySnapshot {
            id: "snapshot".into(),
            schema_version: RUN_CAPABILITY_SCHEMA_VERSION,
            run_id: "run".into(),
            work_id: "work".into(),
            assignment_id: "assignment".into(),
            agent_instance_id: "agent".into(),
            role_kind: RoleKind::Lead,
            permission_mode,
            workspace_root: root,
            expert_pack_ids: vec![],
            host_tool_ids: vec![],
            extension_tool_ids: vec![],
            created_at: Utc::now(),
            expires_at: None,
            revoked_at: None,
        }
    }

    fn workspace_write(root: &Path) -> CapabilityOperation {
        CapabilityOperation::FilesystemWrite {
            path: root.join("new.txt"),
        }
    }

    fn outside_workspace(root: &Path) -> PathBuf {
        root.join("../escape.txt")
    }

    /// Characterization (#18): the same mutating operation is Ask in Balanced
    /// and Allow in AutoExecute at the policy layer.
    #[test]
    fn same_mutating_write_asks_in_balanced_and_allows_in_auto_execute() {
        let root = std::env::current_dir().unwrap();
        let now = Utc::now();
        let operation = workspace_write(&root);

        assert!(
            matches!(
                authorize_at(
                    &snapshot_with(root.clone(), PermissionMode::AskEveryStep),
                    &operation,
                    now
                ),
                CapabilityDecision::Ask { .. }
            ),
            "AskEveryStep must ask before a workspace write"
        );
        assert!(
            matches!(
                authorize_at(
                    &snapshot_with(root.clone(), PermissionMode::Balanced),
                    &operation,
                    now
                ),
                CapabilityDecision::Ask { .. }
            ),
            "Balanced must ask before a workspace write"
        );
        assert!(
            matches!(
                authorize_at(
                    &snapshot_with(root, PermissionMode::AutoExecute),
                    &operation,
                    now
                ),
                CapabilityDecision::Allow { .. }
            ),
            "AutoExecute must allow a workspace write"
        );
    }

    /// Characterization (#18): FilesystemWrite and FilesystemRead outside the
    /// Workspace fail closed with OutsideWorkspace.
    #[test]
    fn filesystem_write_and_read_outside_workspace_deny_fail_closed() {
        let root = std::env::current_dir().unwrap();
        let now = Utc::now();
        let escaped = outside_workspace(&root);
        let snapshot = snapshot(root);

        assert_eq!(
            authorize_at(
                &snapshot,
                &CapabilityOperation::FilesystemWrite {
                    path: escaped.clone()
                },
                now
            ),
            CapabilityDecision::Deny {
                reason: DenialReason::OutsideWorkspace
            }
        );
        assert_eq!(
            authorize_at(
                &snapshot,
                &CapabilityOperation::FilesystemRead { path: escaped },
                now
            ),
            CapabilityDecision::Deny {
                reason: DenialReason::OutsideWorkspace
            }
        );
    }

    /// Characterization (#18): an expired Run Capability Snapshot is rejected
    /// before operation policy runs. AutoExecute + in-bounds write would Allow
    /// if evaluation were reached.
    #[test]
    fn expired_snapshot_denies_before_policy_evaluation() {
        let root = std::env::current_dir().unwrap();
        let mut snapshot = snapshot_with(root.clone(), PermissionMode::AutoExecute);
        snapshot.expires_at = Some(Utc::now() - Duration::seconds(1));
        assert_eq!(
            authorize_at(&snapshot, &workspace_write(&root), Utc::now()),
            CapabilityDecision::Deny {
                reason: DenialReason::Expired
            }
        );
    }

    /// Characterization (#18): a revoked Run Capability Snapshot is rejected
    /// before operation policy runs. AutoExecute + in-bounds write would Allow
    /// if evaluation were reached.
    #[test]
    fn revoked_snapshot_denies_before_policy_evaluation() {
        let root = std::env::current_dir().unwrap();
        let mut snapshot = snapshot_with(root.clone(), PermissionMode::AutoExecute);
        snapshot.revoked_at = Some(Utc::now());
        assert_eq!(
            authorize_at(&snapshot, &workspace_write(&root), Utc::now()),
            CapabilityDecision::Deny {
                reason: DenialReason::Revoked
            }
        );
    }

    /// Characterization (#18): Secret and Publish stay Ask in every Permission
    /// Mode; unknown operations stay Deny.
    #[test]
    fn secret_and_publish_always_ask_unknown_always_deny() {
        let root = std::env::current_dir().unwrap();
        let now = Utc::now();
        let secret = CapabilityOperation::Secret {
            secret_id: "secret".into(),
        };
        let publish = CapabilityOperation::Publish {
            target: "channel".into(),
        };
        let unknown = CapabilityOperation::Unknown {
            name: "invented".into(),
        };

        for mode in [
            PermissionMode::AskEveryStep,
            PermissionMode::Balanced,
            PermissionMode::AutoExecute,
        ] {
            let snapshot = snapshot_with(root.clone(), mode);
            assert!(
                matches!(
                    authorize_at(&snapshot, &secret, now),
                    CapabilityDecision::Ask { .. }
                ),
                "{mode:?} must ask before Secret"
            );
            assert!(
                matches!(
                    authorize_at(&snapshot, &publish, now),
                    CapabilityDecision::Ask { .. }
                ),
                "{mode:?} must ask before Publish"
            );
            assert_eq!(
                authorize_at(&snapshot, &unknown, now),
                CapabilityDecision::Deny {
                    reason: DenialReason::UnknownOperation
                },
                "{mode:?} must deny unknown operations"
            );
        }
    }
}
