use tauri::State;

use crate::{app_state::AppState, error::AppError};

use super::{
    MemoryConnectionTestResult, MemorySettingsSummary, SaveMemorySettingsInput,
    SaveWorkspaceMemoryBindingInput, WorkspaceMemoryBindingSummary,
};

#[tauri::command]
pub async fn get_memory_settings(
    state: State<'_, AppState>,
) -> Result<MemorySettingsSummary, AppError> {
    state.memory_service().settings().await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn save_memory_settings(
    state: State<'_, AppState>,
    input: SaveMemorySettingsInput,
) -> Result<MemorySettingsSummary, AppError> {
    state.memory_service().save_settings(input).await
}

#[tauri::command]
pub async fn test_memory_connection(
    state: State<'_, AppState>,
) -> Result<MemoryConnectionTestResult, AppError> {
    state.memory_service().test_connection().await
}

#[tauri::command]
pub async fn list_workspace_memory_bindings(
    state: State<'_, AppState>,
) -> Result<Vec<WorkspaceMemoryBindingSummary>, AppError> {
    state.memory_service().list_workspace_bindings().await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn save_workspace_memory_binding(
    state: State<'_, AppState>,
    input: SaveWorkspaceMemoryBindingInput,
) -> Result<WorkspaceMemoryBindingSummary, AppError> {
    state.memory_service().save_workspace_binding(input).await
}

#[tauri::command]
pub async fn drain_memory_capture_outbox(state: State<'_, AppState>) -> Result<u32, AppError> {
    state.memory_service().drain_capture_outbox().await
}
