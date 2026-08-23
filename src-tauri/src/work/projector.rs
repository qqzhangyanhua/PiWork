use crate::domain::{
    assignment::AssignmentStatus,
    work::{RunStatus, WorkStatus},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkControlFact {
    Stopped,
    Interrupted,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkExecutionFacts {
    pub archived: bool,
    pub control: Option<WorkControlFact>,
    pub lead_assignment: Option<AssignmentStatus>,
    pub member_assignments: Vec<AssignmentStatus>,
    pub run_statuses: Vec<RunStatus>,
    pub valid_delivery: bool,
}

pub fn project_work_status(facts: WorkExecutionFacts) -> WorkStatus {
    if facts.archived {
        return WorkStatus::Archived;
    }
    if facts.valid_delivery {
        return WorkStatus::Completed;
    }

    let assignments = facts
        .lead_assignment
        .iter()
        .chain(facts.member_assignments.iter());
    if assignments
        .clone()
        .any(|status| *status == AssignmentStatus::DeadLetter)
    {
        return WorkStatus::Failed;
    }
    if assignments
        .clone()
        .any(|status| *status == AssignmentStatus::RecoveryConfirmationRequired)
    {
        return WorkStatus::Interrupted;
    }

    if facts
        .run_statuses
        .iter()
        .any(|status| *status == RunStatus::Running)
        || assignments.clone().any(|status| {
            matches!(
                status,
                AssignmentStatus::Claimed | AssignmentStatus::Running
            )
        })
    {
        return WorkStatus::Running;
    }
    if facts
        .run_statuses
        .iter()
        .any(|status| *status == RunStatus::Waiting)
        || facts.lead_assignment == Some(AssignmentStatus::Waiting)
    {
        return WorkStatus::Waiting;
    }
    if facts
        .run_statuses
        .iter()
        .any(|status| *status == RunStatus::Queued)
        || assignments
            .clone()
            .any(|status| *status == AssignmentStatus::Queued)
    {
        return WorkStatus::Queued;
    }

    if facts.lead_assignment.is_none() {
        if facts
            .run_statuses
            .iter()
            .any(|status| *status == RunStatus::Failed)
        {
            return WorkStatus::Failed;
        }
        if facts
            .run_statuses
            .iter()
            .any(|status| *status == RunStatus::Interrupted)
        {
            return WorkStatus::Interrupted;
        }
        if facts
            .run_statuses
            .iter()
            .any(|status| *status == RunStatus::Stopped)
        {
            return WorkStatus::Stopped;
        }
    }

    match facts.lead_assignment {
        Some(AssignmentStatus::Completed) => WorkStatus::Idle,
        Some(AssignmentStatus::Failed | AssignmentStatus::DeadLetter) => WorkStatus::Failed,
        Some(AssignmentStatus::Interrupted | AssignmentStatus::RecoveryConfirmationRequired) => {
            WorkStatus::Interrupted
        }
        Some(AssignmentStatus::Cancelled) => WorkStatus::Stopped,
        _ => match facts.control {
            Some(WorkControlFact::Stopped) => WorkStatus::Stopped,
            Some(WorkControlFact::Interrupted) => WorkStatus::Interrupted,
            None if facts.lead_assignment.is_some() || !facts.member_assignments.is_empty() => {
                WorkStatus::Idle
            }
            None => WorkStatus::Draft,
        },
    }
}
