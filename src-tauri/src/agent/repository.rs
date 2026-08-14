use std::{collections::HashMap, io};

use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde_json::Value;
use sqlx::{FromRow, SqliteConnection, SqlitePool};

use crate::{
    domain::agent::{
        AgentDefinitionSummary, AgentInstanceSummary, AgentStatus, CapabilityPackStatus,
        CapabilityPackSummary, MemoryPolicy, PermissionPolicy, RoleKind, RoleTemplateSummary,
        WorkAgentStatus, WorkAgentSummary, WorkTeamSummary,
    },
    error::AppError,
};

const BUILTIN_LEAD_INSTANCE_ID: &str = "agent-instance:piwork-lead";

#[derive(Clone)]
pub struct AgentRepository {
    pool: SqlitePool,
}

impl AgentRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_role_templates(&self) -> Result<Vec<RoleTemplateSummary>, AppError> {
        sqlx::query_as::<_, RoleTemplateRow>(
            "SELECT id, slug, role_kind, name, description, base_instructions, \
                    responsibilities_json, non_responsibilities_json, \
                    base_result_contract_json, compatible_capability_kinds_json, \
                    builtin, version, created_at, updated_at \
             FROM role_templates ORDER BY id",
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(RoleTemplateSummary::try_from)
        .collect()
    }

    pub async fn list_agent_instances(&self) -> Result<Vec<AgentInstanceSummary>, AppError> {
        self.load_agent_instances(None).await
    }

    pub async fn get_agent_instance(
        &self,
        id: &str,
    ) -> Result<Option<AgentInstanceSummary>, AppError> {
        Ok(self
            .load_agent_instances(Some(id))
            .await?
            .into_iter()
            .next())
    }

    pub async fn list_capability_packs(&self) -> Result<Vec<CapabilityPackSummary>, AppError> {
        let mut connection = self.pool.acquire().await?;
        load_capability_packs(&mut connection).await
    }

    pub async fn get_work_team(&self, work_id: &str) -> Result<Option<WorkTeamSummary>, AppError> {
        self.load_work_team(work_id, || {}).await
    }

    #[doc(hidden)]
    pub async fn get_work_team_after_work_loaded<F>(
        &self,
        work_id: &str,
        after_work_loaded: F,
    ) -> Result<Option<WorkTeamSummary>, AppError>
    where
        F: FnOnce(),
    {
        self.load_work_team(work_id, after_work_loaded).await
    }

    async fn load_work_team<F>(
        &self,
        work_id: &str,
        after_work_loaded: F,
    ) -> Result<Option<WorkTeamSummary>, AppError>
    where
        F: FnOnce(),
    {
        let mut transaction = self.pool.begin().await?;
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM works WHERE id = ?)")
            .bind(work_id)
            .fetch_one(&mut *transaction)
            .await?;
        if !exists {
            return Ok(None);
        }
        after_work_loaded();

        let member_rows = sqlx::query_as::<_, WorkAgentRow>(
            "SELECT wa.work_id, wa.role_kind AS membership_role_kind, \
                    wa.status AS membership_status, wa.permission_policy, \
                    wa.joined_at, wa.updated_at AS membership_updated_at, \
                    ai.id AS instance_id, ai.display_name, ai.engine_override, \
                    ai.model_configuration_override, ai.permission_policy_override, \
                    ai.parallelism_override, ai.builtin AS instance_builtin, \
                    ai.status AS instance_status, ai.created_at AS instance_created_at, \
                    ai.updated_at AS instance_updated_at, \
                    ad.id AS definition_id, ad.role_template_id, rt.role_kind, ad.slug, \
                    ad.name, ad.description, ad.instructions, ad.responsibilities_json, \
                    ad.non_responsibilities_json, ad.input_contract_json, \
                    ad.result_contract_json, ad.quality_rubric_json, ad.default_engine_kind, \
                    ad.default_model_configuration_id, ad.default_permission_policy, \
                    ad.default_parallelism, ad.memory_policy, ad.builtin AS definition_builtin, \
                    ad.active, ad.version, ad.created_at AS definition_created_at, \
                    ad.updated_at AS definition_updated_at \
             FROM work_agents wa \
             JOIN agent_instances ai ON ai.id = wa.agent_instance_id \
             JOIN agent_definitions ad ON ad.id = ai.definition_id \
             JOIN role_templates rt ON rt.id = ad.role_template_id \
             WHERE wa.work_id = ? ORDER BY ai.id",
        )
        .bind(work_id)
        .fetch_all(&mut *transaction)
        .await?;
        let packs = capability_packs_by_definition(&mut transaction).await?;
        let members = member_rows
            .into_iter()
            .map(|row| row.into_summary(&packs))
            .collect::<Result<Vec<_>, _>>()?;
        let lead_id: Option<String> =
            sqlx::query_scalar("SELECT agent_instance_id FROM work_leads WHERE work_id = ?")
                .bind(work_id)
                .fetch_optional(&mut *transaction)
                .await?;
        let lead_id = lead_id.ok_or_else(|| invariant_error("existing Work has no Lead"))?;
        let lead = members
            .iter()
            .find(|member| member.instance.id == lead_id)
            .cloned()
            .ok_or_else(|| invariant_error("Work Lead is absent from memberships"))?;

