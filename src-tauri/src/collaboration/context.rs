//! Fixed-order, bounded Context Builder for Lead and Member assignments.
//!
//! The eight sections are assembled in the exact order of design §8.2 and the
//! adapter must consume them without reordering. Each section carries its own
//! stable title and source ids so provenance is auditable, and the renderer
//! enforces a global character budget by truncating from the final sections.

use crate::domain::{
    agent::{AgentDefinitionSummary, CapabilityPackSummary, RoleKind},
    assignment::AssignmentSummary,
    collaboration::ResultEnvelope,
    work::WorkSummary,
};

pub const DEFAULT_CONTEXT_BUDGET_CHARS: usize = 48_000;
pub const DEFAULT_SECTION_BUDGET_CHARS: usize = 8_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextSectionKind {
    BaseProtocol,
    AgentDefinition,
    CapabilityPack,
    AgentMemory,
    WorkBrief,
    AssignmentPacket,
    DependencyResults,
    ExplicitContext,
}

impl ContextSectionKind {
    pub const ALL: [ContextSectionKind; 8] = [
        ContextSectionKind::BaseProtocol,
        ContextSectionKind::AgentDefinition,
        ContextSectionKind::CapabilityPack,
        ContextSectionKind::AgentMemory,
        ContextSectionKind::WorkBrief,
        ContextSectionKind::AssignmentPacket,
        ContextSectionKind::DependencyResults,
        ContextSectionKind::ExplicitContext,
    ];

