use piwork_lib::{
    collaboration::tools::{
        authorize_tool, resolve_effective_permission, TOOL_COMPLETE_WORK_DELIVERY,
        TOOL_DELEGATE_ASSIGNMENT, TOOL_SUBMIT_ASSIGNMENT_RESULT,
    },
    domain::{
        agent::{PermissionPolicy, RoleKind},
        work::PermissionMode,
    },
    engine::EngineCapabilities,
};

#[test]
fn authorization_lead_can_delegate_but_member_cannot() {
    let lead = resolve_effective_permission(
        PermissionMode::Balanced,
        PermissionPolicy::InheritWork,
        RoleKind::Lead,
        &[],
        &EngineCapabilities::default(),
    );
    let member = resolve_effective_permission(
        PermissionMode::Balanced,
        PermissionPolicy::ReadOnly,
        RoleKind::Researcher,
        &[],
        &EngineCapabilities::default(),
    );

    assert!(authorize_tool(&lead, TOOL_DELEGATE_ASSIGNMENT).is_ok());
    assert!(authorize_tool(&member, TOOL_DELEGATE_ASSIGNMENT).is_err());
    assert!(authorize_tool(&lead, TOOL_COMPLETE_WORK_DELIVERY).is_ok());
    assert!(authorize_tool(&member, TOOL_COMPLETE_WORK_DELIVERY).is_err());
    assert!(authorize_tool(&member, TOOL_SUBMIT_ASSIGNMENT_RESULT).is_ok());
    assert!(authorize_tool(&lead, TOOL_SUBMIT_ASSIGNMENT_RESULT).is_err());
}
