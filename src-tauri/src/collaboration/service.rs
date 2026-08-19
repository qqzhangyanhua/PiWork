//! LeadToolService: the durable, authorized host tools a Lead Agent calls to
//! inspect its team, delegate a single-level Member Assignment, query status,
//! and cancel its own running children. Every mutation is a SQLite transaction
//! followed by a scheduler wake; nothing here waits for a child to finish.

use crate::{
    agent::repository::AgentRepository,
    assignment::{
        repository::{AcceptAssignmentInput, AssignmentRepository},
        scheduler::AssignmentSchedulerHandle,
    },
    collaboration::result::{repair_decision, validate_result, RepairDecision, ResultSubmissionContext},
    domain::{
        agent::WorkAgentSummary,
        assignment::{AssignmentKind, AssignmentSideEffect, AssignmentStatus, AssignmentSummary},
        collaboration::{DelegateAssignmentInput, ResultEnvelope},
        event::WorkEventPayload,
    },
    error::AppError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegateResult {
    pub assignment_id: String,
    pub status: String,
}

#[derive(Clone)]
pub struct LeadToolService {
    repository: AssignmentRepository,
    agent_repository: AgentRepository,
    scheduler: AssignmentSchedulerHandle,
}

impl LeadToolService {
    pub fn new(
        repository: AssignmentRepository,
        agent_repository: AgentRepository,
        scheduler: AssignmentSchedulerHandle,
    ) -> Self {
        Self {
            repository,
            agent_repository,
            scheduler,
        }
    }

    pub async fn list_work_members(
        &self,
        work_id: &str,
    ) -> Result<Vec<WorkAgentSummary>, AppError> {
        let team = self
            .agent_repository
            .get_work_team(work_id)
            .await?
            .ok_or_else(|| AppError::invalid_input("workId", "work team not found"))?;
        let mut members = team.members;
        members.push(team.lead);
        Ok(members)
    }

    pub async fn get_assignment_status(
        &self,
        assignment_ids: Vec<String>,
    ) -> Result<Vec<AssignmentSummary>, AppError> {
        let mut summaries = Vec::with_capacity(assignment_ids.len());
        for id in assignment_ids {
            if let Some(summary) = self.repository.get_assignment(&id).await? {
                summaries.push(summary);
            }
        }
        Ok(summaries)
    }

    /// Persists a single-level Member Assignment under the Lead and records the
    /// parent→child dependency so the Lead can resume once the child terminal.
    pub async fn delegate_assignment(
        &self,
        lead_assignment_id: &str,
        input: DelegateAssignmentInput,
    ) -> Result<DelegateResult, AppError> {
        let lead = self
            .repository
            .get_assignment(lead_assignment_id)
            .await?
            .ok_or_else(|| AppError::invalid_input("leadAssignmentId", "lead assignment not found"))?;
        if lead.kind != AssignmentKind::Lead {
            return Err(AppError::invalid_input(
                "leadAssignmentId",
                "only the Lead assignment can delegate",
            ));
        }
        if lead.parent_assignment_id.is_some() {
            return Err(AppError::invalid_input(
                "leadAssignmentId",
                "delegation is single-level only",
            ));
        }
        if input.assigned_agent_id == lead.assigned_agent_id {
            return Err(AppError::invalid_input(
                "assignedAgentId",
                "the Lead cannot delegate to itself",
            ));
        }

        let team = self
            .agent_repository
            .get_work_team(&lead.work_id)
            .await?
            .ok_or_else(|| AppError::invalid_input("workId", "work team not found"))?;
        let is_member = team
            .members
            .iter()
            .any(|member| member.instance.id == input.assigned_agent_id);
        if !is_member {
            return Err(AppError::invalid_input(
                "assignedAgentId",
                "target is not an active Work member",
            ));
        }

        let child = self
            .repository
            .accept(AcceptAssignmentInput {
                id: None,
                work_id: lead.work_id.clone(),
                parent_assignment_id: Some(lead.id.clone()),
                created_by_agent_id: Some(lead.assigned_agent_id.clone()),
                assigned_agent_id: input.assigned_agent_id.clone(),
                capability_pack_id: input.capability_pack_id,
                kind: AssignmentKind::Member,
                side_effect: AssignmentSideEffect::Unknown,
                title: input.title.clone(),
                instruction: input.instruction,
                context_manifest: input.context_manifest,
                expected_result_schema: input.expected_result_schema,
                acceptance_criteria: input.acceptance_criteria,
                permission_scope: input.permission_scope,
                priority: input.priority,
                max_attempts: input.max_attempts.max(1),
                not_before: None,
            })
            .await?;

        self.repository
            .add_dependency(&lead.id, &child.id)
            .await?;
        self.repository
            .record_delegation(
                &lead.id,
                &child.id,
                &input.assigned_agent_id,
                &input.title,
            )
            .await?;
        self.scheduler.wake()?;

        Ok(DelegateResult {
            assignment_id: child.id,
            status: "queued".to_owned(),
        })
    }

    /// Cancels a running child Assignment owned by the given Lead. The
    /// scheduler aborts the engine and cancels the attempt; queued or terminal
    /// children are rejected rather than silently mutated.
    pub async fn cancel_assignment(
        &self,
        lead_assignment_id: &str,
        child_assignment_id: &str,
    ) -> Result<AssignmentSummary, AppError> {
        let child = self
            .repository
            .get_assignment(child_assignment_id)
            .await?
            .ok_or_else(|| AppError::invalid_input("assignmentId", "assignment not found"))?;
        if child.parent_assignment_id.as_deref() != Some(lead_assignment_id) {
            return Err(AppError::invalid_input(
                "assignmentId",
                "only the parent Lead can cancel this child",
            ));
        }
        if matches!(
            child.status,
            AssignmentStatus::Completed
                | AssignmentStatus::Cancelled
                | AssignmentStatus::DeadLetter
        ) {
            return Err(AppError::invalid_input(
                "assignmentId",
                "terminal assignments cannot be cancelled",
            ));
        }
        if child.status != AssignmentStatus::Running {
            return Err(AppError::invalid_input(
                "assignmentId",
                "only a running assignment can be cancelled",
            ));
        }
        self.scheduler.interrupt(&child.work_id)?;
        let cancelled = self
            .repository
            .get_assignment(child_assignment_id)
            .await?
            .ok_or_else(|| AppError::invalid_input("assignmentId", "assignment not found"))?;
        self.scheduler.wake()?;
        Ok(cancelled)
    }
}

/// MemberResultService: a Member submits one structured Result, which is
/// validated and either accepted (completing the Assignment), repaired once, or
/// rejected and escalated to the Lead.
#[derive(Clone)]
pub struct MemberResultService {
    repository: AssignmentRepository,
    scheduler: AssignmentSchedulerHandle,
}

impl MemberResultService {
    pub fn new(repository: AssignmentRepository, scheduler: AssignmentSchedulerHandle) -> Self {
        Self {
            repository,
            scheduler,
        }
    }

    pub async fn submit_assignment_result(
        &self,
        submission: MemberResultSubmission,
    ) -> Result<SubmitOutcome, AppError> {
        let envelope_json = serde_json::to_string(&submission.envelope)
            .map_err(|error| AppError::Database(sqlx::Error::Encode(Box::new(error))))?;
        let context = ResultSubmissionContext {
            work_id: submission.work_id.clone(),
            assignment_id: submission.assignment_id.clone(),
            run_id: submission.run_id.clone(),
            author_agent_id: submission.author_agent_id.clone(),
        };

        match validate_result(&context, submission.envelope) {
            Ok(valid) => {
                self.repository
                    .record_result(
                        &submission.assignment_id,
                        &submission.author_agent_id,
                        &envelope_json,
                        "valid",
                        0,
                    )
                    .await?;
                let summary = valid.envelope.summary.clone();
                let status = valid.envelope.status;
                let assignment = self
                    .repository
                    .complete_by_assignment(
                        &submission.assignment_id,
                        &summary,
                        &submission.runtime_owner,
                    )
                    .await?;
                self.repository
                    .emit_collaboration_event(
                        &submission.assignment_id,
                        WorkEventPayload::AssignmentResultSubmitted {
                            assignment_id: submission.assignment_id.clone(),
                            agent_instance_id: submission.author_agent_id.clone(),
                            status,
                            summary,
                        },
                    )
                    .await?;
                self.scheduler.wake()?;
                Ok(SubmitOutcome::Accepted { assignment })
            }
            Err(diagnostics) => {
                let prior = self
                    .repository
                    .latest_repair_attempt(&submission.assignment_id)
                    .await?;
                match repair_decision(prior, diagnostics) {
                    RepairDecision::RequestRepair { diagnostics } => {
                        self.repository
                            .record_result(
                                &submission.assignment_id,
                                &submission.author_agent_id,
                                &envelope_json,
                                "repair_requested",
                                prior,
                            )
                            .await?;
                        Ok(SubmitOutcome::RepairRequested { diagnostics })
                    }
                    RepairDecision::Reject { diagnostics } => {
                        self.repository
                            .record_result(
                                &submission.assignment_id,
                                &submission.author_agent_id,
                                &envelope_json,
                                "rejected",
                                prior,
                            )
                            .await?;
                        self.repository
                            .emit_collaboration_event(
                                &submission.assignment_id,
                                WorkEventPayload::AssignmentResultRejected {
                                    assignment_id: submission.assignment_id.clone(),
                                    agent_instance_id: submission.author_agent_id.clone(),
                                    reason: diagnostics.join("; "),
                                },
                            )
                            .await?;
                        self.scheduler.wake()?;
                        Ok(SubmitOutcome::Rejected { diagnostics })
                    }
                }
            }
        }
    }
}

pub struct MemberResultSubmission {
    pub work_id: String,
    pub assignment_id: String,
    pub run_id: String,
    pub author_agent_id: String,
    pub runtime_owner: String,
    pub envelope: ResultEnvelope,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SubmitOutcome {
    Accepted {
        assignment: AssignmentSummary,
    },
    RepairRequested {
        diagnostics: Vec<String>,
    },
    Rejected {
        diagnostics: Vec<String>,
    },
}
