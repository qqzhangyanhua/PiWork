//! Assignment Service: the user-facing boundary that turns a Work message into
//! a durable Lead Assignment and drives the Scheduler without exposing queue
//! internals. queue/steer/interrupt and recovery confirmation live here.

use serde_json::json;

use crate::{
    assignment::{
        repository::{AcceptAssignmentInput, AssignmentRepository},
        scheduler::AssignmentSchedulerHandle,
    },
    domain::{
        assignment::AssignmentKind,
        assignment::AssignmentSideEffect,
        work::{StartWorkOutput, WorkStatus},
    },
    error::AppError,
    work::repository::WorkRepository,
};

#[derive(Clone)]
pub struct AssignmentService {
    repository: AssignmentRepository,
    work_repository: WorkRepository,
    scheduler: AssignmentSchedulerHandle,
}

impl AssignmentService {
    pub fn new(
        repository: AssignmentRepository,
        work_repository: WorkRepository,
        scheduler: AssignmentSchedulerHandle,
    ) -> Self {
        Self {
            repository,
            work_repository,
            scheduler,
        }
    }

    /// Persists the user message as a Lead Assignment and wakes the Scheduler.
    /// The returned Run is `None` because the Scheduler owns dispatch asynchronously.
    #[allow(clippy::too_many_arguments)]
    pub async fn start_lead_assignment(
        &self,
        work_id: &str,
        prompt: String,
        referenced_files: Vec<String>,
        resource_ids: Vec<String>,
    ) -> Result<StartWorkOutput, AppError> {
        let prompt = prompt.trim();
        if prompt.is_empty() && resource_ids.is_empty() {
            return Err(AppError::invalid_input(
                "prompt",
                "prompt or attachment must not be empty",
            ));
        }
        let lead_agent_id = self.work_repository.lead_agent_id(work_id).await?;
        let work = self
            .work_repository
            .get(work_id)
            .await?
            .ok_or_else(|| AppError::work_not_found(work_id))?;
        let title = work.summary.title.clone();

        let context_manifest = json!({
            "referencedFiles": referenced_files,
            "resourceIds": resource_ids,
        });
        let assignment = self
            .repository
            .accept(AcceptAssignmentInput {
                id: None,
                work_id: work_id.to_owned(),
                parent_assignment_id: None,
                created_by_agent_id: None,
                assigned_agent_id: lead_agent_id,
                capability_pack_id: None,
                kind: AssignmentKind::Lead,
                side_effect: AssignmentSideEffect::Unknown,
                title,
                instruction: prompt.to_owned(),
                context_manifest,
                expected_result_schema: json!({}),
                acceptance_criteria: json!([]),
                permission_scope: json!({"mode": "inherit_work"}),
                priority: 10,
                max_attempts: 3,
                not_before: None,
            })
            .await?;

        let user_message = self
            .work_repository
            .insert_user_message(work_id, prompt, &assignment.id)
            .await?;

        let _ = self
            .work_repository
            .set_work_status(work_id, WorkStatus::Queued)
            .await;
        self.scheduler.wake()?;

        Ok(StartWorkOutput {
            assignment,
            run: None,
            user_message,
        })
    }

    pub async fn list_work_assignments(
        &self,
        work_id: &str,
    ) -> Result<Vec<crate::domain::assignment::AssignmentSummary>, AppError> {
        self.repository.list_for_work(work_id).await
    }

    pub async fn confirm_assignment_recovery(
        &self,
        assignment_id: &str,
        resume: bool,
    ) -> Result<crate::domain::assignment::AssignmentSummary, AppError> {
        let assignment = self
            .repository
            .confirm_recovery(assignment_id, resume, chrono::Utc::now())
            .await?;
        if resume {
            self.scheduler.wake()?;
        }
        Ok(assignment)
    }
}
