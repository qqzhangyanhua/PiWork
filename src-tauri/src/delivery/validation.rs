#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationAdmission {
    pub claim: String,
    pub source_event_id: Option<String>,
    pub verified: bool,
}

pub fn admit_unverified_claim(claim: String) -> Option<ValidationAdmission> {
    let claim = claim.trim().to_owned();
    (!claim.is_empty()).then_some(ValidationAdmission {
        claim,
        source_event_id: None,
        verified: false,
    })
}
