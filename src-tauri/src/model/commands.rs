use tauri::State;

use crate::{app_state::AppState, error::AppError};

use super::{
    ModelConfigurationStatus, ModelConfigurationSummary, ModelConnectionInput,
    ModelConnectionResult, SaveModelConfigurationInput,
};

#[tauri::command]
pub async fn get_model_configuration_status(
    state: State<'_, AppState>,
) -> Result<ModelConfigurationStatus, AppError> {
    state.model_service().status().await
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
