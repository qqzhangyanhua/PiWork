use tauri::State;

use crate::{assignment::repository::AssignmentRepository, error::AppError};

/// Called after the frontend has registered its live event listener.
#[tauri::command]
pub async fn drain_assignment_event_outbox(
    repository: State<'_, AssignmentRepository>,
) -> Result<(), AppError> {
    repository.drain_pending_events().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn typed_assignment_command_items_typecheck() {
        let _ = super::drain_assignment_event_outbox;
    }
}