        let team = WorkTeamSummary {
            work_id: work_id.to_owned(),
            lead,
            members,
        };
        transaction.commit().await?;
        Ok(Some(team))
    }

    pub async fn add_work_member(
        &self,
        work_id: &str,
        instance_id: &str,
    ) -> Result<WorkTeamSummary, AppError> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        ensure_work_exists(&mut transaction, work_id).await?;
        let instance = membership_source(&mut transaction, instance_id).await?;
        sqlx::query(
            "INSERT INTO work_agents \
             (work_id, agent_instance_id, role_kind, status, permission_policy, joined_at, updated_at) \
             VALUES (?, ?, ?, 'joined', ?, ?, ?) \
             ON CONFLICT(work_id, agent_instance_id) DO UPDATE SET \
                 role_kind = excluded.role_kind, status = 'joined', \
                 permission_policy = excluded.permission_policy, updated_at = excluded.updated_at",
        )
        .bind(work_id)
        .bind(instance_id)
        .bind(instance.role_kind)
        .bind(instance.permission_policy)
        .bind(instance.now)
        .bind(instance.now)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        self.get_work_team(work_id)
            .await?
            .ok_or_else(|| invariant_error("Work disappeared after adding member"))
    }

    pub async fn set_work_lead(
        &self,
        work_id: &str,
        instance_id: &str,
    ) -> Result<WorkTeamSummary, AppError> {
        let mut transaction = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        ensure_work_exists(&mut transaction, work_id).await?;
        let instance = membership_source(&mut transaction, instance_id).await?;
        if instance.role_kind != RoleKind::Lead {
            return Err(AppError::invalid_input(
                "instanceId",
                "Work Lead must use a lead role",
            ));
        }
        sqlx::query(
            "INSERT INTO work_agents \
             (work_id, agent_instance_id, role_kind, status, permission_policy, joined_at, updated_at) \
             VALUES (?, ?, 'lead', 'joined', ?, ?, ?) \
             ON CONFLICT(work_id, agent_instance_id) DO UPDATE SET \
                 role_kind = 'lead', status = 'joined', \
                 permission_policy = excluded.permission_policy, updated_at = excluded.updated_at",
        )
        .bind(work_id)
        .bind(instance_id)
        .bind(instance.permission_policy)
        .bind(instance.now)
        .bind(instance.now)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO work_leads (work_id, agent_instance_id, created_at) VALUES (?, ?, ?) \
             ON CONFLICT(work_id) DO UPDATE SET agent_instance_id = excluded.agent_instance_id, \
                 created_at = excluded.created_at",
        )
        .bind(work_id)
        .bind(instance_id)
        .bind(instance.now)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        self.get_work_team(work_id)
            .await?
            .ok_or_else(|| invariant_error("Work disappeared after replacing Lead"))
    }

    async fn load_agent_instances(
        &self,
        id: Option<&str>,
    ) -> Result<Vec<AgentInstanceSummary>, AppError> {
        let mut connection = self.pool.acquire().await?;
        let rows = if let Some(id) = id {
            sqlx::query_as::<_, AgentInstanceRow>(&format!(
                "{} WHERE ai.id = ? ORDER BY ai.id",
                INSTANCE_SELECT
            ))
            .bind(id)
            .fetch_all(&mut *connection)
            .await?
        } else {
            sqlx::query_as::<_, AgentInstanceRow>(&format!("{} ORDER BY ai.id", INSTANCE_SELECT))
                .fetch_all(&mut *connection)
                .await?
        };
        let packs = capability_packs_by_definition(&mut connection).await?;
        rows.into_iter()
            .map(|row| row.into_summary(&packs))
            .collect()
    }
}

