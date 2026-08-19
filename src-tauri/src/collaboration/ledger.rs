//! Work Ledger projection: the shared Work state (goal, plan, decisions,
//! constraints, assignment states, artifacts, validation, open questions, and
//! last delivery) rebuilt from append-only WorkEvents. The Ledger is a cache —
//! events remain the source of truth and a later read can always rebuild.

use std::collections::{BTreeMap, HashSet};

use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    domain::{
        collaboration::{LedgerDecision, LedgerPlanStep, LedgerPlanStepStatus, WorkLedger},
        event::{WorkEventEnvelope, WorkEventPayload},
        work::WorkSummary,
    },
    error::AppError,
};

/// Folds sequence-ordered events into a Work Ledger. Duplicate event ids are
/// ignored, and a stale plan revision or decision version never overwrites a
/// newer one.
pub fn project_work_ledger(work: &WorkSummary, events: &[WorkEventEnvelope]) -> WorkLedger {
    let mut plan_versions: BTreeMap<String, u32> = BTreeMap::new();
    let mut plan_steps: Vec<LedgerPlanStep> = Vec::new();
    let mut decision_versions: BTreeMap<String, u32> = BTreeMap::new();
    let mut decisions: Vec<LedgerDecision> = Vec::new();
    let mut active: Vec<String> = Vec::new();
    let mut waiting: Vec<String> = Vec::new();
    let mut completed: Vec<String> = Vec::new();
    let mut artifacts: Vec<String> = Vec::new();
    let mut validation: Vec<String> = Vec::new();
    let mut open_questions: Vec<String> = Vec::new();
    let mut last_delivery: Option<String> = None;
    let mut seen_events: HashSet<String> = HashSet::new();

    for event in events {
        if let Some(event_id) = &event.event_id {
            if !seen_events.insert(event_id.clone()) {
                continue;
            }
        }
        match &event.payload {
            WorkEventPayload::AssignmentQueued { assignment_id, .. }
            | WorkEventPayload::AssignmentClaimed { assignment_id, .. }
            | WorkEventPayload::AssignmentStarted { assignment_id, .. } => {
                upsert(&mut active, assignment_id);
            }
            WorkEventPayload::AssignmentWaiting {
                assignment_id,
                reason,
                ..
            } => {
                remove(&mut active, assignment_id);
                upsert(&mut waiting, assignment_id);
                if !open_questions.contains(reason) {
                    open_questions.push(reason.clone());
                }
            }
            WorkEventPayload::AssignmentCompleted { assignment_id, .. }
            | WorkEventPayload::AssignmentCancelled { assignment_id, .. }
            | WorkEventPayload::AssignmentFailed { assignment_id, .. }
            | WorkEventPayload::AssignmentDeadLettered { assignment_id, .. }
            | WorkEventPayload::AssignmentInterrupted { assignment_id, .. } => {
                remove(&mut active, assignment_id);
                remove(&mut waiting, assignment_id);
                upsert(&mut completed, assignment_id);
            }
            WorkEventPayload::WorkPlanUpdated {
                plan_id,
                revision,
                text,
            } => {
                let prior = plan_versions.get(plan_id).copied().unwrap_or(0);
                if *revision < prior {
                    continue;
                }
                plan_versions.insert(plan_id.clone(), *revision);
                replace_step(&mut plan_steps, plan_id, text);
            }
            WorkEventPayload::WorkDecisionRecorded {
                decision_id,
                summary,
                version,
            } => {
                let prior = decision_versions.get(decision_id).copied().unwrap_or(0);
                if *version < prior {
                    continue;
                }
                decision_versions.insert(decision_id.clone(), *version);
                replace_decision(&mut decisions, decision_id, summary, *version);
            }
            WorkEventPayload::ArtifactProduced { path } => {
                if !artifacts.contains(path) {
                    artifacts.push(path.clone());
                }
            }
            WorkEventPayload::ValidationProduced { command, .. } => {
                if !validation.contains(command) {
                    validation.push(command.clone());
                }
            }
            WorkEventPayload::RunCompleted {
                artifacts: run_artifacts,
                validation: run_validation,
                ..
            } => {
                for path in run_artifacts {
                    if !artifacts.contains(path) {
                        artifacts.push(path.clone());
                    }
                }
                for command in run_validation {
                    if !validation.contains(command) {
                        validation.push(command.clone());
                    }
                }
            }
            WorkEventPayload::WorkDeliveryCompleted {
                summary,
                artifacts: delivery_artifacts,
                ..
            } => {
                last_delivery = Some(summary.clone());
                for path in delivery_artifacts {
                    if !artifacts.contains(path) {
                        artifacts.push(path.clone());
                    }
                }
            }
            _ => {}
        }
    }

    WorkLedger {
        goal: work.goal.clone(),
        plan: plan_steps,
        decisions,
        constraints: Vec::new(),
        permissions: vec![permission_label(work.permission_mode)],
        active_assignments: active,
        waiting_assignments: waiting,
        completed_assignments: completed,
        artifacts,
        validation,
        open_questions,
        last_delivery,
    }
}

