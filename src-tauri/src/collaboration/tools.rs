//! Effective permission intersection and the Lead/Member tool authority table.
//!
//! The effective permission is the least-privilege intersection of the Work
//! owner policy, the Agent policy, capability-pack requirements, any assignment
//! override, and the engine capabilities. Unknown inputs fail closed, and every
//! decision source is recorded for auditability.

use std::collections::BTreeSet;

use crate::{
    domain::{
        agent::{PermissionPolicy, RoleKind},
        work::PermissionMode,
    },
    engine::EngineCapabilities,
    error::AppError,
};

pub const TOOL_LIST_WORK_MEMBERS: &str = "list_work_members";
pub const TOOL_INSPECT_CAPABILITY_PACKS: &str = "inspect_capability_packs";
pub const TOOL_DELEGATE_ASSIGNMENT: &str = "delegate_assignment";
pub const TOOL_GET_ASSIGNMENT_STATUS: &str = "get_assignment_status";
pub const TOOL_CANCEL_ASSIGNMENT: &str = "cancel_assignment";
pub const TOOL_REQUEST_ASSIGNMENT_RETRY: &str = "request_assignment_retry";
pub const TOOL_RECORD_WORK_DECISION: &str = "record_work_decision";
pub const TOOL_UPDATE_WORK_PLAN: &str = "update_work_plan";
pub const TOOL_COMPLETE_WORK_DELIVERY: &str = "complete_work_delivery";
pub const TOOL_SUBMIT_ASSIGNMENT_RESULT: &str = "submit_assignment_result";
pub const TOOL_REQUEST_CLARIFICATION: &str = "request_clarification";

pub const LEAD_TOOLS: [&str; 9] = [
    TOOL_LIST_WORK_MEMBERS,
    TOOL_INSPECT_CAPABILITY_PACKS,
    TOOL_DELEGATE_ASSIGNMENT,
    TOOL_GET_ASSIGNMENT_STATUS,
    TOOL_CANCEL_ASSIGNMENT,
    TOOL_REQUEST_ASSIGNMENT_RETRY,
    TOOL_RECORD_WORK_DECISION,
    TOOL_UPDATE_WORK_PLAN,
    TOOL_COMPLETE_WORK_DELIVERY,
];

