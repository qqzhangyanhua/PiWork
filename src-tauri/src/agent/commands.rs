use tauri::State;

use crate::{
    app_state::AppState,
    domain::agent::{
        AgentInstanceSummary, AssemblyDiagnostic, CapabilityPackSummary, SaveAgentAssemblyInput,
        WorkTeamSummary,
    },
    error::AppError,
};

#[tauri::command]
pub async fn list_agent_instances(
    state: State<'_, AppState>,
) -> Result<Vec<AgentInstanceSummary>, AppError> {
    state.agent_service().list_team_members().await
}

#[tauri::command]
pub async fn list_capability_packs(
    state: State<'_, AppState>,
) -> Result<Vec<CapabilityPackSummary>, AppError> {
    state.agent_service().list_capability_catalog().await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_work_team(
    state: State<'_, AppState>,
    work_id: String,
) -> Result<WorkTeamSummary, AppError> {
    state.agent_service().get_work_team(&work_id).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn validate_agent_assembly(
    state: State<'_, AppState>,
    input: SaveAgentAssemblyInput,
) -> Result<Vec<AssemblyDiagnostic>, AppError> {
    state.agent_service().validate_agent_assembly(input).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn save_agent_copy(
    state: State<'_, AppState>,
    input: SaveAgentAssemblyInput,
) -> Result<AgentInstanceSummary, AppError> {
    state.agent_service().save_agent_copy(input).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn add_work_member(
    state: State<'_, AppState>,
    work_id: String,
    agent_instance_id: String,
) -> Result<WorkTeamSummary, AppError> {
    state
        .agent_service()
        .add_member_to_work(&work_id, &agent_instance_id)
        .await
}

#[cfg(test)]
mod tests {
    #[test]
    fn typed_agent_command_items_typecheck() {
        let _ = super::list_agent_instances;
        let _ = super::list_capability_packs;
        let _ = super::get_work_team;
        let _ = super::validate_agent_assembly;
        let _ = super::save_agent_copy;
        let _ = super::add_work_member;
    }

    #[test]
    fn commands_delegate_without_accessing_storage() {
        let source = include_str!("commands.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(!production.contains("sqlx::"));
        assert!(!production.contains("AgentRepository"));
    }
}
