use std::collections::BTreeSet;

use crate::domain::agent::{
    AgentDefinitionSummary, AssemblyDiagnostic, AssemblyDiagnosticCode, CapabilityPackStatus,
    CapabilityPackSummary, PermissionPolicy, RoleTemplateSummary,
};

pub const MAX_ASSEMBLY_INSTRUCTION_CHARS: usize = 32_000;

#[derive(Debug, Clone)]
pub struct ResolvedAgentAssembly {
    pub(crate) source: AgentDefinitionSummary,
    pub(crate) capability_packs: Vec<CapabilityPackSummary>,
    pub(crate) permission_policy: PermissionPolicy,
    pub(crate) parallelism: u32,
    pub(crate) display_name: String,
    pub(crate) engine_override: Option<String>,
    pub(crate) model_configuration_override: Option<String>,
    pub(crate) permission_policy_override: Option<PermissionPolicy>,
    pub(crate) parallelism_override: Option<u32>,
}

impl ResolvedAgentAssembly {
    pub fn permission_policy(&self) -> PermissionPolicy {
        self.permission_policy
    }

    pub fn parallelism(&self) -> u32 {
        self.parallelism
    }
}

pub fn validate_assembly(
    role: &RoleTemplateSummary,
    source: &AgentDefinitionSummary,
    packs: &[CapabilityPackSummary],
    available_tools: &BTreeSet<String>,
    engine_capabilities: &BTreeSet<String>,
    requested_permission: PermissionPolicy,
) -> Result<ResolvedAgentAssembly, Vec<AssemblyDiagnostic>> {
    let mut diagnostics = Vec::new();
    let effective_permission =
        least_permission(source.default_permission_policy, requested_permission);

    for pack in packs {
        if pack.status != CapabilityPackStatus::Executable {
            diagnostics.push(diagnostic(
                AssemblyDiagnosticCode::NotExecutable,
                Some(pack),
                format!("Capability pack '{}' is not executable", pack.name),
            ));
        }
    }

    for pack in packs {
        if !pack
            .compatible_role_template_ids
            .iter()
            .any(|role_id| role_id == &role.id)
        {
            diagnostics.push(diagnostic(
                AssemblyDiagnosticCode::IncompatibleRole,
                Some(pack),
                format!(
                    "Capability pack '{}' is incompatible with role '{}'",
                    pack.name, role.name
                ),
            ));
        }
    }

    for pack in packs {
        for tool in &pack.required_tools {
            if !available_tools.contains(tool) {
                diagnostics.push(diagnostic(
                    AssemblyDiagnosticCode::MissingTool,
                    Some(pack),
                    format!("Required tool '{tool}' is unavailable"),
                ));
            }
        }
    }

    for pack in packs {
        for capability in &pack.required_engine_capabilities {
            if !engine_capabilities.contains(capability) {
                diagnostics.push(diagnostic(
                    AssemblyDiagnosticCode::MissingEngineCapability,
                    Some(pack),
                    format!("Required engine capability '{capability}' is unavailable"),
                ));
            }
        }
    }

    if permission_rank(requested_permission) > permission_rank(source.default_permission_policy) {
        diagnostics.push(diagnostic(
            AssemblyDiagnosticCode::PermissionEscalation,
            None,
            "Requested permission would expand the source Agent's authority".into(),
        ));
    }
    for pack in packs {
        if permission_rank(pack.default_permission_scope) > permission_rank(effective_permission) {
            diagnostics.push(diagnostic(
                AssemblyDiagnosticCode::PermissionEscalation,
                Some(pack),
                format!(
                    "Capability pack '{}' requires a broader permission",
                    pack.name
                ),
            ));
        }
    }

    for (index, pack) in packs.iter().enumerate() {
        for other in packs.iter().skip(index + 1) {
            if pack.conflicts_with_capability_pack_ids.contains(&other.id)
                || other.conflicts_with_capability_pack_ids.contains(&pack.id)
            {
                diagnostics.push(diagnostic(
                    AssemblyDiagnosticCode::CapabilityConflict,
                    Some(pack),
                    format!(
                        "Capability packs '{}' and '{}' conflict",
                        pack.name, other.name
                    ),
                ));
            }
        }
    }

    let instruction_chars = role.base_instructions.chars().count()
        + source.instructions.chars().count()
        + packs
            .iter()
            .map(|pack| pack.instructions.chars().count())
            .sum::<usize>();
    if instruction_chars > MAX_ASSEMBLY_INSTRUCTION_CHARS {
        diagnostics.push(diagnostic(
            AssemblyDiagnosticCode::ContextBudgetExceeded,
            None,
            format!(
                "Assembly instructions use {instruction_chars} characters; the limit is {MAX_ASSEMBLY_INSTRUCTION_CHARS}"
            ),
        ));
    }

    if diagnostics.is_empty() {
        Ok(ResolvedAgentAssembly {
            source: source.clone(),
            capability_packs: packs.to_vec(),
            permission_policy: effective_permission,
            parallelism: if source.builtin {
                1
            } else {
                source.default_parallelism
            },
            display_name: source.name.clone(),
            engine_override: None,
            model_configuration_override: None,
            permission_policy_override: Some(effective_permission),
            parallelism_override: None,
        })
    } else {
        Err(diagnostics)
    }
}

fn diagnostic(
    code: AssemblyDiagnosticCode,
    pack: Option<&CapabilityPackSummary>,
    message: String,
) -> AssemblyDiagnostic {
    AssemblyDiagnostic {
        code,
        capability_pack_id: pack.map(|pack| pack.id.clone()),
        message,
    }
}

fn least_permission(left: PermissionPolicy, right: PermissionPolicy) -> PermissionPolicy {
    if permission_rank(left) <= permission_rank(right) {
        left
    } else {
        right
    }
}

fn permission_rank(permission: PermissionPolicy) -> u8 {
    match permission {
        PermissionPolicy::ReadOnly => 0,
        PermissionPolicy::InheritWork => 1,
        PermissionPolicy::WorkWrite => 2,
    }
}
