pub mod artifact;
pub mod result;
pub mod validation;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::{
    assignment::repository::AssignmentRepository,
    domain::{
        assignment::{AssignmentKind, AssignmentStatus, AssignmentSummary},
        collaboration::{CompleteWorkDeliveryInput, ResultEnvelope},
        event::WorkEventPayload,
    },
    error::AppError,
    work::repository::WorkRepository,
};

use self::{
    artifact::admit_artifact,
    result::{RepairDecision, ResultSubmissionContext, repair_decision, validate_result},
    validation::admit_unverified_claim,
};

#[derive(Clone)]
pub struct DeliveryModule {
    pool: SqlitePool,
    assignments: AssignmentRepository,
    works: WorkRepository,
}

#[derive(Debug, Clone)]
pub struct WorkDeliverySubmission {
    pub lead_assignment_id: String,
    pub runtime_owner: String,
    pub input: CompleteWorkDeliveryInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkDeliveryReceipt {
    pub delivery_id: String,
    pub assignment: AssignmentSummary,
}

pub struct MemberResultSubmission {
    pub work_id: String,
    pub assignment_id: String,
    pub run_id: String,
    pub author_agent_id: String,
    pub runtime_owner: String,
    pub envelope: ResultEnvelope,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[allow(clippy::large_enum_variant)]
pub enum SubmitOutcome {
    Accepted { assignment: AssignmentSummary },
    RepairRequested { diagnostics: Vec<String> },
    Rejected { diagnostics: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeliveryReadModel {
    pub id: String,
    pub work_id: String,
    pub lead_assignment_id: String,
    pub summary: String,
    pub status: String,
    pub artifacts: Vec<String>,
    pub validations: Vec<ValidationReadModel>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReadModel {
    pub claim: String,
    pub verification_status: String,
    pub source_event_id: Option<String>,
}

impl DeliveryModule {
    pub fn new(pool: SqlitePool, assignments: AssignmentRepository, works: WorkRepository) -> Self {
        Self {
            pool,
            assignments,
            works,
        }
    }

    pub async fn complete_work(
        &self,
        submission: WorkDeliverySubmission,
    ) -> Result<WorkDeliveryReceipt, AppError> {
        let lead = self
            .assignments
            .get_assignment(&submission.lead_assignment_id)
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
        let summary = submission.input.summary.trim().to_owned();
        if summary.is_empty() || summary.len() > 8 * 1024 {
            return Err(AppError::invalid_input(
                "summary",
                "must contain between 1 and 8192 bytes",
            ));
        }
        let incomplete_children = self
            .assignments
            .list_for_work(&lead.work_id)
            .await?
            .into_iter()
            .filter(|assignment| {
                assignment.parent_assignment_id.as_deref() == Some(lead.id.as_str())
                    && assignment.status != AssignmentStatus::Completed
            })
            .map(|assignment| assignment.id)
            .collect::<Vec<_>>();
        if !incomplete_children.is_empty() {
            return Err(AppError::invalid_input(
                "delivery",
                format!(
                    "required assignments are incomplete: {}",
                    incomplete_children.join(", ")
                ),
            ));
        }
        let work = self
            .works
            .get(&lead.work_id)
            .await?
            .ok_or_else(|| AppError::work_not_found(&lead.work_id))?;
        let root = dunce::canonicalize(&work.summary.root_path).map_err(|source| {
            AppError::WorkspacePathResolution {
                path: work.summary.root_path.clone().into(),
                source,
            }
        })?;
        let artifacts = submission
            .input
            .artifacts
            .iter()
            .map(|path| admit_artifact(&root, path))
            .collect::<Result<Vec<_>, _>>()?;
        let validations = submission
            .input
            .validation
            .iter()
            .cloned()
            .filter_map(admit_unverified_claim)
            .collect::<Vec<_>>();
        let delivery_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let limitations = serde_json::to_string(&submission.input.limitations)
            .map_err(|error| AppError::Database(sqlx::Error::Encode(Box::new(error))))?;
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        sqlx::query("INSERT INTO work_deliveries (id, work_id, lead_assignment_id, summary, limitations_json, status, created_at) VALUES (?, ?, ?, ?, ?, 'pending', ?)")
            .bind(&delivery_id).bind(&lead.work_id).bind(&lead.id).bind(&summary).bind(limitations).bind(now)
            .execute(&mut *transaction).await?;
        for artifact in &artifacts {
            sqlx::query("INSERT INTO delivery_artifacts (id, delivery_id, workspace_path, size_bytes, admission_status, created_at) VALUES (?, ?, ?, ?, 'admitted', ?)")
                .bind(Uuid::new_v4().to_string()).bind(&delivery_id).bind(artifact.path.to_string_lossy().as_ref()).bind(artifact.size_bytes as i64).bind(now)
                .execute(&mut *transaction).await?;
        }
        for validation in &validations {
            sqlx::query("INSERT INTO delivery_validations (id, delivery_id, claim, source_event_id, verification_status, created_at) VALUES (?, ?, ?, ?, 'unverified', ?)")
                .bind(Uuid::new_v4().to_string()).bind(&delivery_id).bind(&validation.claim).bind(&validation.source_event_id).bind(now)
                .execute(&mut *transaction).await?;
        }
        transaction.commit().await?;

        let assignment = self
            .assignments
            .complete_by_assignment(&lead.id, &summary, &submission.runtime_owner)
            .await?;
        self.assignments
            .emit_collaboration_event(
                &lead.id,
                WorkEventPayload::WorkDeliveryCompleted {
                    summary: summary.clone(),
                    artifacts: submission.input.artifacts,
                    validation: submission.input.validation,
                    limitations: submission.input.limitations,
                },
            )
            .await?;
        sqlx::query("UPDATE work_deliveries SET status = 'valid', validated_at = ? WHERE id = ? AND status = 'pending'")
            .bind(Utc::now()).bind(&delivery_id).execute(&self.pool).await?;
        self.works.reproject_work_status(&lead.work_id).await?;
        Ok(WorkDeliveryReceipt {
            delivery_id,
            assignment,
        })
    }

    pub async fn submit_member_result(
        &self,
        submission: MemberResultSubmission,
    ) -> Result<SubmitOutcome, AppError> {
        let assignment = self
            .assignments
            .get_assignment(&submission.assignment_id)
            .await?
            .ok_or_else(|| AppError::invalid_input("assignmentId", "assignment not found"))?;
        if assignment.kind != AssignmentKind::Member
            || assignment.work_id != submission.work_id
            || assignment.assigned_agent_id != submission.author_agent_id
        {
            return Err(AppError::invalid_input(
                "submission",
                "member Result identity does not match the Assignment",
            ));
        }
        let active_run_matches = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM runs WHERE id = ? AND work_id = ? AND assignment_id = ? AND agent_instance_id = ? AND status IN ('queued', 'running', 'waiting'))",
        )
        .bind(&submission.run_id)
        .bind(&submission.work_id)
        .bind(&submission.assignment_id)
        .bind(&submission.author_agent_id)
        .fetch_one(&self.pool)
        .await?;
        if !active_run_matches {
            return Err(AppError::invalid_input(
                "runId",
                "Run is not the active Assignment attempt",
            ));
        }

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
                let work = self
                    .works
                    .get(&submission.work_id)
                    .await?
                    .ok_or_else(|| AppError::work_not_found(&submission.work_id))?;
                let root = dunce::canonicalize(&work.summary.root_path).map_err(|source| {
                    AppError::WorkspacePathResolution {
                        path: work.summary.root_path.clone().into(),
                        source,
                    }
                })?;
                for artifact in &valid.envelope.artifacts {
                    admit_artifact(&root, &artifact.path)?;
                }
                self.assignments
                    .record_result(
                        &submission.assignment_id,
                        &submission.author_agent_id,
                        &envelope_json,
                        "valid",
                        0,
                    )
                    .await?;
                let summary = valid.envelope.summary;
                let status = valid.envelope.status;
                let assignment = self
                    .assignments
                    .complete_by_assignment(
                        &submission.assignment_id,
                        &summary,
                        &submission.runtime_owner,
                    )
                    .await?;
                self.assignments
                    .emit_collaboration_event(
                        &submission.assignment_id,
                        WorkEventPayload::AssignmentResultSubmitted {
                            assignment_id: submission.assignment_id.clone(),
                            agent_instance_id: submission.author_agent_id,
                            status,
                            summary,
                        },
                    )
                    .await?;
                self.works
                    .reproject_work_status(&submission.work_id)
                    .await?;
                Ok(SubmitOutcome::Accepted { assignment })
            }
            Err(diagnostics) => {
                let prior = self
                    .assignments
                    .latest_repair_attempt(&submission.assignment_id)
                    .await?;
                match repair_decision(prior, diagnostics) {
                    RepairDecision::RequestRepair { diagnostics } => {
                        self.assignments
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
                        self.assignments
                            .record_result(
                                &submission.assignment_id,
                                &submission.author_agent_id,
                                &envelope_json,
                                "rejected",
                                prior,
                            )
                            .await?;
                        self.assignments
                            .emit_collaboration_event(
                                &submission.assignment_id,
                                WorkEventPayload::AssignmentResultRejected {
                                    assignment_id: submission.assignment_id.clone(),
                                    agent_instance_id: submission.author_agent_id,
                                    reason: diagnostics.join("; "),
                                },
                            )
                            .await?;
                        Ok(SubmitOutcome::Rejected { diagnostics })
                    }
                }
            }
        }
    }

