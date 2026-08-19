use std::collections::BTreeMap;

use piwork_lib::{
    collaboration::result::{repair_decision, validate_result, RepairDecision, ResultSubmissionContext},
    domain::collaboration::{ResultEnvelope, ResultStatus},
};

fn context() -> ResultSubmissionContext {
    ResultSubmissionContext {
        work_id: "work-1".into(),
        assignment_id: "assignment-1".into(),
        run_id: "run-1".into(),
        author_agent_id: "agent-1".into(),
    }
}

fn minimal_envelope() -> ResultEnvelope {
    ResultEnvelope {
        status: ResultStatus::Completed,
        summary: "Done".into(),
        findings: vec![],
        evidence: vec![],
        artifacts: vec![],
        validation: vec![],
        decisions_recommended: vec![],
        uncertainties: vec![],
        delegation_requests: vec![],
        memory_candidates: vec![],
        limitations: vec![],
        extensions: BTreeMap::new(),
    }
}

#[test]
fn result_validation_and_repair_escalation_are_public() {
    assert!(validate_result(&context(), minimal_envelope()).is_ok());

    let diagnostics = vec!["summary must not be empty".to_owned()];
    match repair_decision(0, diagnostics.clone()) {
        RepairDecision::RequestRepair { diagnostics: d } => assert_eq!(d, diagnostics),
        _ => panic!("first failure must request a repair"),
    }
    match repair_decision(1, diagnostics.clone()) {
        RepairDecision::Reject { diagnostics: d } => assert_eq!(d, diagnostics),
        _ => panic!("second failure must reject"),
    }
}