const INSTANCE_SELECT: &str = "SELECT ai.id AS instance_id, ai.display_name, ai.engine_override, \
            ai.model_configuration_override, ai.permission_policy_override, \
            ai.parallelism_override, ai.builtin AS instance_builtin, \
            ai.status AS instance_status, ai.created_at AS instance_created_at, \
            ai.updated_at AS instance_updated_at, \
            ad.id AS definition_id, ad.role_template_id, rt.role_kind, ad.slug, \
            ad.name, ad.description, ad.instructions, ad.responsibilities_json, \
            ad.non_responsibilities_json, ad.input_contract_json, ad.result_contract_json, \
            ad.quality_rubric_json, ad.default_engine_kind, \
            ad.default_model_configuration_id, ad.default_permission_policy, \
            ad.default_parallelism, ad.memory_policy, ad.builtin AS definition_builtin, \
            ad.active, ad.version, ad.created_at AS definition_created_at, \
            ad.updated_at AS definition_updated_at \
     FROM agent_instances ai \
     JOIN agent_definitions ad ON ad.id = ai.definition_id \
     JOIN role_templates rt ON rt.id = ad.role_template_id";

async fn load_capability_packs(
    connection: &mut SqliteConnection,
) -> Result<Vec<CapabilityPackSummary>, AppError> {
    sqlx::query_as::<_, CapabilityPackRow>(
        "SELECT id, catalog_capability_id, name, description, instructions, \
                input_schema_json, output_schema_json, procedure_json, validation_rubric_json, \
                required_tools_json, default_permission_scope, compatible_role_template_ids_json, \
                required_engine_capabilities_json, conflicts_with_capability_pack_ids_json, \
                version, status FROM capability_packs ORDER BY id",
    )
    .fetch_all(&mut *connection)
    .await?
    .into_iter()
    .map(CapabilityPackSummary::try_from)
    .collect()
}

async fn capability_packs_by_definition(
    connection: &mut SqliteConnection,
) -> Result<HashMap<String, Vec<CapabilityPackSummary>>, AppError> {
    let packs = load_capability_packs(connection).await?;
    let packs_by_id = packs
        .into_iter()
        .map(|pack| (pack.id.clone(), pack))
        .collect::<HashMap<_, _>>();
    let bindings: Vec<(String, String)> = sqlx::query_as(
        "SELECT agent_definition_id, capability_pack_id \
         FROM agent_capability_bindings ORDER BY agent_definition_id, capability_pack_id",
    )
    .fetch_all(&mut *connection)
    .await?;
    let mut result: HashMap<String, Vec<CapabilityPackSummary>> = HashMap::new();
    for (definition_id, pack_id) in bindings {
        let pack = packs_by_id
            .get(&pack_id)
            .cloned()
            .ok_or_else(|| invariant_error("capability binding references missing pack"))?;
        result.entry(definition_id).or_default().push(pack);
    }
    Ok(result)
}

struct MembershipSource {
    role_kind: RoleKind,
    permission_policy: PermissionPolicy,
    now: DateTime<Utc>,
}

async fn ensure_work_exists(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    work_id: &str,
) -> Result<(), AppError> {
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM works WHERE id = ?)")
        .bind(work_id)
        .fetch_one(&mut **transaction)
        .await?;
    if exists {
        Ok(())
    } else {
        Err(AppError::work_not_found(work_id))
    }
}

