use tauri::State;

use crate::{
    app_state::AppState,
    domain::work::{CreateWorkInput, WorkDetail, WorkSummary},
    error::AppError,
};

#[tauri::command(rename_all = "camelCase")]
pub async fn create_work(
    state: State<'_, AppState>,
    input: CreateWorkInput,
) -> Result<WorkDetail, AppError> {
    state.work_service().create_work(input).await
}

#[tauri::command]
pub async fn list_works(state: State<'_, AppState>) -> Result<Vec<WorkSummary>, AppError> {
    state.work_service().list_works().await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_work(state: State<'_, AppState>, work_id: String) -> Result<WorkDetail, AppError> {
    state.work_service().get_work(&work_id).await
}

#[cfg(test)]
mod tests {
    #[test]
    fn typed_work_command_items_typecheck() {
        let _ = super::create_work;
        let _ = super::list_works;
        let _ = super::get_work;
    }
}
