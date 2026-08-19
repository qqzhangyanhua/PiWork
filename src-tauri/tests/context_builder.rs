use chrono::{TimeZone, Utc};
use piwork_lib::{
    collaboration::ledger::project_work_ledger,
    domain::{
        event::{WorkEventEnvelope, WorkEventPayload},
        work::{PermissionMode, WorkStatus, WorkSummary},
    },
};

fn work() -> WorkSummary {
    WorkSummary {
        id: "work-1".into(),
        title: "Ship".into(),
        goal: "Build the foundation".into(),
        root_path: "/workspace".into(),
        permission_mode: PermissionMode::Balanced,
        status: WorkStatus::Running,
        created_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        updated_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
    }
}

fn event(id: &str, sequence: u32, payload: WorkEventPayload) -> WorkEventEnvelope {
    WorkEventEnvelope {
        version: 2,
        event_id: Some(id.to_owned()),
        work_id: "work-1".into(),
        run_id: Some("run-1".into()),
        turn_id: None,
        session_id: None,
        agent_id: None,
        assignment_id: None,
        causation_id: None,
        correlation_id: None,
        sequence,
        occurred_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        payload,
    }
}

#[test]
fn ledger_projects_goal_and_assignment_states() {
    let events = vec![
        event(
            "e1",
            1,
            WorkEventPayload::AssignmentQueued {
                assignment_id: "a1".into(),
                assigned_agent_id: "lead".into(),
                title: "Research".into(),
                priority: 10,
            },
        ),
        event(
            "e2",
            2,
            WorkEventPayload::AssignmentStarted {
                assignment_id: "a1".into(),
                agent_instance_id: "lead".into(),
                agent_session_id: "s1".into(),
                run_id: "run-1".into(),
            },
        ),
        event(
            "e3",
            3,
            WorkEventPayload::AssignmentWaiting {
                assignment_id: "a1".into(),
                agent_instance_id: "lead".into(),
                agent_session_id: "s1".into(),
                reason: "awaiting review".into(),
            },
        ),
        event(
            "e4",
            4,
            WorkEventPayload::AssignmentCompleted {
                assignment_id: "a1".into(),
                agent_instance_id: "lead".into(),
                agent_session_id: "s1".into(),
                result_summary: "done".into(),
            },
        ),
    ];

    let ledger = project_work_ledger(&work(), &events);
    assert_eq!(ledger.goal, "Build the foundation");
    assert!(ledger.active_assignments.is_empty());
    assert!(ledger.waiting_assignments.is_empty());
    assert_eq!(ledger.completed_assignments, vec!["a1"]);
    assert!(ledger.open_questions.contains(&"awaiting review".to_owned()));
}

#[test]
fn ledger_ignores_stale_revisions_and_duplicate_events() {
    let events = vec![
        event(
            "e1",
            1,
            WorkEventPayload::WorkPlanUpdated {
                plan_id: "plan".into(),
                revision: 2,
                text: "Step B".into(),
            },
        ),
        event(
            "e2",
            2,
            WorkEventPayload::WorkPlanUpdated {
                plan_id: "plan".into(),
                revision: 1,
                text: "Step A (stale)".into(),
            },
        ),
        // Duplicate of e1 must be idempotent.
        event(
            "e1",
            2,
            WorkEventPayload::WorkPlanUpdated {
                plan_id: "plan".into(),
                revision: 2,
                text: "Step B".into(),
            },
        ),
        event(
            "e3",
            3,
            WorkEventPayload::WorkDecisionRecorded {
                decision_id: "d1".into(),
                summary: "Use SQLite".into(),
                version: 3,
            },
        ),
        event(
            "e4",
            4,
            WorkEventPayload::WorkDecisionRecorded {
                decision_id: "d1".into(),
                summary: "Use Postgres (stale)".into(),
                version: 2,
            },
        ),
    ];

    let ledger = project_work_ledger(&work(), &events);
    assert_eq!(ledger.plan.len(), 1);
    assert_eq!(ledger.plan[0].title, "Step B");
    assert_eq!(ledger.decisions.len(), 1);
    assert_eq!(ledger.decisions[0].summary, "Use SQLite");
    assert_eq!(ledger.decisions[0].version, 3);
}

#[test]
fn ledger_accumulates_artifacts_validation_and_last_delivery() {
    let events = vec![
        event(
            "e1",
            1,
            WorkEventPayload::ArtifactProduced {
                path: "src/main.rs".into(),
            },
        ),
        event(
            "e2",
            2,
            WorkEventPayload::ValidationProduced {
                command: "cargo test".into(),
                success: true,
                summary: "passed".into(),
            },
        ),
        event(
            "e3",
            3,
            WorkEventPayload::WorkDeliveryCompleted {
                summary: "Delivered".into(),
                artifacts: vec!["notes.md".into()],
                validation: vec!["cargo test".into()],
                limitations: vec![],
            },
        ),
    ];

    let ledger = project_work_ledger(&work(), &events);
    assert_eq!(ledger.artifacts, vec!["src/main.rs", "notes.md"]);
    assert_eq!(ledger.validation, vec!["cargo test"]);
    assert_eq!(ledger.last_delivery.as_deref(), Some("Delivered"));
}