async fn membership_source(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    instance_id: &str,
) -> Result<MembershipSource, AppError> {
    let row: Option<(
        RoleKind,
        AgentStatus,
        Option<PermissionPolicy>,
        PermissionPolicy,
    )> = sqlx::query_as(
        "SELECT rt.role_kind, ai.status, ai.permission_policy_override, \
                    ad.default_permission_policy \
             FROM agent_instances ai \
             JOIN agent_definitions ad ON ad.id = ai.definition_id \
             JOIN role_templates rt ON rt.id = ad.role_template_id \
             WHERE ai.id = ?",
    )
    .bind(instance_id)
    .fetch_optional(&mut **transaction)
    .await?;
    let (role_kind, status, permission_override, default_permission) =
        row.ok_or_else(|| AppError::invalid_input("instanceId", "Agent instance does not exist"))?;
    if status != AgentStatus::Active {
        return Err(AppError::invalid_input(
            "instanceId",
            "Agent instance is not active",
        ));
    }
    Ok(MembershipSource {
        role_kind,
        permission_policy: permission_override.unwrap_or(default_permission),
        now: Utc::now(),
    })
}

fn decode_json<T: DeserializeOwned>(raw: &str) -> Result<T, AppError> {
    serde_json::from_str(raw)
        .map_err(|error| AppError::Database(sqlx::Error::Decode(Box::new(error))))
}

fn checked_u32(value: i64, field: &'static str) -> Result<u32, AppError> {
    u32::try_from(value).map_err(|_| {
        AppError::Database(sqlx::Error::Decode(Box::new(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{field} is outside u32 range"),
        ))))
    })
}

fn invariant_error(message: &'static str) -> AppError {
    AppError::Database(sqlx::Error::Decode(Box::new(io::Error::new(
        io::ErrorKind::InvalidData,
        message,
    ))))
}

#[derive(FromRow)]
struct RoleTemplateRow {
    id: String,
    slug: String,
    role_kind: RoleKind,
    name: String,
    description: String,
    base_instructions: String,
    responsibilities_json: String,
    non_responsibilities_json: String,
    base_result_contract_json: String,
    compatible_capability_kinds_json: String,
    builtin: bool,
    version: i64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl TryFrom<RoleTemplateRow> for RoleTemplateSummary {
    type Error = AppError;

    fn try_from(row: RoleTemplateRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            slug: row.slug,
            role_kind: row.role_kind,
            name: row.name,
            description: row.description,
            base_instructions: row.base_instructions,
            responsibilities: decode_json(&row.responsibilities_json)?,
            non_responsibilities: decode_json(&row.non_responsibilities_json)?,
            base_result_contract: decode_json(&row.base_result_contract_json)?,
            compatible_capability_kinds: decode_json(&row.compatible_capability_kinds_json)?,
            builtin: row.builtin,
            version: checked_u32(row.version, "role template version")?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(FromRow)]
struct CapabilityPackRow {
    id: String,
    catalog_capability_id: Option<String>,
    name: String,
    description: String,
    instructions: String,
    input_schema_json: String,
    output_schema_json: String,
    procedure_json: String,
    validation_rubric_json: String,
    required_tools_json: String,
    default_permission_scope: PermissionPolicy,
    compatible_role_template_ids_json: String,
    required_engine_capabilities_json: String,
    conflicts_with_capability_pack_ids_json: String,
    version: i64,
    status: CapabilityPackStatus,
}

impl TryFrom<CapabilityPackRow> for CapabilityPackSummary {
    type Error = AppError;

    fn try_from(row: CapabilityPackRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            catalog_capability_id: row.catalog_capability_id,
            name: row.name,
            description: row.description,
            instructions: row.instructions,
            input_schema: decode_json(&row.input_schema_json)?,
            output_schema: decode_json(&row.output_schema_json)?,
            procedure: decode_json(&row.procedure_json)?,
            validation_rubric: decode_json(&row.validation_rubric_json)?,
            required_tools: decode_json(&row.required_tools_json)?,
            default_permission_scope: row.default_permission_scope,
            compatible_role_template_ids: decode_json(&row.compatible_role_template_ids_json)?,
            required_engine_capabilities: decode_json(&row.required_engine_capabilities_json)?,
            conflicts_with_capability_pack_ids: decode_json(
                &row.conflicts_with_capability_pack_ids_json,
            )?,
            version: checked_u32(row.version, "capability pack version")?,
            status: row.status,
        })
    }
}