    pub async fn inspect(&self, work_id: &str) -> Result<Option<DeliveryReadModel>, AppError> {
        let row = sqlx::query_as::<_, DeliveryRow>("SELECT id, work_id, lead_assignment_id, summary, status, created_at FROM work_deliveries WHERE work_id = ? ORDER BY created_at DESC LIMIT 1")
            .bind(work_id).fetch_optional(&self.pool).await?;
        let Some(row) = row else { return Ok(None) };
        let artifacts = sqlx::query_scalar::<_, String>("SELECT workspace_path FROM delivery_artifacts WHERE delivery_id = ? ORDER BY workspace_path")
            .bind(&row.id).fetch_all(&self.pool).await?;
        let validations = sqlx::query_as::<_, ValidationReadModel>("SELECT claim, verification_status, source_event_id FROM delivery_validations WHERE delivery_id = ? ORDER BY created_at, id")
            .bind(&row.id).fetch_all(&self.pool).await?;
        Ok(Some(DeliveryReadModel {
            id: row.id,
            work_id: row.work_id,
            lead_assignment_id: row.lead_assignment_id,
            summary: row.summary,
            status: row.status,
            artifacts,
            validations,
            created_at: row.created_at,
        }))
    }
}

#[derive(FromRow)]
struct DeliveryRow {
    id: String,
    work_id: String,
    lead_assignment_id: String,
    summary: String,
    status: String,
    created_at: DateTime<Utc>,
}
