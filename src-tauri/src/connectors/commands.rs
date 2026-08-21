use tauri::State;

use crate::{app_state::AppState, error::AppError};

use super::{
    AppNotificationSummary, ConnectorTestResult, EmailConnectorSummary, EmailMetadataSummary,
    PendingConnectorActionSummary, SaveEmailConnectorInput, SetConnectorWorkGrantInput,
};

#[tauri::command]
pub async fn list_email_connectors(
    state: State<'_, AppState>,
) -> Result<Vec<EmailConnectorSummary>, AppError> {
    state.connector_service().list_connections().await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn save_email_connector(
    state: State<'_, AppState>,
    input: SaveEmailConnectorInput,
) -> Result<EmailConnectorSummary, AppError> {
    state.connector_service().save_connection(input).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn test_email_connector(
    state: State<'_, AppState>,
    input: SaveEmailConnectorInput,
) -> Result<ConnectorTestResult, AppError> {
    state.connector_service().test_connection(input).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn set_email_connector_enabled(
    state: State<'_, AppState>,
    connection_id: String,
    enabled: bool,
) -> Result<EmailConnectorSummary, AppError> {
    state
        .connector_service()
        .set_enabled(&connection_id, enabled)
        .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn delete_email_connector(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<(), AppError> {
    state
        .connector_service()
        .delete_connection(&connection_id)
        .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn set_connector_work_grant(
    state: State<'_, AppState>,
    input: SetConnectorWorkGrantInput,
) -> Result<EmailConnectorSummary, AppError> {
    state.connector_service().set_work_grant(input).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn list_email_metadata(
    state: State<'_, AppState>,
    connection_id: String,
    query: String,
    limit: u32,
) -> Result<Vec<EmailMetadataSummary>, AppError> {
    state
        .connector_service()
        .list_metadata(&connection_id, &query, limit)
        .await
}

#[tauri::command]
pub async fn list_app_notifications(
    state: State<'_, AppState>,
    limit: u32,
) -> Result<Vec<AppNotificationSummary>, AppError> {
    state.connector_service().list_notifications(limit).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn mark_app_notification_read(
    state: State<'_, AppState>,
    notification_id: String,
) -> Result<(), AppError> {
    state
        .connector_service()
        .mark_notification_read(&notification_id)
        .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn clear_app_notification(
    state: State<'_, AppState>,
    notification_id: String,
) -> Result<(), AppError> {
    state
        .connector_service()
        .clear_notification(&notification_id)
        .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn list_pending_connector_actions(
    state: State<'_, AppState>,
    work_id: Option<String>,
) -> Result<Vec<PendingConnectorActionSummary>, AppError> {
    state
        .connector_service()
        .list_pending_actions(work_id.as_deref())
        .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn resolve_pending_connector_action(
    state: State<'_, AppState>,
    action_id: String,
    approve: bool,
) -> Result<PendingConnectorActionSummary, AppError> {
    state
        .connector_service()
        .resolve_pending_action(&action_id, approve)
        .await
}
