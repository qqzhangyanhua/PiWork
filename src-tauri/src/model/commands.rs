use tauri::State;

use crate::{app_state::AppState, error::AppError};

use super::{
    ModelConfigurationStatus, ModelConfigurationSummary, ModelConnectionInput,
    ModelConnectionResult, SaveModelConfigurationInput, SelectModelForConfigurationInput,
};

#[tauri::command]
pub async fn get_model_configuration_status(
    state: State<'_, AppState>,
) -> Result<ModelConfigurationStatus, AppError> {
    state.model_service().status().await
}

#[tauri::command]
pub async fn list_model_configurations(
    state: State<'_, AppState>,
) -> Result<Vec<ModelConfigurationSummary>, AppError> {
    state.model_service().list_configurations().await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn test_model_connection(
    state: State<'_, AppState>,
    input: ModelConnectionInput,
) -> Result<ModelConnectionResult, AppError> {
    state.model_service().test_connection(input).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn save_model_configuration(
    state: State<'_, AppState>,
    input: SaveModelConfigurationInput,
) -> Result<ModelConfigurationSummary, AppError> {
    state.model_service().save(input).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn activate_model_configuration(
    state: State<'_, AppState>,
    configuration_id: String,
) -> Result<ModelConfigurationSummary, AppError> {
    state
        .model_service()
        .activate_configuration(&configuration_id)
        .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn select_model_for_configuration(
    state: State<'_, AppState>,
    input: SelectModelForConfigurationInput,
) -> Result<ModelConfigurationSummary, AppError> {
    state.model_service().select_model(input).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn test_saved_model_configuration(
    state: State<'_, AppState>,
    configuration_id: String,
) -> Result<ModelConnectionResult, AppError> {
    state
        .model_service()
        .test_saved_configuration(&configuration_id)
        .await
}
