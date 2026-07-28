use thiserror::Error;

use crate::domain::work::WorkStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkAction {
    Queue,
    Start,
    Wait,
    Idle,
    Complete,
    Fail,
    Stop,
    Interrupt,
    Archive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("invalid Work transition from {from:?} using {action:?}")]
pub struct InvalidTransition {
    pub from: WorkStatus,
    pub action: WorkAction,
}

pub fn transition(
    current: WorkStatus,
    action: WorkAction,
) -> Result<WorkStatus, InvalidTransition> {
    let next = match (current, action) {
        (WorkStatus::Draft, WorkAction::Queue) => WorkStatus::Queued,
        (WorkStatus::Queued, WorkAction::Start) => WorkStatus::Running,
        (WorkStatus::Queued, WorkAction::Stop) => WorkStatus::Stopped,
        (WorkStatus::Queued, WorkAction::Interrupt) => WorkStatus::Interrupted,
        (WorkStatus::Running, WorkAction::Wait) => WorkStatus::Waiting,
        (WorkStatus::Running, WorkAction::Idle) => WorkStatus::Idle,
        (WorkStatus::Running, WorkAction::Complete) => WorkStatus::Completed,
        (WorkStatus::Running, WorkAction::Fail) => WorkStatus::Failed,
        (WorkStatus::Running, WorkAction::Stop) => WorkStatus::Stopped,
        (WorkStatus::Running, WorkAction::Interrupt) => WorkStatus::Interrupted,
        (WorkStatus::Waiting, WorkAction::Start) => WorkStatus::Running,
        (WorkStatus::Waiting, WorkAction::Idle) => WorkStatus::Idle,
        (WorkStatus::Waiting, WorkAction::Complete) => WorkStatus::Completed,
        (WorkStatus::Waiting, WorkAction::Fail) => WorkStatus::Failed,
        (WorkStatus::Waiting, WorkAction::Stop) => WorkStatus::Stopped,
        (WorkStatus::Waiting, WorkAction::Interrupt) => WorkStatus::Interrupted,
        (WorkStatus::Idle, WorkAction::Complete) => WorkStatus::Completed,
        (
            WorkStatus::Idle
            | WorkStatus::Completed
            | WorkStatus::Failed
            | WorkStatus::Stopped
            | WorkStatus::Interrupted,
            WorkAction::Queue,
        ) => WorkStatus::Queued,
        (
            WorkStatus::Draft
            | WorkStatus::Idle
            | WorkStatus::Completed
            | WorkStatus::Failed
            | WorkStatus::Stopped
            | WorkStatus::Interrupted,
            WorkAction::Archive,
        ) => WorkStatus::Archived,
        _ => {
            return Err(InvalidTransition {
                from: current,
                action,
            });
        }
    };

    Ok(next)
}

#[cfg(test)]
mod tests {
    use crate::domain::work::WorkStatus;

    use super::{WorkAction, transition};

    #[test]
    fn completed_work_can_start_a_new_run() {
        assert_eq!(
            transition(WorkStatus::Completed, WorkAction::Queue).unwrap(),
            WorkStatus::Queued
        );
    }

    #[test]
    fn idle_is_not_completion() {
        assert!(transition(WorkStatus::Idle, WorkAction::Complete).is_ok());
        assert_ne!(WorkStatus::Idle, WorkStatus::Completed);
    }

    #[test]
    fn archived_work_cannot_run() {
        assert!(transition(WorkStatus::Archived, WorkAction::Queue).is_err());
    }

    #[test]
    fn active_work_can_follow_the_execution_lifecycle() {
        assert_eq!(
            transition(WorkStatus::Draft, WorkAction::Queue).unwrap(),
            WorkStatus::Queued
        );
        assert_eq!(
            transition(WorkStatus::Queued, WorkAction::Start).unwrap(),
            WorkStatus::Running
        );
        assert_eq!(
            transition(WorkStatus::Running, WorkAction::Wait).unwrap(),
            WorkStatus::Waiting
        );
        assert_eq!(
            transition(WorkStatus::Waiting, WorkAction::Start).unwrap(),
            WorkStatus::Running
        );
        assert_eq!(
            transition(WorkStatus::Running, WorkAction::Idle).unwrap(),
            WorkStatus::Idle
        );
    }

    #[test]
    fn terminal_actions_are_available_during_execution() {
        for (action, expected) in [
            (WorkAction::Complete, WorkStatus::Completed),
            (WorkAction::Fail, WorkStatus::Failed),
            (WorkAction::Stop, WorkStatus::Stopped),
            (WorkAction::Interrupt, WorkStatus::Interrupted),
        ] {
            assert_eq!(transition(WorkStatus::Running, action).unwrap(), expected);
        }
    }

    #[test]
    fn non_active_work_can_be_archived() {
        for status in [
            WorkStatus::Draft,
            WorkStatus::Idle,
            WorkStatus::Completed,
            WorkStatus::Failed,
            WorkStatus::Stopped,
            WorkStatus::Interrupted,
        ] {
            assert_eq!(
                transition(status, WorkAction::Archive).unwrap(),
                WorkStatus::Archived
            );
        }
    }

    #[test]
    fn active_work_cannot_be_archived() {
        for status in [WorkStatus::Queued, WorkStatus::Running, WorkStatus::Waiting] {
            assert!(transition(status, WorkAction::Archive).is_err());
        }
    }
}
