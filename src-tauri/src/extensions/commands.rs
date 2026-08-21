use tauri::State;

use crate::{app_state::AppState, error::AppError};

use super::{
    CommunityExtensionSummary, ExtensionSummary, SaveWebAccessSettingsInput,
    WebAccessSettingsSummary,
};

#[tauri::command]
pub async fn list_extensions(
    state: State<'_, AppState>,
) -> Result<Vec<ExtensionSummary>, AppError> {
    state.extension_service().list_extensions().await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn search_community_extensions(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<CommunityExtensionSummary>, AppError> {
    state.extension_service().search_community(&query).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn set_extension_agent_enabled(
    state: State<'_, AppState>,
    package_id: String,
    agent_instance_id: String,
    enabled: bool,
    tool_allowlist: Vec<String>,
) -> Result<ExtensionSummary, AppError> {
    state
        .extension_service()
        .set_agent_enabled(&package_id, &agent_instance_id, enabled, tool_allowlist)
        .await
}

#[tauri::command]
pub async fn get_web_access_settings(
    state: State<'_, AppState>,
) -> Result<WebAccessSettingsSummary, AppError> {
    state.extension_service().web_access_settings().await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn save_web_access_settings(
    state: State<'_, AppState>,
    input: SaveWebAccessSettingsInput,
) -> Result<WebAccessSettingsSummary, AppError> {
    state
        .extension_service()
        .save_web_access_settings(input)
        .await
}
