use std::collections::{BTreeSet, HashMap};

use crate::{
    agent::{
        assembly::{ResolvedAgentAssembly, validate_assembly},
        repository::AgentRepository,
    },
    domain::agent::{
        AgentInstanceSummary, AssemblyDiagnostic, CapabilityPackSummary, RoleTemplateSummary,
        SaveAgentAssemblyInput, WorkTeamSummary,
    },
    error::AppError,
};

#[derive(Clone)]
pub struct AgentService {
    repository: AgentRepository,
    available_tools: BTreeSet<String>,
    engine_capabilities: BTreeSet<String>,
}

impl AgentService {
    pub fn new(
        repository: AgentRepository,
        available_tools: BTreeSet<String>,
        engine_capabilities: BTreeSet<String>,
    ) -> Self {
        Self {
            repository,
            available_tools,
            engine_capabilities,
        }
    }

    pub async fn list_team_members(&self) -> Result<Vec<AgentInstanceSummary>, AppError> {
        self.repository.list_agent_instances().await
    }

    pub async fn list_capability_catalog(&self) -> Result<Vec<CapabilityPackSummary>, AppError> {
        self.repository.list_capability_packs().await
    }

    pub async fn get_work_team(&self, work_id: &str) -> Result<WorkTeamSummary, AppError> {
        self.repository
            .get_work_team(work_id)
            .await?
            .ok_or_else(|| AppError::work_not_found(work_id))
    }

    pub async fn validate_agent_assembly(
        &self,
        input: SaveAgentAssemblyInput,
    ) -> Result<Vec<AssemblyDiagnostic>, AppError> {
        match self.resolve(input).await? {
            Ok(_) => Ok(Vec::new()),
            Err(diagnostics) => Ok(diagnostics),
        }
    }

    pub async fn save_agent_copy(
        &self,
        input: SaveAgentAssemblyInput,
    ) -> Result<AgentInstanceSummary, AppError> {
        let resolved = self.resolve(input).await?.map_err(|diagnostics| {
            AppError::invalid_input(
                "capabilityPackIds",
                diagnostics
                    .into_iter()
                    .map(|diagnostic| diagnostic.message)
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })?;
        self.repository.copy_agent_assembly(resolved).await
    }

    pub async fn add_member_to_work(
        &self,
        work_id: &str,
        instance_id: &str,
    ) -> Result<WorkTeamSummary, AppError> {
        self.repository.add_work_member(work_id, instance_id).await
    }

    async fn resolve(
        &self,
        input: SaveAgentAssemblyInput,
    ) -> Result<Result<ResolvedAgentAssembly, Vec<AssemblyDiagnostic>>, AppError> {
        if input.display_name.trim().is_empty() {
            return Err(AppError::invalid_input(
                "displayName",
                "Display name must not be empty",
            ));
        }
        if matches!(input.parallelism_override, Some(0 | 9..)) {
            return Err(AppError::invalid_input(
                "parallelismOverride",
                "Parallelism must be between 1 and 8",
            ));
        }
        let source_instance = self
            .repository
            .get_agent_instance(&input.source_instance_id)
            .await?
            .ok_or_else(|| {
                AppError::invalid_input("sourceInstanceId", "Agent instance does not exist")
            })?;
        let roles = self.repository.list_role_templates().await?;
        let role = find_role(&roles, &source_instance.definition.role_template_id)?;
        let packs_by_id = self
            .repository
            .list_capability_packs()
            .await?
            .into_iter()
            .map(|pack| (pack.id.clone(), pack))
            .collect::<HashMap<_, _>>();
        let mut packs = Vec::with_capacity(input.capability_pack_ids.len());
        let mut seen = BTreeSet::new();
        for pack_id in &input.capability_pack_ids {
            if !seen.insert(pack_id) {
                return Err(AppError::invalid_input(
                    "capabilityPackIds",
                    "Capability pack ids must be unique",
                ));
            }
            packs.push(packs_by_id.get(pack_id).cloned().ok_or_else(|| {
                AppError::invalid_input("capabilityPackIds", "Capability pack does not exist")
            })?);
        }

        let source_permission = source_instance
            .permission_policy_override
            .unwrap_or(source_instance.definition.default_permission_policy);
        let requested_permission = input
            .permission_policy_override
            .unwrap_or(source_permission);
        let mut source = source_instance.definition;
        source.default_permission_policy = source_permission;
        let mut result = validate_assembly(
            role,
            &source,
            &packs,
            &self.available_tools,
            &self.engine_capabilities,
            requested_permission,
        );
        if let Ok(resolved) = &mut result {
            resolved.display_name = input.display_name;
            resolved.engine_override = input.engine_override;
            resolved.model_configuration_override = input.model_configuration_override;
            resolved.permission_policy_override = input.permission_policy_override;
            resolved.parallelism_override = input.parallelism_override;
        }
        Ok(result)
    }
}

fn find_role<'a>(
    roles: &'a [RoleTemplateSummary],
    role_id: &str,
) -> Result<&'a RoleTemplateSummary, AppError> {
    roles
        .iter()
        .find(|role| role.id == role_id)
        .ok_or_else(|| AppError::invalid_input("sourceInstanceId", "Role template does not exist"))
}
