//! Characterization: CapabilityBroker policy-layer Permission Mode differences (#18).
//!
//! Public seam: `CapabilityBroker::authorize` (and snapshot compile/revoke).
//! Module seam for `policy::authorize_at` lives next to that function.
//! These tests do not start a real engine or Pi sidecar.

use chrono::{Duration, Utc};
use piwork_lib::{
    capability::{
        CapabilityBroker, CapabilityDecision, CapabilityOperation, DenialReason,
        RunCapabilityRequest,
    },
    domain::{agent::RoleKind, work::PermissionMode},
    storage::sqlite::Database,
};

async fn seed_run(pool: &sqlx::SqlitePool, root: &str) {
    let now = Utc::now();
    sqlx::query("INSERT INTO works (id, title, goal, root_path, permission_mode, status, created_at, updated_at) VALUES ('work-cap', 'Capability', 'Deny by default', ?, 'balanced', 'running', ?, ?)")
        .bind(root).bind(now).bind(now).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO work_agents (work_id, agent_instance_id, role_kind, status, permission_policy, joined_at, updated_at) VALUES ('work-cap', 'agent-instance:piwork-lead', 'lead', 'joined', 'inherit_work', ?, ?)")
        .bind(now).bind(now).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO work_leads (work_id, agent_instance_id, created_at) VALUES ('work-cap', 'agent-instance:piwork-lead', ?)")
        .bind(now).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO assignments (id, work_id, assigned_agent_id, kind, side_effect, title, instruction, context_manifest_json, expected_result_schema_json, acceptance_criteria_json, permission_scope_json, priority, status, attempt_count, max_attempts, created_at, updated_at) VALUES ('assignment-cap', 'work-cap', 'agent-instance:piwork-lead', 'lead', 'unknown', 'Capability', 'Test', '{}', '{}', '[]', '{\"mode\":\"inherit_work\"}', 10, 'running', 1, 3, ?, ?)")
        .bind(now).bind(now).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO runs (id, work_id, engine_kind, model_label, status, created_at, updated_at, assignment_id, agent_instance_id, attempt_number) VALUES ('run-cap', 'work-cap', 'fake', 'fake', 'running', ?, ?, 'assignment-cap', 'agent-instance:piwork-lead', 1)")
        .bind(now).bind(now).execute(pool).await.unwrap();
}

#[tokio::test]
async fn snapshot_is_immutable_and_unknown_or_expired_operations_deny() {
    let root = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    seed_run(database.pool(), &root.path().to_string_lossy()).await;
    let broker = CapabilityBroker::new(database.pool().clone());
    let snapshot = broker
        .snapshot(RunCapabilityRequest {
            run_id: "run-cap".into(),
            work_id: "work-cap".into(),
            assignment_id: "assignment-cap".into(),
            agent_instance_id: "agent-instance:piwork-lead".into(),
            role_kind: RoleKind::Lead,
            permission_mode: PermissionMode::Balanced,
            workspace_root: root.path().to_path_buf(),
            expert_pack_ids: vec![],
            host_tool_ids: vec!["get_assignment_status".into()],
            extension_tool_ids: vec![],
            expires_at: Some(Utc::now() + Duration::minutes(5)),
        })
        .await
        .unwrap();

    let allowed = broker
        .authorize_and_record(
            &snapshot,
            &CapabilityOperation::HostTool {
                tool_id: "get_assignment_status".into(),
            },
        )
        .await
        .unwrap();
    let CapabilityDecision::Allow { audit } = allowed else {
        panic!("granted host tool should be allowed");
    };
    let execution_id = broker.begin_execution(&audit).await.unwrap();
    broker.finish_execution(&execution_id, true).await.unwrap();
    let execution_status: String =
        sqlx::query_scalar("SELECT status FROM capability_executions WHERE decision_id = ?")
            .bind(&audit.decision_id)
            .fetch_one(database.pool())
            .await
            .unwrap();
    assert_eq!(execution_status, "succeeded");
    assert!(matches!(
        broker.authorize(
            &snapshot,
            &CapabilityOperation::HostTool {
                tool_id: "invented_tool".into()
            }
        ),
        CapabilityDecision::Deny {
            reason: DenialReason::NotGranted
        }
    ));

    broker.revoke(&snapshot.id).await.unwrap();
    let revoked = broker.inspect(&snapshot.id).await.unwrap().unwrap();
    assert!(matches!(
        broker.authorize(
            &revoked,
            &CapabilityOperation::HostTool {
                tool_id: "get_assignment_status".into()
            }
        ),
        CapabilityDecision::Deny {
            reason: DenialReason::Revoked
        }
    ));
}