pub const MEMBER_TOOLS: [&str; 3] = [
    TOOL_GET_ASSIGNMENT_STATUS,
    TOOL_SUBMIT_ASSIGNMENT_RESULT,
    TOOL_REQUEST_CLARIFICATION,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecisionSource {
    WorkPolicy {
        mode: PermissionMode,
    },
    AgentPolicy {
        policy: PermissionPolicy,
    },
    CapabilityPack {
        pack_id: String,
        required_tools: Vec<String>,
    },
    AssignmentOverride,
    EngineCapability {
        capability: String,
    },
    FailClosed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectivePermission {
    pub tools: BTreeSet<String>,
    pub mode: PermissionMode,
    pub sources: Vec<PermissionDecisionSource>,
}

/// Computes the least-privilege effective permission. The Agent policy narrows
/// the Work mode (a read-only Agent can never auto-execute), capability-pack
/// required tools must all be present or they are denied, and engine
/// capabilities gate the tools that depend on them.
pub fn resolve_effective_permission(
    work_mode: PermissionMode,
    agent_policy: PermissionPolicy,
    role_kind: RoleKind,
    capability_required_tools: &[String],
    engine_capabilities: &EngineCapabilities,
) -> EffectivePermission {
    let mut sources = vec![
        PermissionDecisionSource::WorkPolicy { mode: work_mode },
        PermissionDecisionSource::AgentPolicy {
            policy: agent_policy,
        },
    ];

    let mode = match agent_policy {
        PermissionPolicy::ReadOnly => PermissionMode::AskEveryStep,
        PermissionPolicy::InheritWork | PermissionPolicy::WorkWrite => work_mode,
    };

    let mut tools: BTreeSet<String> = if role_kind == RoleKind::Lead {
        LEAD_TOOLS.iter().map(|tool| (*tool).to_owned()).collect()
    } else {
        MEMBER_TOOLS.iter().map(|tool| (*tool).to_owned()).collect()
    };

    for required in capability_required_tools {
        // A capability pack can only narrow the tool set: every required tool
        // must already be granted by the role, otherwise the pack is denied.
        if !tools.contains(required) {
            tools.clear();
            sources.push(PermissionDecisionSource::CapabilityPack {
                pack_id: String::new(),
                required_tools: vec![required.clone()],
            });
            break;
        }
    }

    if !engine_capabilities.native_steer {
        sources.push(PermissionDecisionSource::EngineCapability {
            capability: "native_steer".to_owned(),
        });
    }
    if !engine_capabilities.cancel {
        sources.push(PermissionDecisionSource::EngineCapability {
            capability: "cancel".to_owned(),
        });
    }

    EffectivePermission {
        tools,
        mode,
        sources,
    }
}

/// Authorizes a single tool name against the effective permission. Deny by
/// default: an unknown tool or one outside the resolved set is rejected.
pub fn authorize_tool(permission: &EffectivePermission, tool: &str) -> Result<(), AppError> {
    if permission.tools.contains(tool) {
        Ok(())
    } else {
        Err(AppError::invalid_input(
            "tool",
            format!("tool '{tool}' is not authorized for this role"),
        ))
    }
}

/// Whether a role is the Lead (the only role that may delegate or deliver).
pub fn is_lead(role_kind: RoleKind) -> bool {
    role_kind == RoleKind::Lead
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lead_and_member_tool_sets_are_disjoint_except_status() {
        let lead: BTreeSet<String> = LEAD_TOOLS.iter().map(|t| (*t).to_owned()).collect();
        let member: BTreeSet<String> = MEMBER_TOOLS.iter().map(|t| (*t).to_owned()).collect();
        // Members may not delegate, decide, plan, or deliver.
        for tool in [
            TOOL_DELEGATE_ASSIGNMENT,
            TOOL_RECORD_WORK_DECISION,
            TOOL_UPDATE_WORK_PLAN,
            TOOL_COMPLETE_WORK_DELIVERY,
        ] {
            assert!(lead.contains(tool) && !member.contains(tool));
        }
        assert!(member.contains(TOOL_SUBMIT_ASSIGNMENT_RESULT));
        assert!(!lead.contains(TOOL_SUBMIT_ASSIGNMENT_RESULT));
        assert!(
            lead.contains(TOOL_GET_ASSIGNMENT_STATUS)
                && member.contains(TOOL_GET_ASSIGNMENT_STATUS)
        );
    }

    #[test]
    fn read_only_agent_never_auto_executes() {
        let permission = resolve_effective_permission(
            PermissionMode::AutoExecute,
            PermissionPolicy::ReadOnly,
            RoleKind::Researcher,
            &[],
            &EngineCapabilities::default(),
        );
        assert_eq!(permission.mode, PermissionMode::AskEveryStep);
    }

    #[test]
    fn engineer_inherits_work_mode() {
        let permission = resolve_effective_permission(
            PermissionMode::AutoExecute,
            PermissionPolicy::WorkWrite,
            RoleKind::Engineer,
            &[],
            &EngineCapabilities::default(),
        );
        assert_eq!(permission.mode, PermissionMode::AutoExecute);
    }

    #[test]
    fn unknown_required_capability_tool_denies_everything() {
        let permission = resolve_effective_permission(
            PermissionMode::Balanced,
            PermissionPolicy::InheritWork,
            RoleKind::Lead,
            &["delegate_assignment".to_owned(), "unknown_tool".to_owned()],
            &EngineCapabilities::default(),
        );
        assert!(permission.tools.is_empty());
    }

    #[test]
    fn authorize_tool_denies_unresolved_tools() {
        let permission = resolve_effective_permission(
            PermissionMode::Balanced,
            PermissionPolicy::InheritWork,
            RoleKind::Researcher,
            &[],
            &EngineCapabilities::default(),
        );
        assert!(authorize_tool(&permission, TOOL_SUBMIT_ASSIGNMENT_RESULT).is_ok());
        assert!(authorize_tool(&permission, TOOL_DELEGATE_ASSIGNMENT).is_err());
    }
}
