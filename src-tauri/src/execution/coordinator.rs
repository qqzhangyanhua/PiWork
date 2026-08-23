use std::sync::Arc;

use crate::{
    assignment::{scheduler::AssignmentSchedulerHandle, service::AssignmentService},
    error::AppError,
    work::repository::WorkRepository,
};

use super::{ExecutionCommand, ExecutionOutcome, ExecutionReceipt, SubmissionReceipt, WorkInput};

#[derive(Clone)]
pub struct ExecutionCoordinator {
    assignments: Arc<AssignmentService>,
    works: WorkRepository,
    scheduler: AssignmentSchedulerHandle,
}

impl ExecutionCoordinator {
    pub fn new(
        assignments: Arc<AssignmentService>,
        works: WorkRepository,
        scheduler: AssignmentSchedulerHandle,
    ) -> Self {
        Self {
            assignments,
            works,
            scheduler,
        }
    }

    pub async fn submit(
        &self,
        work_id: &str,
        input: WorkInput,
    ) -> Result<SubmissionReceipt, AppError> {
        let output = self
            .assignments
            .start_lead_assignment(
                work_id,
                input.instruction,
                input.referenced_files,
                input.resource_ids,
            )
            .await?;
        Ok(SubmissionReceipt { output })
    }

    pub async fn control(
        &self,
        work_id: &str,
        command: ExecutionCommand,
    ) -> Result<ExecutionReceipt, AppError> {
        let (outcome, submission) = match command {
            ExecutionCommand::Stop => {
                self.scheduler.stop(work_id).await?;
                (ExecutionOutcome::Stopped, None)
            }
            ExecutionCommand::Steer(input) => {
                let receipt = self.submit(work_id, input).await?;
                (ExecutionOutcome::Submitted, Some(receipt.output))
            }
            ExecutionCommand::InterruptAndReplace(input) => {
                self.scheduler.stop(work_id).await?;
                let receipt = self.submit(work_id, input).await?;
                (ExecutionOutcome::Replaced, Some(receipt.output))
            }
        };
        let work = self
            .works
            .get(work_id)
            .await?
            .ok_or_else(|| AppError::work_not_found(work_id))?;
        Ok(ExecutionReceipt {
            outcome,
            work,
            submission,
        })
    }
}
