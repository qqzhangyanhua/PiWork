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
    collaboration::{
        memory::MemoryService,
        result::{RepairDecision, ResultSubmissionContext, repair_decision, validate_result},
    },
    domain::{
        agent::WorkAgentSummary,
        assignment::{AssignmentKind, AssignmentSideEffect, AssignmentStatus, AssignmentSummary},
        collaboration::{
            CompleteWorkDeliveryInput, DelegateAssignmentInput, RecordWorkDecisionInput,
            ResultEnvelope, UpdateWorkPlanInput,
        },
        event::WorkEventPayload,
        work::WorkStatus,
    },
    error::AppError,
    work::repository::WorkRepository,
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DelegateResult {
    pub assignment_id: String,
    pub status: String,
}

#[derive(Clone)]
pub struct LeadToolService {
    repository: AssignmentRepository,
    work_repository: WorkRepository,
    agent_repository: AgentRepository,
    scheduler: AssignmentSchedulerHandle,
}

impl LeadToolService {
    pub fn new(
        repository: AssignmentRepository,
        work_repository: WorkRepository,
        agent_repository: AgentRepository,
        scheduler: AssignmentSchedulerHandle,
    ) -> Self {
        Self {
            repository,
            work_repository,
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
        if !members
            .iter()
            .any(|member| member.instance.id == team.lead.instance.id)
        {
            members.push(team.lead);
        }
        Ok(members)
    }

    pub async fn inspect_capability_packs(
        &self,
        ids: Vec<String>,
    ) -> Result<Vec<crate::domain::agent::CapabilityPackSummary>, AppError> {
        let packs = self.agent_repository.list_capability_packs().await?;
        if ids.is_empty() {
            return Ok(packs);
        }
        Ok(packs
            .into_iter()
            .filter(|pack| ids.iter().any(|id| id == &pack.id))
            .collect())
    }

    /// Retries a failed or interrupted child Assignment owned by the Lead.
    pub async fn request_assignment_retry(
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
                "only the parent Lead can retry this child",
            ));
        }
        let retried = self
            .repository
            .retry_assignment(child_assignment_id)
            .await?;
        self.scheduler.wake()?;
        Ok(retried)
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
            .ok_or_else(|| {
                AppError::invalid_input("leadAssignmentId", "lead assignment not found")
            })?;
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
        let member = team
            .members
            .iter()
            .find(|member| member.instance.id == input.assigned_agent_id)
            .ok_or_else(|| {
                AppError::invalid_input("assignedAgentId", "target is not an active Work member")
            })?;
        let capability_pack_id = match input.capability_pack_id.as_deref() {
            Some(pack_id) => {
                if !member
                    .instance
                    .definition
                    .capability_packs
                    .iter()
                    .any(|pack| pack.id == pack_id)
                {
                    return Err(AppError::invalid_input(
                        "capabilityPackId",
                        "capability pack is not bound to the target Agent",
                    ));
                }
                Some(pack_id.to_owned())
            }
            None if member.instance.definition.capability_packs.len() == 1 => member
                .instance
                .definition
                .capability_packs
                .first()
                .map(|pack| pack.id.clone()),
            None if member.instance.definition.capability_packs.len() > 1 => {
                return Err(AppError::invalid_input(
                    "capabilityPackId",
                    "target Agent has multiple capability packs; select one explicitly",
                ));
            }
            None => None,
        };

        let child = self
            .repository
            .accept(AcceptAssignmentInput {
                id: None,
                work_id: lead.work_id.clone(),
                parent_assignment_id: Some(lead.id.clone()),
                created_by_agent_id: Some(lead.assigned_agent_id.clone()),
                assigned_agent_id: input.assigned_agent_id.clone(),
                capability_pack_id,
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

        self.repository.add_dependency(&lead.id, &child.id).await?;
        self.repository
            .record_delegation(&lead.id, &child.id, &input.assigned_agent_id, &input.title)
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

    /// Records a Work decision with a monotonic version and writes the event.
    pub async fn record_work_decision(
        &self,
        lead_assignment_id: &str,
        input: RecordWorkDecisionInput,
    ) -> Result<(), AppError> {
        let lead = self
            .repository
            .get_assignment(lead_assignment_id)
            .await?
            .ok_or_else(|| {
                AppError::invalid_input("leadAssignmentId", "lead assignment not found")
            })?;
        if lead.kind != AssignmentKind::Lead {
            return Err(AppError::invalid_input(
                "leadAssignmentId",
                "only the Lead assignment can record decisions",
            ));
        }
        let version = self
            .repository
            .next_collaboration_revision(&lead.work_id, "version")
            .await?;
        self.repository
            .emit_collaboration_event(
                &lead.id,
                WorkEventPayload::WorkDecisionRecorded {
                    decision_id: uuid::Uuid::new_v4().to_string(),
                    summary: input.summary,
                    version,
                },
            )
            .await
    }

    /// Updates the Work plan with a monotonic revision and writes the event.
    pub async fn update_work_plan(
        &self,
        lead_assignment_id: &str,
        input: UpdateWorkPlanInput,
    ) -> Result<(), AppError> {
        let lead = self
            .repository
            .get_assignment(lead_assignment_id)
            .await?
            .ok_or_else(|| {
                AppError::invalid_input("leadAssignmentId", "lead assignment not found")
            })?;
        if lead.kind != AssignmentKind::Lead {
            return Err(AppError::invalid_input(
                "leadAssignmentId",
                "only the Lead assignment can update the plan",
            ));
        }
        let revision = self
            .repository
            .next_collaboration_revision(&lead.work_id, "revision")
            .await?;
        let text = input
            .plan
            .iter()
            .map(|step| step.title.clone())
            .collect::<Vec<_>>()
            .join("\n");
        self.repository
            .emit_collaboration_event(
                &lead.id,
                WorkEventPayload::WorkPlanUpdated {
                    plan_id: "work-plan".to_owned(),
                    revision,
                    text,
                },
            )
            .await
    }

    /// Completes the final delivery: writes the event, completes the Lead
    /// Assignment, and marks the Work completed.
    pub async fn complete_work_delivery(
        &self,
        lead_assignment_id: &str,
        runtime_owner: &str,
        input: CompleteWorkDeliveryInput,
    ) -> Result<AssignmentSummary, AppError> {
        let lead = self
            .repository
            .get_assignment(lead_assignment_id)
            .await?
            .ok_or_else(|| {
                AppError::invalid_input("leadAssignmentId", "lead assignment not found")
            })?;
        if lead.kind != AssignmentKind::Lead {
            return Err(AppError::invalid_input(
                "leadAssignmentId",
                "only the Lead assignment can complete delivery",
            ));
        }
        self.repository
            .emit_collaboration_event(
                &lead.id,
                WorkEventPayload::WorkDeliveryCompleted {
                    summary: input.summary.clone(),
                    artifacts: input.artifacts.clone(),
                    validation: input.validation.clone(),
                    limitations: input.limitations.clone(),
                },
            )
            .await?;
        let completed = self
            .repository
            .complete_by_assignment(&lead.id, &input.summary, runtime_owner)
            .await?;
        let _ = self
            .work_repository
            .set_work_status(&lead.work_id, WorkStatus::Completed)
            .await;
        Ok(completed)
    }
}

/// MemberResultService: a Member submits one structured Result, which is
/// validated and either accepted (completing the Assignment), repaired once, or
/// rejected and escalated to the Lead.
#[derive(Clone)]
pub struct MemberResultService {
    repository: AssignmentRepository,
    scheduler: AssignmentSchedulerHandle,
    memory: Option<MemoryService>,
}

impl MemberResultService {
    pub fn new(repository: AssignmentRepository, scheduler: AssignmentSchedulerHandle) -> Self {
        Self {
            repository,
            scheduler,
            memory: None,
        }
    }

    /// Attaches Memory candidate proposal so accepted Result envelopes can
    /// propose durable (but unconfirmed) Memory candidates.
    pub fn with_memory(mut self, memory: MemoryService) -> Self {
        self.memory = Some(memory);
        self
    }

    /// Proposes each accepted Memory candidate from an accepted Result envelope
    /// and journals a `memoryCandidateProposed` event for the Inspector.
    async fn propose_memory_candidates(
        &self,
        work_id: &str,
        assignment_id: &str,
        author_agent_id: &str,
        candidates: &[crate::domain::collaboration::MemoryCandidateInput],
    ) -> Result<(), AppError> {
        let Some(memory) = &self.memory else {
            return Ok(());
        };
        let proposed = memory
            .propose_candidates(work_id, author_agent_id, assignment_id, candidates.to_vec())
            .await?;
        for (candidate_id, content) in proposed {
            self.repository
                .emit_collaboration_event(
                    assignment_id,
                    WorkEventPayload::MemoryCandidateProposed {
                        candidate_id,
                        author_agent_id: author_agent_id.to_owned(),
                        content,
                    },
                )
                .await?;
        }
        Ok(())
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
                self.propose_memory_candidates(
                    &submission.work_id,
                    &submission.assignment_id,
                    &submission.author_agent_id,
                    &valid.envelope.memory_candidates,
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

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[allow(clippy::large_enum_variant)] // wire DTO, not a hot-loop type
pub enum SubmitOutcome {
    Accepted { assignment: AssignmentSummary },
    RepairRequested { diagnostics: Vec<String> },
    Rejected { diagnostics: Vec<String> },
}

/// Dispatches an authenticated Host Tool call to the Lead or Member service.
/// The loopback HTTP thread blocks on the captured Tokio handle for each call.
#[derive(Clone)]
pub struct HostToolDispatcher {
    lead: LeadToolService,
    member: MemberResultService,
    connectors: Option<std::sync::Arc<crate::connectors::ConnectorService>>,
    runtime: tokio::runtime::Handle,
}

impl HostToolDispatcher {
    pub fn new(lead: LeadToolService, member: MemberResultService) -> Self {
        Self {
            lead,
            member,
            connectors: None,
            runtime: tokio::runtime::Handle::current(),
        }
    }

    pub fn with_connector_service(
        mut self,
        connectors: std::sync::Arc<crate::connectors::ConnectorService>,
    ) -> Self {
        self.connectors = Some(connectors);
        self
    }

    pub fn dispatch(
        &self,
        tool: &str,
        context: &crate::collaboration::tool_bridge::AuthorizedRunContext,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, AppError> {
        self.runtime.block_on(async {
            match tool {
                crate::collaboration::tools::TOOL_LIST_WORK_MEMBERS => {
                    let members = self.lead.list_work_members(&context.work_id).await?;
                    Ok(serde_json::to_value(members).map_err(json_error)?)
                }
                crate::collaboration::tools::TOOL_INSPECT_CAPABILITY_PACKS => {
                    let ids: Vec<String> = serde_json::from_value(
                        arguments
                            .get("ids")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null),
                    )
                    .unwrap_or_default();
                    let packs = self.lead.inspect_capability_packs(ids).await?;
                    Ok(serde_json::to_value(packs).map_err(json_error)?)
                }
                crate::collaboration::tools::TOOL_DELEGATE_ASSIGNMENT => {
                    let input: DelegateAssignmentInput =
                        serde_json::from_value(arguments).map_err(decode_error)?;
                    let result = self
                        .lead
                        .delegate_assignment(&context.assignment_id, input)
                        .await?;
                    Ok(serde_json::to_value(result).map_err(json_error)?)
                }
                crate::collaboration::tools::TOOL_GET_ASSIGNMENT_STATUS => {
                    let ids: Vec<String> = serde_json::from_value(
                        arguments
                            .get("assignmentIds")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null),
                    )
                    .unwrap_or_default();
                    let statuses = self.lead.get_assignment_status(ids).await?;
                    Ok(serde_json::to_value(statuses).map_err(json_error)?)
                }
                crate::collaboration::tools::TOOL_CANCEL_ASSIGNMENT => {
                    let child_id = arguments
                        .get("assignmentId")
                        .and_then(|value| value.as_str())
                        .ok_or_else(|| AppError::invalid_input("assignmentId", "missing"))?;
                    let cancelled = self
                        .lead
                        .cancel_assignment(&context.assignment_id, child_id)
                        .await?;
                    Ok(serde_json::to_value(cancelled).map_err(json_error)?)
                }
                crate::collaboration::tools::TOOL_REQUEST_ASSIGNMENT_RETRY => {
                    let child_id = arguments
                        .get("assignmentId")
                        .and_then(|value| value.as_str())
                        .ok_or_else(|| AppError::invalid_input("assignmentId", "missing"))?;
                    let retried = self
                        .lead
                        .request_assignment_retry(&context.assignment_id, child_id)
                        .await?;
                    Ok(serde_json::to_value(retried).map_err(json_error)?)
                }
                crate::collaboration::tools::TOOL_RECORD_WORK_DECISION => {
                    let input: RecordWorkDecisionInput =
                        serde_json::from_value(arguments).map_err(decode_error)?;
                    self.lead
                        .record_work_decision(&context.assignment_id, input)
                        .await?;
                    Ok(serde_json::json!({ "ok": true }))
                }
                crate::collaboration::tools::TOOL_UPDATE_WORK_PLAN => {
                    let input: UpdateWorkPlanInput =
                        serde_json::from_value(arguments).map_err(decode_error)?;
                    self.lead
                        .update_work_plan(&context.assignment_id, input)
                        .await?;
                    Ok(serde_json::json!({ "ok": true }))
                }
                crate::collaboration::tools::TOOL_COMPLETE_WORK_DELIVERY => {
                    let input: CompleteWorkDeliveryInput =
                        serde_json::from_value(arguments).map_err(decode_error)?;
                    let completed = self
                        .lead
                        .complete_work_delivery(
                            &context.assignment_id,
                            &context.runtime_owner,
                            input,
                        )
                        .await?;
                    Ok(serde_json::to_value(completed).map_err(json_error)?)
                }
                crate::collaboration::tools::TOOL_SUBMIT_ASSIGNMENT_RESULT => {
                    let envelope: ResultEnvelope = serde_json::from_value(
                        arguments
                            .get("envelope")
                            .cloned()
                            .unwrap_or(serde_json::Value::Null),
                    )
                    .map_err(decode_error)?;
                    let outcome = self
                        .member
                        .submit_assignment_result(MemberResultSubmission {
                            work_id: context.work_id.clone(),
                            assignment_id: context.assignment_id.clone(),
                            run_id: context.run_id.clone(),
                            author_agent_id: context.agent_instance_id.clone(),
                            runtime_owner: context.runtime_owner.clone(),
                            envelope,
                        })
                        .await?;
                    Ok(serde_json::to_value(outcome).map_err(json_error)?)
                }
                crate::connectors::TOOL_LIST_EMAIL_ACCOUNTS
                | crate::connectors::TOOL_SEARCH_EMAIL_METADATA
                | crate::connectors::TOOL_REQUEST_EMAIL_BODY
                | crate::connectors::TOOL_REQUEST_SEND_EMAIL => {
                    let connectors = self.connectors.as_ref().ok_or_else(|| {
                        AppError::invalid_input("tool", "connector service is unavailable")
                    })?;
                    connectors
                        .dispatch_host_tool(tool, context, arguments)
                        .await
                }
                other => Err(AppError::invalid_input(
                    "tool",
                    format!("unknown or unauthorized host tool '{other}'"),
                )),
            }
        })
    }
}

fn json_error(error: serde_json::Error) -> AppError {
    AppError::invalid_input("arguments", error.to_string())
}

fn decode_error(error: serde_json::Error) -> AppError {
    AppError::invalid_input("arguments", error.to_string())
}