#[tokio::test]
async fn balanced_asks_for_workspace_writes_and_denies_escape() {
    let root = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    seed_run(database.pool(), &root.path().to_string_lossy()).await;
    let broker = CapabilityBroker::new(database.pool().clone());
    let snapshot = broker
        .snapshot(RunCapabilityRequest {
            run_id: "run-cap".into(),
            work_id: "work-cap".into(),
            assignment_id: "assignment-cap".into(),
            agent_instance_id: "agent-instance:piwork-lead".into(),
            role_kind: RoleKind::Lead,
            permission_mode: PermissionMode::Balanced,
            workspace_root: root.path().to_path_buf(),
            expert_pack_ids: vec![],
            host_tool_ids: vec![],
            extension_tool_ids: vec![],
            expires_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        broker.authorize(
            &snapshot,
            &CapabilityOperation::FilesystemWrite {
                path: root.path().join("new.txt")
            }
        ),
        CapabilityDecision::Ask { .. }
    ));
    assert!(matches!(
        broker.authorize(
            &snapshot,
            &CapabilityOperation::FilesystemWrite {
                path: root.path().join("../escape.txt")
            }
        ),
        CapabilityDecision::Deny {
            reason: DenialReason::OutsideWorkspace
        }
    ));

    let operation = CapabilityOperation::FilesystemWrite {
        path: root.path().join("persisted.txt"),
    };
    let decision = broker
        .authorize_and_record(&snapshot, &operation)
        .await
        .unwrap();
    let CapabilityDecision::Ask { request } = decision else {
        panic!("balanced workspace write should require approval");
    };

    let restarted_broker = CapabilityBroker::new(database.pool().clone());
    let pending = restarted_broker
        .pending_approvals(&snapshot.id)
        .await
        .unwrap();
    assert_eq!(pending, vec![request]);
}

#[tokio::test]
async fn authorize_locks_policy_layer_mode_differences_without_an_engine() {
    let root = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    seed_run(database.pool(), &root.path().to_string_lossy()).await;
    let broker = CapabilityBroker::new(database.pool().clone());
    let balanced = broker
        .snapshot(RunCapabilityRequest {
            run_id: "run-cap".into(),
            work_id: "work-cap".into(),
            assignment_id: "assignment-cap".into(),
            agent_instance_id: "agent-instance:piwork-lead".into(),
            role_kind: RoleKind::Lead,
            permission_mode: PermissionMode::Balanced,
            workspace_root: root.path().to_path_buf(),
            expert_pack_ids: vec![],
            host_tool_ids: vec![],
            extension_tool_ids: vec![],
            expires_at: None,
        })
        .await
        .unwrap();
    let mut auto_execute = balanced.clone();
    auto_execute.permission_mode = PermissionMode::AutoExecute;

    let write = CapabilityOperation::FilesystemWrite {
        path: root.path().join("new.txt"),
    };
    assert!(
        matches!(
            broker.authorize(&balanced, &write),
            CapabilityDecision::Ask { .. }
        ),
        "Balanced must ask before the same workspace write"
    );
    assert!(
        matches!(
            broker.authorize(&auto_execute, &write),
            CapabilityDecision::Allow { .. }
        ),
        "AutoExecute must allow the same workspace write"
    );

    let escaped = root.path().join("../escape.txt");
    assert!(matches!(
        broker.authorize(
            &balanced,
            &CapabilityOperation::FilesystemWrite {
                path: escaped.clone()
            }
        ),
        CapabilityDecision::Deny {
            reason: DenialReason::OutsideWorkspace
        }
    ));
    assert!(matches!(
        broker.authorize(
            &balanced,
            &CapabilityOperation::FilesystemRead { path: escaped }
        ),
        CapabilityDecision::Deny {
            reason: DenialReason::OutsideWorkspace
        }
    ));

    for snapshot in [&balanced, &auto_execute] {
        assert!(
            matches!(
                broker.authorize(
                    snapshot,
                    &CapabilityOperation::Secret {
                        secret_id: "secret".into()
                    }
                ),
                CapabilityDecision::Ask { .. }
            ),
            "{:?} must ask before Secret",
            snapshot.permission_mode
        );
        assert!(
            matches!(
                broker.authorize(
                    snapshot,
                    &CapabilityOperation::Publish {
                        target: "channel".into()
                    }
                ),
                CapabilityDecision::Ask { .. }
            ),
            "{:?} must ask before Publish",
            snapshot.permission_mode
        );
        assert!(
            matches!(
                broker.authorize(
                    snapshot,
                    &CapabilityOperation::Unknown {
                        name: "invented".into()
                    }
                ),
                CapabilityDecision::Deny {
                    reason: DenialReason::UnknownOperation
                }
            ),
            "{:?} must deny unknown operations",
            snapshot.permission_mode
        );
    }

    let mut expired = auto_execute.clone();
    expired.expires_at = Some(Utc::now() - Duration::seconds(1));
    assert!(
        matches!(
            broker.authorize(&expired, &write),
            CapabilityDecision::Deny {
                reason: DenialReason::Expired
            }
        ),
        "expired Run Capability Snapshot must be rejected before AutoExecute write policy"
    );

    broker.revoke(&balanced.id).await.unwrap();
    let revoked = broker.inspect(&balanced.id).await.unwrap().unwrap();
    assert!(
        matches!(
            broker.authorize(&revoked, &write),
            CapabilityDecision::Deny {
                reason: DenialReason::Revoked
            }
        ),
        "revoked Run Capability Snapshot must be rejected before write policy"
    );
}
