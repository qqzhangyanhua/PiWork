use tauri::State;

use crate::{
    app_state::AppState,
    domain::resource::{ImportResourcesInput, ResourceSummary, ResourceThumbnail},
    error::AppError,
};

#[tauri::command(rename_all = "camelCase")]
pub async fn import_resources(
    state: State<'_, AppState>,
    input: ImportResourcesInput,
) -> Result<Vec<ResourceSummary>, AppError> {
    state.resource_service().import_resources(input).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn list_work_resources(
    state: State<'_, AppState>,
    work_id: String,
) -> Result<Vec<ResourceSummary>, AppError> {
    state.resource_service().list_work_resources(&work_id).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_resource_thumbnail(
    state: State<'_, AppState>,
    resource_id: String,
) -> Result<ResourceThumbnail, AppError> {
    state.resource_service().thumbnail(&resource_id).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn detach_draft_resource(
    state: State<'_, AppState>,
    draft_id: String,
    resource_id: String,
) -> Result<(), AppError> {
    state
        .resource_service()
        .detach_draft_resource(&draft_id, &resource_id)
        .await
}

#[cfg(test)]
mod tests {
    #[test]
    fn typed_resource_command_items_typecheck() {
        let _ = super::import_resources;
        let _ = super::list_work_resources;
        let _ = super::get_resource_thumbnail;
        let _ = super::detach_draft_resource;
    }
}
