//! Tauri commands for the user-facing collaboration queries: listing and
//! resolving Memory candidates. Ledger/plan and team data already flow through
//! the Work event stream and `get_work_team`, so no duplicate command is needed
//! for reads that the Inspector projects from events.

use tauri::State;

use crate::{
    assignment::repository::AssignmentRepository,
    collaboration::memory::MemoryService,
    domain::{collaboration::MemoryCandidateSummary, event::WorkEventPayload},
    error::AppError,
};

const USER_ACTOR: &str = "user";

/// Confirms or rejects a proposed Memory candidate. Confirmation writes the
/// candidate to `agent_memory`; rejection keeps the audit trail. Either way the
/// candidate's status is journaled for the Inspector.
#[tauri::command(rename_all = "camelCase")]
pub async fn resolve_memory_candidate(
    memory: State<'_, MemoryService>,
    repository: State<'_, AssignmentRepository>,
    candidate_id: String,
    confirm: bool,
) -> Result<MemoryCandidateSummary, AppError> {
    let (resolved, source_assignment_id) = memory
        .resolve_candidate(&candidate_id, confirm, USER_ACTOR)
        .await?;
    if let Some(assignment_id) = source_assignment_id {
        repository
            .emit_collaboration_event(
                &assignment_id,
                WorkEventPayload::MemoryCandidateResolved {
                    candidate_id: resolved.id.clone(),
                    status: resolved.status,
                    resolved_by: USER_ACTOR.to_owned(),
                },
            )
            .await?;
    }
    Ok(resolved)
}

/// Lists the still-open (`proposed`) Memory candidates for a Work.
#[tauri::command(rename_all = "camelCase")]
pub async fn list_memory_candidates(
    memory: State<'_, MemoryService>,
    work_id: String,
) -> Result<Vec<MemoryCandidateSummary>, AppError> {
    memory.list_work_candidates(&work_id).await
}
