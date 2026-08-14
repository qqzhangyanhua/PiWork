use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

macro_rules! binding_path {
    () => {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings/")
    };
}

macro_rules! wire_enum {
    ($name:ident { $($variant:ident),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, TS)]
        #[serde(rename_all = "snake_case")]
        #[sqlx(type_name = "TEXT", rename_all = "snake_case")]
        #[ts(rename_all = "snake_case", export_to = binding_path!())]
        pub enum $name {
            $($variant),+
        }
    };
}

wire_enum!(RoleKind {
    Lead,
    Researcher,
    Engineer,
    Reviewer,
});
wire_enum!(AgentStatus { Active, Inactive });
wire_enum!(CapabilityPackStatus {
    CatalogOnly,
    Executable,
    Deprecated,
});
wire_enum!(WorkAgentStatus { Joined, Inactive });
wire_enum!(PermissionPolicy {
    InheritWork,
    ReadOnly,
    WorkWrite,
});
wire_enum!(MemoryPolicy { ConfirmedOnly });
wire_enum!(AssemblyDiagnosticCode {
    NotExecutable,
    IncompatibleRole,
    MissingTool,
    MissingEngineCapability,
    PermissionEscalation,
    CapabilityConflict,
    ContextBudgetExceeded,
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct RoleTemplateSummary {
    pub id: String,
    pub slug: String,
    pub role_kind: RoleKind,
    pub name: String,
    pub description: String,
    pub base_instructions: String,
    pub responsibilities: Vec<String>,
    pub non_responsibilities: Vec<String>,
    #[ts(type = "unknown")]
    pub base_result_contract: Value,
    pub compatible_capability_kinds: Vec<String>,
    pub builtin: bool,
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct AgentDefinitionSummary {
    pub id: String,
    pub role_template_id: String,
    pub role_kind: RoleKind,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub responsibilities: Vec<String>,
    pub non_responsibilities: Vec<String>,
    #[ts(type = "unknown")]
    pub input_contract: Value,
    #[ts(type = "unknown")]
    pub result_contract: Value,
    #[ts(type = "unknown")]
    pub quality_rubric: Value,
    pub default_engine_kind: String,
    pub default_model_configuration_id: Option<String>,
    pub default_permission_policy: PermissionPolicy,
    pub default_parallelism: u32,
    pub memory_policy: MemoryPolicy,
    pub capability_packs: Vec<CapabilityPackSummary>,
    pub builtin: bool,
    pub active: bool,
    pub version: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct AgentInstanceSummary {
    pub id: String,
    pub definition: AgentDefinitionSummary,
    pub display_name: String,
    pub engine_override: Option<String>,
    pub model_configuration_override: Option<String>,
    pub permission_policy_override: Option<PermissionPolicy>,
    pub parallelism_override: Option<u32>,
    pub builtin: bool,
    pub status: AgentStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct CapabilityPackSummary {
    pub id: String,
    pub catalog_capability_id: Option<String>,
    pub name: String,
    pub description: String,
    pub instructions: String,
    #[ts(type = "unknown")]
    pub input_schema: Value,
    #[ts(type = "unknown")]
    pub output_schema: Value,
    #[ts(type = "unknown")]
    pub procedure: Value,
    #[ts(type = "unknown")]
    pub validation_rubric: Value,
    pub required_tools: Vec<String>,
    pub default_permission_scope: PermissionPolicy,
    pub compatible_role_template_ids: Vec<String>,
    pub required_engine_capabilities: Vec<String>,
    pub conflicts_with_capability_pack_ids: Vec<String>,
    pub version: u32,
    pub status: CapabilityPackStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct WorkAgentSummary {
    pub work_id: String,
    pub instance: AgentInstanceSummary,
    pub role_kind: RoleKind,
    pub status: WorkAgentStatus,
    pub permission_policy: PermissionPolicy,
    pub joined_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct WorkTeamSummary {
    pub work_id: String,
    pub lead: WorkAgentSummary,
    pub members: Vec<WorkAgentSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct AssemblyDiagnostic {
    pub code: AssemblyDiagnosticCode,
    pub capability_pack_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct SaveAgentAssemblyInput {
    pub source_instance_id: String,
    pub display_name: String,
    pub capability_pack_ids: Vec<String>,
    pub engine_override: Option<String>,
    pub model_configuration_override: Option<String>,
    pub permission_policy_override: Option<PermissionPolicy>,
    pub parallelism_override: Option<u32>,
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    use super::{
        AgentDefinitionSummary, AgentInstanceSummary, AgentStatus, AssemblyDiagnosticCode,
        CapabilityPackStatus, MemoryPolicy, PermissionPolicy, RoleKind, WorkAgentStatus,
        WorkAgentSummary, WorkTeamSummary,
    };

    #[test]
    fn enums_use_stable_snake_case_wire_values() {
        macro_rules! assert_wire_values {
            ($enum_type:ty, { $($variant:path => $wire_value:literal),+ $(,)? }) => {
                $(
                    assert_eq!(serde_json::to_value($variant).unwrap(), json!($wire_value));
                    assert_eq!(
                        serde_json::from_value::<$enum_type>(json!($wire_value)).unwrap(),
                        $variant
                    );
                )+
            };
        }

        assert_wire_values!(RoleKind, {
            RoleKind::Lead => "lead",
            RoleKind::Researcher => "researcher",
            RoleKind::Engineer => "engineer",
            RoleKind::Reviewer => "reviewer",
        });
        assert_wire_values!(AgentStatus, {
            AgentStatus::Active => "active",
            AgentStatus::Inactive => "inactive",
        });
        assert_wire_values!(CapabilityPackStatus, {
            CapabilityPackStatus::CatalogOnly => "catalog_only",
            CapabilityPackStatus::Executable => "executable",
            CapabilityPackStatus::Deprecated => "deprecated",
        });
        assert_wire_values!(WorkAgentStatus, {
            WorkAgentStatus::Joined => "joined",
            WorkAgentStatus::Inactive => "inactive",
        });
        assert_wire_values!(PermissionPolicy, {
            PermissionPolicy::InheritWork => "inherit_work",
            PermissionPolicy::ReadOnly => "read_only",
            PermissionPolicy::WorkWrite => "work_write",
        });
        assert_wire_values!(MemoryPolicy, {
            MemoryPolicy::ConfirmedOnly => "confirmed_only",
        });
        assert_wire_values!(AssemblyDiagnosticCode, {
            AssemblyDiagnosticCode::NotExecutable => "not_executable",
            AssemblyDiagnosticCode::IncompatibleRole => "incompatible_role",
            AssemblyDiagnosticCode::MissingTool => "missing_tool",
            AssemblyDiagnosticCode::MissingEngineCapability => "missing_engine_capability",
            AssemblyDiagnosticCode::PermissionEscalation => "permission_escalation",
            AssemblyDiagnosticCode::CapabilityConflict => "capability_conflict",
            AssemblyDiagnosticCode::ContextBudgetExceeded => "context_budget_exceeded",
        });
    }

    #[test]
    fn work_team_summary_uses_camel_case_and_preserves_nested_role() {
        let timestamp = Utc.with_ymd_and_hms(2026, 8, 14, 8, 0, 0).unwrap();
        let definition = AgentDefinitionSummary {
            id: "definition-1".into(),
            role_template_id: "role-template-1".into(),
            role_kind: RoleKind::Lead,
            slug: "lead".into(),
            name: "Lead".into(),
            description: "Coordinates the work".into(),
            instructions: "Lead the team".into(),
            responsibilities: vec!["coordinate".into()],
            non_responsibilities: vec!["implement everything".into()],
            input_contract: json!({"type": "object"}),
            result_contract: json!({"type": "object"}),
            quality_rubric: json!({"required": true}),
            default_engine_kind: "codex".into(),
            default_model_configuration_id: None,
            default_permission_policy: PermissionPolicy::InheritWork,
            default_parallelism: 1,
            memory_policy: MemoryPolicy::ConfirmedOnly,
            capability_packs: vec![],
            builtin: true,
            active: true,
            version: 1,
            created_at: timestamp,
            updated_at: timestamp,
        };
        let instance = AgentInstanceSummary {
            id: "instance-1".into(),
            definition,
            display_name: "Primary lead".into(),
            engine_override: None,
            model_configuration_override: None,
            permission_policy_override: None,
            parallelism_override: None,
            builtin: false,
            status: AgentStatus::Active,
            created_at: timestamp,
            updated_at: timestamp,
        };
        let lead = WorkAgentSummary {
            work_id: "work-1".into(),
            instance,
            role_kind: RoleKind::Lead,
            status: WorkAgentStatus::Joined,
            permission_policy: PermissionPolicy::ReadOnly,
            joined_at: timestamp,
            updated_at: timestamp,
        };
        let member = lead.clone();
        let value = serde_json::to_value(WorkTeamSummary {
            work_id: "work-1".into(),
            lead,
            members: vec![member],
        })
        .unwrap();

        assert_eq!(value["workId"], "work-1");
        assert_eq!(value["lead"]["instance"]["definition"]["roleKind"], "lead");
        assert_eq!(value["lead"]["instance"]["displayName"], "Primary lead");
        assert_eq!(value["lead"]["permissionPolicy"], "read_only");
        assert_eq!(value["lead"]["joinedAt"], "2026-08-14T08:00:00Z");
        assert_eq!(value["members"].as_array().unwrap().len(), 1);
        assert!(value.get("work_id").is_none());
        assert!(value["lead"].get("permission_policy").is_none());
    }
}
