use std::collections::BTreeSet;

use serde::Serialize;

use crate::domain::agent::{
    AgentDefinitionSummary, AgentInstanceSummary, AssemblyDiagnostic, AssemblyDiagnosticCode,
    CapabilityPackStatus, CapabilityPackSummary, PermissionPolicy, RoleTemplateSummary,
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
    pub(crate) validated_role: RoleTemplateSummary,
    pub(crate) source_instance_snapshot: Option<Box<AgentInstanceSummary>>,
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

    let context_chars = assembly_context_chars(role, source, packs);
    if context_chars > MAX_ASSEMBLY_INSTRUCTION_CHARS {
        diagnostics.push(diagnostic(
            AssemblyDiagnosticCode::ContextBudgetExceeded,
            None,
            format!(
                "Assembly context uses {context_chars} characters; the limit is {MAX_ASSEMBLY_INSTRUCTION_CHARS}"
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
            validated_role: role.clone(),
            source_instance_snapshot: None,
        })
    } else {
        Err(diagnostics)
    }
}

/// Counts Unicode scalar values in every field injected into assembly context.
/// Structured fields are charged using their compact JSON representation.
pub fn assembly_context_chars(
    role: &RoleTemplateSummary,
    source: &AgentDefinitionSummary,
    packs: &[CapabilityPackSummary],
) -> usize {
    role.base_instructions.chars().count()
        + json_chars(&role.responsibilities)
        + json_chars(&role.non_responsibilities)
        + json_chars(&role.base_result_contract)
        + source.instructions.chars().count()
        + json_chars(&source.responsibilities)
        + json_chars(&source.non_responsibilities)
        + json_chars(&source.input_contract)
        + json_chars(&source.result_contract)
        + json_chars(&source.quality_rubric)
        + packs
            .iter()
            .map(|pack| {
                pack.instructions.chars().count()
                    + json_chars(&pack.input_schema)
                    + json_chars(&pack.output_schema)
                    + json_chars(&pack.procedure)
                    + json_chars(&pack.validation_rubric)
            })
            .sum::<usize>()
}

fn json_chars(value: &impl Serialize) -> usize {
    serde_json::to_string(value)
        .expect("assembly context DTOs are always JSON serializable")
        .chars()
        .count()
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

pub(crate) fn least_permission(
    left: PermissionPolicy,
    right: PermissionPolicy,
) -> PermissionPolicy {
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
