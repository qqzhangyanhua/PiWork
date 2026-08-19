use tauri::State;

use crate::{
    app_state::AppState,
    assignment::repository::AssignmentRepository,
    domain::{
        assignment::{AssignmentSummary, InterruptWorkInput, QueueWorkInput},
        work::StartWorkOutput,
    },
    error::AppError,
};

/// Called after the frontend has registered its live event listener.
#[tauri::command]
pub async fn drain_assignment_event_outbox(
    repository: State<'_, AssignmentRepository>,
) -> Result<(), AppError> {
    repository.drain_pending_events().await?;
    Ok(())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn list_work_assignments(
    state: State<'_, AppState>,
    work_id: String,
) -> Result<Vec<AssignmentSummary>, AppError> {
    state
        .assignment_service()
        .list_work_assignments(&work_id)
        .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn queue_work_input(
    state: State<'_, AppState>,
    work_id: String,
    input: QueueWorkInput,
) -> Result<StartWorkOutput, AppError> {
    state
        .assignment_service()
        .start_lead_assignment(
            &work_id,
            input.instruction,
            input.referenced_files,
            input.resource_ids,
        )
        .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn confirm_assignment_recovery(
    state: State<'_, AppState>,
    assignment_id: String,
    resume: bool,
) -> Result<AssignmentSummary, AppError> {
    state
        .assignment_service()
        .confirm_assignment_recovery(&assignment_id, resume)
        .await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn interrupt_and_replace(
    state: State<'_, AppState>,
    work_id: String,
    input: InterruptWorkInput,
) -> Result<StartWorkOutput, AppError> {
    state
        .assignment_service()
        .interrupt_and_replace(&work_id, input.replacement)
        .await
}

#[cfg(test)]
mod tests {
    #[test]
    fn typed_assignment_command_items_typecheck() {
        let _ = super::drain_assignment_event_outbox;
        let _ = super::list_work_assignments;
        let _ = super::queue_work_input;
        let _ = super::confirm_assignment_recovery;
        let _ = super::interrupt_and_replace;
    }
}
