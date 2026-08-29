use tauri::State;

use crate::{
    app_state::AppState,
    assignment::event_outbox::AssignmentEventOutbox,
    domain::{
        assignment::{AssignmentSummary, InterruptWorkInput, QueueWorkInput},
        work::StartWorkOutput,
    },
    error::AppError,
    execution::{ExecutionCommand, WorkInput},
};

/// Called after the frontend has registered its live event listener.
#[tauri::command]
pub async fn drain_assignment_event_outbox(
    outbox: State<'_, AssignmentEventOutbox>,
) -> Result<(), AppError> {
    outbox.drain().await?;
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
        .execution_coordinator()
        .submit(
            &work_id,
            WorkInput {
                instruction: input.instruction,
                referenced_files: input.referenced_files,
                resource_ids: input.resource_ids,
            },
        )
        .await
        .map(|receipt| receipt.output)
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
        .execution_coordinator()
        .control(
            &work_id,
            ExecutionCommand::InterruptAndReplace(WorkInput {
                instruction: input.replacement.instruction,
                referenced_files: input.replacement.referenced_files,
                resource_ids: input.replacement.resource_ids,
            }),
        )
        .await?
        .submission
        .ok_or_else(|| AppError::engine("replacement submission was not created"))
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