    fn title(self) -> &'static str {
        match self {
            ContextSectionKind::BaseProtocol => "PiWork Base Protocol",
            ContextSectionKind::AgentDefinition => "Agent Definition",
            ContextSectionKind::CapabilityPack => "Capability Pack",
            ContextSectionKind::AgentMemory => "Agent Core Memory",
            ContextSectionKind::WorkBrief => "Work Brief",
            ContextSectionKind::AssignmentPacket => "Current Assignment Packet",
            ContextSectionKind::DependencyResults => "Dependency Result Envelopes",
            ContextSectionKind::ExplicitContext => {
                "Explicit Files, Attachments and Recent Messages"
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextSection {
    pub kind: ContextSectionKind,
    pub source_ids: Vec<String>,
    pub content: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplicitContextFile {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextManifest {
    pub section_kinds: Vec<ContextSectionKind>,
    pub total_chars: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltAssignmentContext {
    pub sections: Vec<ContextSection>,
    pub rendered_prompt: String,
    pub manifest: ContextManifest,
}

#[derive(Debug, Clone)]
pub struct ContextBuildInput {
    pub is_lead: bool,
    pub agent_definition: AgentDefinitionSummary,
    pub capability_packs: Vec<CapabilityPackSummary>,
    pub agent_memory: Vec<String>,
    pub work: WorkSummary,
    pub assignment: AssignmentSummary,
    pub dependency_results: Vec<ResultEnvelope>,
    pub explicit_files: Vec<ExplicitContextFile>,
    pub recent_messages: Vec<String>,
    pub budget_chars: usize,
}

impl Default for ContextBuildInput {
    fn default() -> Self {
        Self {
            is_lead: true,
            agent_definition: AgentDefinitionSummary {
                id: String::new(),
                role_template_id: String::new(),
                role_kind: RoleKind::Lead,
                slug: String::new(),
                name: String::new(),
                description: String::new(),
                instructions: String::new(),
                responsibilities: Vec::new(),
                non_responsibilities: Vec::new(),
                input_contract: serde_json::Value::Null,
                result_contract: serde_json::Value::Null,
                quality_rubric: serde_json::Value::Null,
                default_engine_kind: String::new(),
                default_model_configuration_id: None,
                default_permission_policy: crate::domain::agent::PermissionPolicy::InheritWork,
                default_parallelism: 1,
                memory_policy: crate::domain::agent::MemoryPolicy::ConfirmedOnly,
                capability_packs: Vec::new(),
                builtin: true,
                active: true,
                version: 1,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
            capability_packs: Vec::new(),
            agent_memory: Vec::new(),
            work: WorkSummary {
                id: String::new(),
                title: String::new(),
                goal: String::new(),
                root_path: String::new(),
                permission_mode: crate::domain::work::PermissionMode::Balanced,
                status: crate::domain::work::WorkStatus::Draft,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
            assignment: AssignmentSummary {
                id: String::new(),
                work_id: String::new(),
                parent_assignment_id: None,
                created_by_agent_id: None,
                assigned_agent_id: String::new(),
                capability_pack_id: None,
                kind: crate::domain::assignment::AssignmentKind::Lead,
                side_effect: crate::domain::assignment::AssignmentSideEffect::Unknown,
                title: String::new(),
                instruction: String::new(),
                context_manifest: serde_json::Value::Null,
                expected_result_schema: serde_json::Value::Null,
                acceptance_criteria: serde_json::Value::Null,
                permission_scope: serde_json::Value::Null,
                priority: 0,
                status: crate::domain::assignment::AssignmentStatus::Queued,
                attempt_count: 0,
                max_attempts: 1,
                not_before: None,
                result_summary: None,
                last_error: None,
                next_attempt_at: None,
                recovery_reason: None,
                created_at: chrono::Utc::now(),
                claimed_at: None,
                started_at: None,
                completed_at: None,
                updated_at: chrono::Utc::now(),
            },
            dependency_results: Vec::new(),
            explicit_files: Vec::new(),
            recent_messages: Vec::new(),
            budget_chars: DEFAULT_CONTEXT_BUDGET_CHARS,
        }
    }
}

/// Builds the eight sections in fixed order and renders a bounded prompt. The
/// global budget is applied from the final sections backwards, so the base
/// protocol and agent definition are never silently dropped first.
pub fn build_assignment_context(input: ContextBuildInput) -> BuiltAssignmentContext {
    let mut sections = Vec::with_capacity(8);

    sections.push(ContextSection {
        kind: ContextSectionKind::BaseProtocol,
        source_ids: vec!["piwork-base-protocol".into()],
        content: base_protocol(input.is_lead).to_owned(),
        truncated: false,
    });

    sections.push(ContextSection {
        kind: ContextSectionKind::AgentDefinition,
        source_ids: vec![input.agent_definition.id.clone()],
        content: render_definition(&input.agent_definition),
        truncated: false,
    });

    sections.push(ContextSection {
        kind: ContextSectionKind::CapabilityPack,
        source_ids: input
            .capability_packs
            .iter()
            .map(|pack| pack.id.clone())
            .collect(),
        content: input
            .capability_packs
            .iter()
            .map(render_pack)
            .collect::<Vec<_>>()
            .join("\n\n"),
        truncated: false,
    });

    sections.push(ContextSection {
        kind: ContextSectionKind::AgentMemory,
        source_ids: Vec::new(),
        content: input.agent_memory.join("\n"),
        truncated: false,
    });

    sections.push(ContextSection {
        kind: ContextSectionKind::WorkBrief,
        source_ids: vec![input.work.id.clone()],
        content: format!("Goal: {}", input.work.goal),
        truncated: false,
    });

    sections.push(ContextSection {
        kind: ContextSectionKind::AssignmentPacket,
        source_ids: vec![input.assignment.id.clone()],
        content: render_assignment(&input.assignment),
        truncated: false,
    });

    sections.push(ContextSection {
        kind: ContextSectionKind::DependencyResults,
        source_ids: Vec::new(),
        content: input
            .dependency_results
            .iter()
            .map(render_result)
            .collect::<Vec<_>>()
            .join("\n\n"),
        truncated: false,
    });

    sections.push(ContextSection {
        kind: ContextSectionKind::ExplicitContext,
        source_ids: input
            .explicit_files
            .iter()
            .map(|file| file.path.clone())
            .collect(),
        content: input
            .explicit_files
            .iter()
            .map(|file| format!("## {}\n{}", file.path, file.content))
            .chain(input.recent_messages.iter().cloned())
            .collect::<Vec<_>>()
            .join("\n\n"),
        truncated: false,
    });

    // Apply per-section budget and then the global budget from the tail.
    for section in &mut sections {
        if section.content.chars().count() > DEFAULT_SECTION_BUDGET_CHARS {
            section.content = truncate_chars(&section.content, DEFAULT_SECTION_BUDGET_CHARS);
            section.truncated = true;
        }
    }

    let mut total = 0usize;
    let mut truncated = false;
    for section in sections.iter_mut() {
        let chars = section.content.chars().count();
        let header = section_header(section.kind);
        let section_len = chars + header.chars().count();
        if total + section_len > input.budget_chars {
            let remaining = input
                .budget_chars
                .saturating_sub(total + header.chars().count());
            section.content = truncate_chars(&section.content, remaining);
            section.truncated = true;
            truncated = true;
        }
        total += header.chars().count() + section.content.chars().count();
    }

    let mut rendered = String::new();
    for section in &sections {
        if !rendered.is_empty() {
            rendered.push_str("\n\n");
        }
        rendered.push_str(&section_header(section.kind));
        rendered.push('\n');
        rendered.push_str(&section.content);
    }

    // Final safety net: the rendered prompt must never exceed the budget.
    if rendered.chars().count() > input.budget_chars {
        rendered = truncate_chars(&rendered, input.budget_chars);
        truncated = true;
    }

    let manifest = ContextManifest {
        section_kinds: sections.iter().map(|section| section.kind).collect(),
        total_chars: rendered.chars().count(),
        truncated,
    };

    BuiltAssignmentContext {
        sections,
        rendered_prompt: rendered,
        manifest,
    }
}

fn base_protocol(is_lead: bool) -> &'static str {
    if is_lead {
        "You are the Work lead. Delegate only when there is a clear expertise boundary, an independent evidence need, or an independent review need. One assignment has exactly one responsible member. Synthesize member results into the final delivery; never present a member result as your own conclusion."
    } else {
        "You are a member executing one assignment. You must not delegate. Submit exactly one structured result. Request clarification or suggest delegation only through your result envelope."
    }
}

fn section_header(kind: ContextSectionKind) -> String {
    format!("== {} ==", kind.title())
}

fn render_definition(definition: &AgentDefinitionSummary) -> String {
    let mut parts = vec![
        format!("Name: {}", definition.name),
        format!("Description: {}", definition.description),
        format!("Instructions:\n{}", definition.instructions),
    ];
    if !definition.responsibilities.is_empty() {
        parts.push(format!(
            "Responsibilities:\n- {}",
            definition.responsibilities.join("\n- ")
        ));
    }
    if !definition.non_responsibilities.is_empty() {
        parts.push(format!(
            "Non-responsibilities:\n- {}",
            definition.non_responsibilities.join("\n- ")
        ));
    }
    parts.join("\n")
}

fn render_pack(pack: &CapabilityPackSummary) -> String {
    format!(
        "Pack {}: {}\n{}",
        pack.name, pack.description, pack.instructions
    )
}

fn render_assignment(assignment: &AssignmentSummary) -> String {
    let mut parts = vec![
        format!("Title: {}", assignment.title),
        format!("Instruction:\n{}", assignment.instruction),
    ];
    if assignment.expected_result_schema != serde_json::Value::Null {
        parts.push(format!(
            "Expected result schema:\n{}",
            assignment.expected_result_schema
        ));
    }
    if assignment.acceptance_criteria != serde_json::Value::Null {
        parts.push(format!(
            "Acceptance criteria:\n{}",
            assignment.acceptance_criteria
        ));
    }
    parts.join("\n")
}

fn render_result(result: &ResultEnvelope) -> String {
    format!("Status: {:?}\nSummary: {}", result.status, result.summary)
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}
