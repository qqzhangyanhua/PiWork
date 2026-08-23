use crate::domain::work::{StartWorkOutput, WorkDetail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkInput {
    pub instruction: String,
    pub referenced_files: Vec<String>,
    pub resource_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmissionReceipt {
    pub output: StartWorkOutput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionCommand {
    Stop,
    Steer(WorkInput),
    InterruptAndReplace(WorkInput),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionOutcome {
    Stopped,
    Submitted,
    Replaced,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionReceipt {
    pub outcome: ExecutionOutcome,
    pub work: WorkDetail,
    pub submission: Option<StartWorkOutput>,
}
