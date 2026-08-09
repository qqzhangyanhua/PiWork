use tauri::{AppHandle, Manager, State};

use crate::{
    app_state::AppState,
    domain::work::{
        CreateWorkInput, ProjectFileSummary, StartWorkInput, StartWorkOutput, WorkDetail,
        WorkSummary,
    },
    error::AppError,
    paths::AppPaths,
};

#[tauri::command]
pub async fn get_default_project_directory(app: AppHandle) -> Result<String, AppError> {
    let paths = AppPaths::from_resolver(app.path())?;
    Ok(paths
        .ensure_default_workspace()?
        .to_string_lossy()
        .into_owned())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn create_work(
    state: State<'_, AppState>,
    input: CreateWorkInput,
) -> Result<WorkDetail, AppError> {
    state.model_service().require_configured().await?;
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

#[tauri::command(rename_all = "camelCase")]
pub async fn list_project_files(
    state: State<'_, AppState>,
    root_path: String,
) -> Result<Vec<ProjectFileSummary>, AppError> {
    state.work_service().list_project_files(root_path).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn start_work(
    state: State<'_, AppState>,
    work_id: String,
    input: StartWorkInput,
) -> Result<StartWorkOutput, AppError> {
    state.model_service().require_configured().await?;
    state.work_service().start_work(&work_id, input).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn stop_work(
    state: State<'_, AppState>,
    work_id: String,
) -> Result<WorkDetail, AppError> {
    state.work_service().stop_work(&work_id).await
}

#[cfg(test)]
mod tests {
    #[test]
    fn typed_work_command_items_typecheck() {
        let _ = super::get_default_project_directory;
        let _ = super::create_work;
        let _ = super::list_works;
        let _ = super::get_work;
        let _ = super::list_project_files;
        let _ = super::start_work;
        let _ = super::stop_work;
    }
}