#[derive(FromRow)]
struct AgentInstanceRow {
    instance_id: String,
    display_name: String,
    engine_override: Option<String>,
    model_configuration_override: Option<String>,
    permission_policy_override: Option<PermissionPolicy>,
    parallelism_override: Option<i64>,
    instance_builtin: bool,
    instance_status: AgentStatus,
    instance_created_at: DateTime<Utc>,
    instance_updated_at: DateTime<Utc>,
    definition_id: String,
    role_template_id: String,
    role_kind: RoleKind,
    slug: String,
    name: String,
    description: String,
    instructions: String,
    responsibilities_json: String,
    non_responsibilities_json: String,
    input_contract_json: String,
    result_contract_json: String,
    quality_rubric_json: String,
    default_engine_kind: String,
    default_model_configuration_id: Option<String>,
    default_permission_policy: PermissionPolicy,
    default_parallelism: i64,
    memory_policy: MemoryPolicy,
    definition_builtin: bool,
    active: bool,
    version: i64,
    definition_created_at: DateTime<Utc>,
    definition_updated_at: DateTime<Utc>,
}

impl AgentInstanceRow {
    fn into_summary(
        self,
        packs: &HashMap<String, Vec<CapabilityPackSummary>>,
    ) -> Result<AgentInstanceSummary, AppError> {
        let capability_packs = packs.get(&self.definition_id).cloned().unwrap_or_default();
        Ok(AgentInstanceSummary {
            id: self.instance_id,
            definition: AgentDefinitionSummary {
                id: self.definition_id,
                role_template_id: self.role_template_id,
                role_kind: self.role_kind,
                slug: self.slug,
                name: self.name,
                description: self.description,
                instructions: self.instructions,
                responsibilities: decode_json(&self.responsibilities_json)?,
                non_responsibilities: decode_json(&self.non_responsibilities_json)?,
                input_contract: decode_json::<Value>(&self.input_contract_json)?,
                result_contract: decode_json::<Value>(&self.result_contract_json)?,
                quality_rubric: decode_json::<Value>(&self.quality_rubric_json)?,
                default_engine_kind: self.default_engine_kind,
                default_model_configuration_id: self.default_model_configuration_id,
                default_permission_policy: self.default_permission_policy,
                default_parallelism: checked_u32(
                    self.default_parallelism,
                    "agent definition default parallelism",
                )?,
                memory_policy: self.memory_policy,
                capability_packs,
                builtin: self.definition_builtin,
                active: self.active,
                version: checked_u32(self.version, "agent definition version")?,
                created_at: self.definition_created_at,
                updated_at: self.definition_updated_at,
            },
            display_name: self.display_name,
            engine_override: self.engine_override,
            model_configuration_override: self.model_configuration_override,
            permission_policy_override: self.permission_policy_override,
            parallelism_override: self
                .parallelism_override
                .map(|value| checked_u32(value, "agent instance parallelism override"))
                .transpose()?,
            builtin: self.instance_builtin,
            status: self.instance_status,
            created_at: self.instance_created_at,
            updated_at: self.instance_updated_at,
        })
    }
}

#[derive(FromRow)]
struct WorkAgentRow {
    work_id: String,
    membership_role_kind: RoleKind,
    membership_status: WorkAgentStatus,
    permission_policy: PermissionPolicy,
    joined_at: DateTime<Utc>,
    membership_updated_at: DateTime<Utc>,
    #[sqlx(flatten)]
    instance: AgentInstanceRow,
}

impl WorkAgentRow {
    fn into_summary(
        self,
        packs: &HashMap<String, Vec<CapabilityPackSummary>>,
    ) -> Result<WorkAgentSummary, AppError> {
        Ok(WorkAgentSummary {
            work_id: self.work_id,
            instance: self.instance.into_summary(packs)?,
            role_kind: self.membership_role_kind,
            status: self.membership_status,
            permission_policy: self.permission_policy,
            joined_at: self.joined_at,
            updated_at: self.membership_updated_at,
        })
    }
}

pub(crate) const DEFAULT_LEAD_INSTANCE_ID: &str = BUILTIN_LEAD_INSTANCE_ID;
