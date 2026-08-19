//! Result Envelope validation and the one-shot repair policy.
//!
//! The validator enforces the fixed contract from design §7.4: every finding,
//! evidence, artifact, and validation carries author/assignment provenance, and
//! the whole envelope stays inside the fixed byte/array bounds. Provenance that
//! must be checked against durable state (event/resource/path ownership within
//! the Work) is verified by the member-tool service in `collaboration::tools`.

use crate::domain::collaboration::{
    DelegationRequest, MemoryCandidateInput, ResultArtifact, ResultEnvelope, ResultEvidence,
    ResultFinding, ResultValidation,
};

pub const MAX_SUMMARY_BYTES: usize = 8 * 1024;
pub const MAX_TEXT_BYTES: usize = 16 * 1024;
pub const MAX_ARRAY_ITEMS: usize = 128;
pub const MAX_ENVELOPE_BYTES: usize = 256 * 1024;

/// The submitter identity and the Assignment/Run the result belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultSubmissionContext {
    pub work_id: String,
    pub assignment_id: String,
    pub run_id: String,
    pub author_agent_id: String,
}

#[derive(Debug, Clone)]
pub struct ValidatedResultEnvelope {
    pub envelope: ResultEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepairDecision {
    RequestRepair { diagnostics: Vec<String> },
    Reject { diagnostics: Vec<String> },
}

/// Validates the structural contract. Returns a machine-readable diagnostic
/// list on failure; success means the envelope may be persisted as valid.
pub fn validate_result(
    context: &ResultSubmissionContext,
    envelope: ResultEnvelope,
) -> Result<ValidatedResultEnvelope, Vec<String>> {
    let mut diagnostics = Vec::new();

    if let Ok(bytes) = serde_json::to_vec(&envelope) {
        if bytes.len() > MAX_ENVELOPE_BYTES {
            diagnostics.push("envelope exceeds 256 KiB UTF-8".to_owned());
        }
    }

    check_summary(&mut diagnostics, &envelope.summary);
    check_array(&mut diagnostics, "findings", envelope.findings.len());
    check_array(&mut diagnostics, "evidence", envelope.evidence.len());
    check_array(&mut diagnostics, "artifacts", envelope.artifacts.len());
    check_array(&mut diagnostics, "validation", envelope.validation.len());
    check_array(
        &mut diagnostics,
        "decisionsRecommended",
        envelope.decisions_recommended.len(),
    );
    check_array(&mut diagnostics, "uncertainties", envelope.uncertainties.len());
    check_array(
        &mut diagnostics,
        "delegationRequests",
        envelope.delegation_requests.len(),
    );
    check_array(
        &mut diagnostics,
        "memoryCandidates",
        envelope.memory_candidates.len(),
    );
    check_array(&mut diagnostics, "limitations", envelope.limitations.len());

    for (index, finding) in envelope.findings.iter().enumerate() {
        validate_finding(&mut diagnostics, context, finding, index);
    }
    for (index, evidence) in envelope.evidence.iter().enumerate() {
        validate_evidence(&mut diagnostics, context, evidence, index);
    }
    for (index, artifact) in envelope.artifacts.iter().enumerate() {
        validate_artifact(&mut diagnostics, context, artifact, index);
    }
    for (index, validation) in envelope.validation.iter().enumerate() {
        validate_validation(&mut diagnostics, context, validation, index);
    }
    for (index, request) in envelope.delegation_requests.iter().enumerate() {
        validate_delegation(&mut diagnostics, request, index);
    }
    for (index, candidate) in envelope.memory_candidates.iter().enumerate() {
        validate_candidate(&mut diagnostics, candidate, index);
    }
    for key in envelope.extensions.keys() {
        if key.trim().is_empty() {
            diagnostics.push("extension keys must be non-empty capability pack ids".to_owned());
            break;
        }
    }

    if diagnostics.is_empty() {
        Ok(ValidatedResultEnvelope { envelope })
    } else {
        Err(diagnostics)
    }
}

/// First invalid submission requests a repair; a second invalid submission is
/// rejected and escalates to the Lead. `prior_repair_attempts` is read from the
/// durable `assignment_results` repair counter.
pub fn repair_decision(prior_repair_attempts: u32, diagnostics: Vec<String>) -> RepairDecision {
    if prior_repair_attempts == 0 {
        RepairDecision::RequestRepair { diagnostics }
    } else {
        RepairDecision::Reject { diagnostics }
    }
}

fn validate_finding(
    diagnostics: &mut Vec<String>,
    context: &ResultSubmissionContext,
    finding: &ResultFinding,
    index: usize,
) {
    check_text(diagnostics, &finding.title, &format!("findings[{index}].title"));
    check_text(diagnostics, &finding.detail, &format!("findings[{index}].detail"));
    check_author(diagnostics, context, &finding.author_agent_id, index, "finding");
    check_assignment(diagnostics, context, &finding.assignment_id, index, "finding");
}

fn validate_evidence(
    diagnostics: &mut Vec<String>,
    context: &ResultSubmissionContext,
    evidence: &ResultEvidence,
    index: usize,
) {
    check_text(
        diagnostics,
        &evidence.description,
        &format!("evidence[{index}].description"),
    );
    check_author(diagnostics, context, &evidence.author_agent_id, index, "evidence");
    check_assignment(diagnostics, context, &evidence.assignment_id, index, "evidence");
    let has_source = evidence.source_event_id.is_some()
        || evidence.source_resource_id.is_some()
        || evidence.source_path.is_some();
    if !has_source {
        diagnostics.push(format!(
            "evidence[{index}] must reference a source event, resource, or path"
        ));
    }
}

fn validate_artifact(
    diagnostics: &mut Vec<String>,
    context: &ResultSubmissionContext,
    artifact: &ResultArtifact,
    index: usize,
) {
    check_text(diagnostics, &artifact.path, &format!("artifacts[{index}].path"));
    check_author(diagnostics, context, &artifact.author_agent_id, index, "artifact");
    check_assignment(diagnostics, context, &artifact.assignment_id, index, "artifact");
}

fn validate_validation(
    diagnostics: &mut Vec<String>,
    context: &ResultSubmissionContext,
    validation: &ResultValidation,
    index: usize,
) {
    check_text(
        diagnostics,
        &validation.command,
        &format!("validation[{index}].command"),
    );
    check_author(diagnostics, context, &validation.author_agent_id, index, "validation");
    check_assignment(diagnostics, context, &validation.assignment_id, index, "validation");
}

fn validate_delegation(
    diagnostics: &mut Vec<String>,
    request: &DelegationRequest,
    index: usize,
) {
    check_text(
        diagnostics,
        &request.reason,
        &format!("delegationRequests[{index}].reason"),
    );
    check_text(
        diagnostics,
        &request.expected_output,
        &format!("delegationRequests[{index}].expectedOutput"),
    );
}

fn validate_candidate(
    diagnostics: &mut Vec<String>,
    candidate: &MemoryCandidateInput,
    index: usize,
) {
    check_text(
        diagnostics,
        &candidate.content,
        &format!("memoryCandidates[{index}].content"),
    );
    check_text(
        diagnostics,
        &candidate.reason,
        &format!("memoryCandidates[{index}].reason"),
    );
}

fn check_summary(diagnostics: &mut Vec<String>, summary: &str) {
    if summary.trim().is_empty() {
        diagnostics.push("summary must not be empty".to_owned());
    } else if summary.len() > MAX_SUMMARY_BYTES {
        diagnostics.push("summary exceeds 8 KiB".to_owned());
    }
}

fn check_text(diagnostics: &mut Vec<String>, text: &str, field: &str) {
    if text.trim().is_empty() {
        diagnostics.push(format!("{field} must not be empty"));
    } else if text.len() > MAX_TEXT_BYTES {
        diagnostics.push(format!("{field} exceeds 16 KiB"));
    }
}

fn check_array(diagnostics: &mut Vec<String>, field: &str, len: usize) {
    if len > MAX_ARRAY_ITEMS {
        diagnostics.push(format!("{field} exceeds {MAX_ARRAY_ITEMS} items"));
    }
}

fn check_author(
    diagnostics: &mut Vec<String>,
    context: &ResultSubmissionContext,
    author: &str,
    index: usize,
    kind: &str,
) {
    if author != context.author_agent_id {
        diagnostics.push(format!(
            "{kind}[{index}] author {author:?} does not match the submitter"
        ));
    }
}

fn check_assignment(
    diagnostics: &mut Vec<String>,
    context: &ResultSubmissionContext,
    assignment_id: &str,
    index: usize,
    kind: &str,
) {
    if assignment_id != context.assignment_id {
        diagnostics.push(format!(
            "{kind}[{index}] assignment {assignment_id:?} does not match the submission"
        ));
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::domain::collaboration::ResultStatus;

    fn context() -> ResultSubmissionContext {
        ResultSubmissionContext {
            work_id: "work-1".into(),
            assignment_id: "assignment-1".into(),
            run_id: "run-1".into(),
            author_agent_id: "agent-1".into(),
        }
    }

    fn valid_envelope() -> ResultEnvelope {
        ResultEnvelope {
            status: ResultStatus::Completed,
            summary: "Investigated the queue".into(),
            findings: vec![ResultFinding {
                title: "Eight invariants".into(),
                detail: "Derived from the queue".into(),
                confidence: Some("high".into()),
                source_event_id: Some("event-1".into()),
                author_agent_id: "agent-1".into(),
                assignment_id: "assignment-1".into(),
                occurred_at: None,
            }],
            evidence: vec![ResultEvidence {
                description: "Source inspection".into(),
                source_event_id: None,
                source_resource_id: None,
                source_path: Some("src/assignment/queue.rs".into()),
                author_agent_id: "agent-1".into(),
                assignment_id: "assignment-1".into(),
                occurred_at: None,
            }],
            artifacts: vec![ResultArtifact {
                path: "notes.md".into(),
                description: "Notes".into(),
                produced_by_command: None,
                author_agent_id: "agent-1".into(),
                assignment_id: "assignment-1".into(),
            }],
            validation: vec![ResultValidation {
                command: "cargo test".into(),
                success: true,
                summary: "passed".into(),
                author_agent_id: "agent-1".into(),
                assignment_id: "assignment-1".into(),
                occurred_at: None,
            }],
            decisions_recommended: vec![],
            uncertainties: vec![],
            delegation_requests: vec![],
            memory_candidates: vec![],
            limitations: vec![],
            extensions: BTreeMap::new(),
        }
    }

    #[test]
    fn valid_envelope_passes() {
        assert!(validate_result(&context(), valid_envelope()).is_ok());
    }

    #[test]
    fn missing_provenance_is_rejected() {
        let mut envelope = valid_envelope();
        envelope.evidence[0].source_path = None;
        let diagnostics = validate_result(&context(), envelope).unwrap_err();
        assert!(diagnostics.iter().any(|d| d.contains("must reference a source")));
    }

    #[test]
    fn wrong_author_and_assignment_are_rejected() {
        let mut envelope = valid_envelope();
        envelope.findings[0].author_agent_id = "agent-2".into();
        let diagnostics = validate_result(&context(), envelope).unwrap_err();
        assert!(diagnostics.iter().any(|d| d.contains("does not match the submitter")));

        let mut envelope = valid_envelope();
        envelope.artifacts[0].assignment_id = "assignment-2".into();
        let diagnostics = validate_result(&context(), envelope).unwrap_err();
        assert!(diagnostics.iter().any(|d| d.contains("does not match the submission")));
    }

    #[test]
    fn oversized_summary_is_rejected() {
        let mut envelope = valid_envelope();
        envelope.summary = "x".repeat(MAX_SUMMARY_BYTES + 1);
        let diagnostics = validate_result(&context(), envelope).unwrap_err();
        assert!(diagnostics.iter().any(|d| d.contains("exceeds 8 KiB")));
    }

    #[test]
    fn repair_policy_escalates_on_the_second_failure() {
        let diagnostics = vec!["bad".to_owned()];
        assert_eq!(
            repair_decision(0, diagnostics.clone()),
            RepairDecision::RequestRepair { diagnostics: diagnostics.clone() }
        );
        assert_eq!(
            repair_decision(1, diagnostics.clone()),
            RepairDecision::Reject { diagnostics }
        );
    }
}