fn upsert(list: &mut Vec<String>, value: &str) {
    if !list.iter().any(|item| item == value) {
        list.push(value.to_owned());
    }
}

fn remove(list: &mut Vec<String>, value: &str) {
    list.retain(|item| item != value);
}

fn replace_step(steps: &mut Vec<LedgerPlanStep>, id: &str, title: &str) {
    if let Some(step) = steps.iter_mut().find(|step| step.id == id) {
        step.title = title.to_owned();
        step.status = LedgerPlanStepStatus::InProgress;
    } else {
        steps.push(LedgerPlanStep {
            id: id.to_owned(),
            title: title.to_owned(),
            status: LedgerPlanStepStatus::InProgress,
        });
    }
}

fn replace_decision(decisions: &mut Vec<LedgerDecision>, id: &str, summary: &str, version: u32) {
    if let Some(decision) = decisions.iter_mut().find(|decision| decision.id == id) {
        decision.summary = summary.to_owned();
        decision.version = version;
    } else {
        decisions.push(LedgerDecision {
            id: id.to_owned(),
            summary: summary.to_owned(),
            version,
        });
    }
}

fn permission_label(mode: crate::domain::work::PermissionMode) -> String {
    serde_json::to_string(&mode)
        .expect("permission mode serializes")
        .trim_matches('"')
        .to_owned()
}

/// Persists the Ledger projection as a Work cache row. A later read can always
/// rebuild from events when the cache is stale or missing.
#[derive(Clone)]
pub struct WorkLedgerRepository {
    pool: SqlitePool,
}

impl WorkLedgerRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn rebuild_and_cache(
        &self,
        work: &WorkSummary,
        events: &[WorkEventEnvelope],
    ) -> Result<WorkLedger, AppError> {
        let ledger = project_work_ledger(work, events);
        let source_sequence = events.iter().map(|event| event.sequence).max().unwrap_or(0);
        let now = Utc::now();
        let revision: i64 = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT COALESCE(MAX(revision), 0) FROM work_memory WHERE work_id = ?",
        )
        .bind(&work.id)
        .fetch_one(&self.pool)
        .await?
        .unwrap_or(0);
        let revision = revision
            .checked_add(1)
            .ok_or_else(|| AppError::invalid_input("revision", "ledger revision limit exceeded"))?;
        let ledger_json = serde_json::to_string(&ledger)
            .map_err(|error| AppError::Database(sqlx::Error::Encode(Box::new(error))))?;
        sqlx::query(
            "INSERT INTO work_memory (id, work_id, revision, ledger_json, source_sequence, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&work.id)
        .bind(revision)
        .bind(&ledger_json)
        .bind(i64::from(source_sequence))
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(ledger)
    }
}
