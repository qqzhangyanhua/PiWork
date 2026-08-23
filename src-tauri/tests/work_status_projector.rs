use piwork_lib::{
    domain::{
        assignment::AssignmentStatus,
        work::{RunStatus, WorkStatus},
    },
    work::projector::{WorkControlFact, WorkExecutionFacts, project_work_status},
};

fn facts() -> WorkExecutionFacts {
    WorkExecutionFacts::default()
}

#[test]
fn active_run_and_lead_waiting_take_precedence_over_member_completion() {
    let mut input = facts();
    input.lead_assignment = Some(AssignmentStatus::Waiting);
    input.member_assignments = vec![AssignmentStatus::Completed];
    input.run_statuses = vec![RunStatus::Running];
    assert_eq!(project_work_status(input.clone()), WorkStatus::Running);

    input.run_statuses.clear();
    assert_eq!(project_work_status(input), WorkStatus::Waiting);
}

#[test]
fn a_completed_lead_without_delivery_is_idle_not_completed() {
    let mut input = facts();
    input.lead_assignment = Some(AssignmentStatus::Completed);
    input.member_assignments = vec![AssignmentStatus::Completed];
    assert_eq!(project_work_status(input), WorkStatus::Idle);
}

#[test]
fn only_a_valid_delivery_projects_completed() {
    let mut input = facts();
    input.lead_assignment = Some(AssignmentStatus::Completed);
    input.valid_delivery = true;
    assert_eq!(project_work_status(input), WorkStatus::Completed);
}

#[test]
fn recovery_and_required_dead_letter_are_durable_failure_facts() {
    let mut input = facts();
    input.lead_assignment = Some(AssignmentStatus::RecoveryConfirmationRequired);
    assert_eq!(project_work_status(input.clone()), WorkStatus::Interrupted);

    input.lead_assignment = Some(AssignmentStatus::Waiting);
    input.member_assignments = vec![AssignmentStatus::DeadLetter];
    assert_eq!(project_work_status(input), WorkStatus::Failed);
}

#[test]
fn a_new_queue_supersedes_an_old_stop_control_fact() {
    let mut input = facts();
    input.control = Some(WorkControlFact::Stopped);
    assert_eq!(project_work_status(input.clone()), WorkStatus::Stopped);

    input.lead_assignment = Some(AssignmentStatus::Queued);
    assert_eq!(project_work_status(input), WorkStatus::Queued);
}
