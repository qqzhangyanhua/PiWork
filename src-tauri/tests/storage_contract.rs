use piwork_lib::storage::sqlite::Database;
use sqlx::{
    Connection,
    migrate::{Migrate, Migration, MigrationType},
    sqlite::{SqliteConnectOptions, SqliteConnection, SqliteQueryResult},
};
use std::borrow::Cow;

#[test]
fn migration_files_use_stable_lf_line_endings() {
    let migration = include_bytes!("../migrations/0001_foundation.sql");

    assert!(
        !migration.contains(&b'\r'),
        "migration checksums must not vary between Windows and Unix checkouts"
    );
}

#[test]
fn resource_migration_uses_stable_lf_line_endings() {
    let migration = include_bytes!("../migrations/0002_resources.sql");
    assert!(
        !migration.contains(&b'\r'),
        "resource migration checksums must not vary between Windows and Unix checkouts"
    );
}

#[test]
fn document_derivative_migration_uses_stable_lf_line_endings() {
    let migration = include_bytes!("../migrations/0003_document_derivatives.sql");
    assert!(!migration.contains(&b'\r'));
}

#[test]
fn activity_protocol_migration_uses_stable_lf_line_endings() {
    let migration = include_bytes!("../migrations/0004_activity_protocol_v2.sql");
    assert!(!migration.contains(&b'\r'));
}

#[test]
fn agent_domain_migration_uses_stable_lf_line_endings() {
    let migration = include_str!("../migrations/0005_agent_domain.sql");
    assert!(!migration.contains('\r'));
}

#[tokio::test]
async fn agent_domain_migration_seeds_builtin_team_and_capabilities() {
    let database = Database::open_in_memory().await.unwrap();
    let names = database.table_names().await.unwrap();
    for expected in [
        "role_templates",
        "agent_definitions",
        "agent_instances",
        "capability_packs",
        "agent_capability_bindings",
        "work_agents",
        "work_leads",
    ] {
        assert!(names.contains(&expected.to_string()), "missing {expected}");
    }

    for (table, expected) in [
        ("role_templates", 4_i64),
        ("agent_definitions", 4),
        ("agent_instances", 4),
    ] {
        let count: i64 =
            sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE builtin = 1"))
                .fetch_one(database.pool())
                .await
                .unwrap();
        assert_eq!(count, expected, "unexpected builtin count in {table}");
    }

    let role_ids = sqlx::query_scalar::<_, String>("SELECT id FROM role_templates ORDER BY id")
        .fetch_all(database.pool())
        .await
        .unwrap();
    assert_eq!(
        role_ids,
        vec![
            "role-template:engineer:v1",
            "role-template:lead:v1",
            "role-template:researcher:v1",
            "role-template:reviewer:v1",
        ]
    );
    let definition_ids =
        sqlx::query_scalar::<_, String>("SELECT id FROM agent_definitions ORDER BY id")
            .fetch_all(database.pool())
            .await
            .unwrap();
    assert_eq!(
        definition_ids,
        vec![
            "agent-definition:piwork-engineer:v1",
            "agent-definition:piwork-lead:v1",
            "agent-definition:piwork-researcher:v1",
            "agent-definition:piwork-reviewer:v1",
        ]
    );
    let instance_ids =
        sqlx::query_scalar::<_, String>("SELECT id FROM agent_instances ORDER BY id")
            .fetch_all(database.pool())
            .await
            .unwrap();
    assert_eq!(
        instance_ids,
        vec![
            "agent-instance:piwork-engineer",
            "agent-instance:piwork-lead",
            "agent-instance:piwork-researcher",
            "agent-instance:piwork-reviewer",
        ]
    );

    let catalog_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM capability_packs WHERE status = 'catalog_only'")
            .fetch_one(database.pool())
            .await
            .unwrap();
    assert_eq!(catalog_count, 96);
    let system_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM capability_packs WHERE status = 'executable' AND catalog_capability_id IS NULL",
    ).fetch_one(database.pool()).await.unwrap();
    assert_eq!(system_count, 4);
    let catalog_ids = sqlx::query_scalar::<_, String>(
        "SELECT catalog_capability_id FROM capability_packs WHERE status = 'catalog_only' ORDER BY catalog_capability_id",
    ).fetch_all(database.pool()).await.unwrap();
    assert_eq!(
        catalog_ids,
        (1..=96)
            .map(|id| format!("catalog-capability:{id:03}"))
            .collect::<Vec<_>>()
    );
    let mismatched_catalog_ids: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM capability_packs \
         WHERE status = 'catalog_only' AND id <> catalog_capability_id",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(mismatched_catalog_ids, 0);
    let catalog_rows = sqlx::query_as::<_, (String, String)>(
        "SELECT catalog_capability_id, name FROM capability_packs \
         WHERE status = 'catalog_only' ORDER BY catalog_capability_id",
    )
    .fetch_all(database.pool())
    .await
    .unwrap();
    let source = include_str!("../../src/features/agent-center/agentCapabilities.ts");
    let rows_source = source
        .split_once("const ROWS")
        .unwrap()
        .1
        .split_once("\n];")
        .unwrap()
        .0;
    let source_catalog_rows = rows_source
        .lines()
        .filter_map(|line| {
            let row = line.trim().strip_prefix('[')?;
            let (id, remainder) = row.split_once(',')?;
            let name = remainder.trim().strip_prefix('"')?.split_once('"')?.0;
            Some((
                format!("catalog-capability:{:03}", id.trim().parse::<u8>().unwrap()),
                name.to_string(),
            ))
        })
        .collect::<Vec<_>>();
    assert_eq!(catalog_rows, source_catalog_rows);
    let catalog_placeholders: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM capability_packs WHERE status = 'catalog_only' \
         AND description = '' AND instructions = '' \
         AND input_schema_json = '{}' AND output_schema_json = '{}' \
         AND procedure_json = '{}' AND validation_rubric_json = '{}' \
         AND required_tools_json = '[]' AND compatible_role_template_ids_json = '[]' \
         AND required_engine_capabilities_json = '[]' \
         AND conflicts_with_capability_pack_ids_json = '[]' \
         AND default_permission_scope = 'read_only' AND version = 1",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(catalog_placeholders, 96);

    let builtin_contracts = sqlx::query_as::<_, (String, String, String, String, String, i64, String, i64, i64)>(
        "SELECT definitions.id, definitions.name, roles.role_kind, \
         definitions.default_permission_policy, definitions.default_engine_kind, \
         definitions.default_parallelism, definitions.memory_policy, definitions.builtin, definitions.active \
         FROM agent_definitions AS definitions \
         JOIN role_templates AS roles ON roles.id = definitions.role_template_id \
         ORDER BY definitions.id",
    )
    .fetch_all(database.pool())
    .await
    .unwrap();
    assert_eq!(
        builtin_contracts,
        vec![
            (
                "agent-definition:piwork-engineer:v1".into(),
                "工程师".into(),
                "engineer".into(),
                "inherit_work".into(),
                "pi".into(),
                1,
                "confirmed_only".into(),
                1,
                1
            ),
            (
                "agent-definition:piwork-lead:v1".into(),
                "PiWork 主理人".into(),
                "lead".into(),
                "inherit_work".into(),
                "pi".into(),
                1,
                "confirmed_only".into(),
                1,
                1
            ),
            (
                "agent-definition:piwork-researcher:v1".into(),
                "研究员".into(),
                "researcher".into(),
                "read_only".into(),
                "pi".into(),
                1,
                "confirmed_only".into(),
                1,
                1
            ),
            (
                "agent-definition:piwork-reviewer:v1".into(),
                "审阅者".into(),
                "reviewer".into(),
                "read_only".into(),
                "pi".into(),
                1,
                "confirmed_only".into(),
                1,
                1
            ),
        ]
    );

    let bindings = sqlx::query_as::<_, (String, String)>(
        "SELECT agent_definition_id, capability_pack_id FROM agent_capability_bindings ORDER BY agent_definition_id",
    ).fetch_all(database.pool()).await.unwrap();
    assert_eq!(
        bindings,
        vec![
            (
                "agent-definition:piwork-engineer:v1".into(),
                "capability-pack:engineering-execution:v1".into()
            ),
            (
                "agent-definition:piwork-lead:v1".into(),
                "capability-pack:lead-coordination:v1".into()
            ),
            (
                "agent-definition:piwork-researcher:v1".into(),
                "capability-pack:source-research:v1".into()
            ),
            (
                "agent-definition:piwork-reviewer:v1".into(),
                "capability-pack:independent-review:v1".into()
            ),
        ]
    );
}

async fn apply_agent_domain_migration(
    connection: &mut SqliteConnection,
    version: i64,
    description: &'static str,
    sql: &'static str,
) {
    let migration = Migration::new(
        version,
        Cow::Borrowed(description),
        MigrationType::Simple,
        Cow::Borrowed(sql),
        false,
    );
    connection.apply(&migration).await.unwrap();
}

#[tokio::test]
async fn agent_domain_migration_backfills_legacy_work_lead() {
    let mut connection = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(":memory:")
            .foreign_keys(true),
    )
    .await
    .unwrap();
    connection.ensure_migrations_table().await.unwrap();
    for (version, description, sql) in [
        (
            1,
            "foundation",
            include_str!("../migrations/0001_foundation.sql"),
        ),
        (
            2,
            "resources",
            include_str!("../migrations/0002_resources.sql"),
        ),
        (
            3,
            "document derivatives",
            include_str!("../migrations/0003_document_derivatives.sql"),
        ),
        (
            4,
            "activity protocol v2",
            include_str!("../migrations/0004_activity_protocol_v2.sql"),
        ),
    ] {
        apply_agent_domain_migration(&mut connection, version, description, sql).await;
    }
    sqlx::query(
        "INSERT INTO works (id, title, goal, root_path, permission_mode, status, created_at, updated_at) \
         VALUES ('legacy-work', 'Legacy', 'Goal', '/workspace', 'balanced', 'draft', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    ).execute(&mut connection).await.unwrap();

    apply_agent_domain_migration(
        &mut connection,
        5,
        "agent domain",
        include_str!("../migrations/0005_agent_domain.sql"),
    )
    .await;

    let membership = sqlx::query_as::<_, (String, String, String)>(
        "SELECT role_kind, status, permission_policy FROM work_agents WHERE work_id = 'legacy-work' AND agent_instance_id = 'agent-instance:piwork-lead'",
    ).fetch_all(&mut connection).await.unwrap();
    assert_eq!(
        membership,
        vec![("lead".into(), "joined".into(), "inherit_work".into())]
    );
    let leads = sqlx::query_as::<_, (String, String)>(
        "SELECT work_id, agent_instance_id FROM work_leads WHERE work_id = 'legacy-work'",
    )
    .fetch_all(&mut connection)
    .await
    .unwrap();
    assert_eq!(
        leads,
        vec![("legacy-work".into(), "agent-instance:piwork-lead".into())]
    );
}

#[tokio::test]
async fn agent_domain_constraints_reject_duplicate_versions_and_invalid_json() {
    let database = Database::open_in_memory().await.unwrap();
    let duplicate_role = sqlx::query(
        "INSERT INTO role_templates SELECT 'other-role-id', slug, role_kind, name, description, base_instructions, responsibilities_json, non_responsibilities_json, base_result_contract_json, compatible_capability_kinds_json, builtin, version, created_at, updated_at FROM role_templates WHERE id = 'role-template:lead:v1'",
    ).execute(database.pool()).await;
    assert_database_error_contains(duplicate_role, "UNIQUE constraint failed");
    let duplicate_definition = sqlx::query(
        "INSERT INTO agent_definitions SELECT 'other-definition-id', role_template_id, slug, name, description, instructions, responsibilities_json, non_responsibilities_json, input_contract_json, result_contract_json, quality_rubric_json, default_engine_kind, default_model_configuration_id, default_permission_policy, default_parallelism, memory_policy, builtin, active, version, created_at, updated_at FROM agent_definitions WHERE id = 'agent-definition:piwork-lead:v1'",
    ).execute(database.pool()).await;
    assert_database_error_contains(duplicate_definition, "UNIQUE constraint failed");
    for (table, id, columns) in [
        (
            "role_templates",
            "role-template:lead:v1",
            &[
                "responsibilities_json",
                "non_responsibilities_json",
                "base_result_contract_json",
                "compatible_capability_kinds_json",
            ][..],
        ),
        (
            "agent_definitions",
            "agent-definition:piwork-lead:v1",
            &[
                "responsibilities_json",
                "non_responsibilities_json",
                "input_contract_json",
                "result_contract_json",
                "quality_rubric_json",
            ][..],
        ),
        (
            "capability_packs",
            "capability-pack:lead-coordination:v1",
            &[
                "input_schema_json",
                "output_schema_json",
                "procedure_json",
                "validation_rubric_json",
                "required_tools_json",
                "compatible_role_template_ids_json",
                "required_engine_capabilities_json",
                "conflicts_with_capability_pack_ids_json",
            ][..],
        ),
    ] {
        for column in columns {
            let invalid_json = sqlx::query(&format!(
                "UPDATE {table} SET {column} = 'not-json' WHERE id = ?"
            ))
            .bind(id)
            .execute(database.pool())
            .await;
            assert_database_error_contains(invalid_json, "CHECK constraint failed");
        }
    }
}

async fn assert_agent_domain_statement_rejected(
    database: &Database,
    statement: &str,
    expected: &str,
) {
    let result = sqlx::query(statement).execute(database.pool()).await;
    assert_database_error_contains(result, expected);
}

#[tokio::test]
async fn agent_domain_check_constraint_families_are_enforced() {
    let database = Database::open_in_memory().await.unwrap();

    for statement in [
        "UPDATE role_templates SET role_kind = 'operator' WHERE id = 'role-template:lead:v1'",
        "UPDATE role_templates SET builtin = 2 WHERE id = 'role-template:lead:v1'",
        "UPDATE role_templates SET version = 0 WHERE id = 'role-template:lead:v1'",
        "UPDATE agent_definitions SET default_permission_policy = 'admin' WHERE id = 'agent-definition:piwork-lead:v1'",
        "UPDATE agent_definitions SET default_parallelism = 0 WHERE id = 'agent-definition:piwork-lead:v1'",
        "UPDATE agent_definitions SET default_parallelism = 9 WHERE id = 'agent-definition:piwork-lead:v1'",
        "UPDATE agent_definitions SET memory_policy = 'always' WHERE id = 'agent-definition:piwork-lead:v1'",
        "UPDATE agent_definitions SET builtin = -1 WHERE id = 'agent-definition:piwork-lead:v1'",
        "UPDATE agent_definitions SET active = 2 WHERE id = 'agent-definition:piwork-lead:v1'",
        "UPDATE agent_definitions SET version = -1 WHERE id = 'agent-definition:piwork-lead:v1'",
        "UPDATE agent_instances SET permission_policy_override = 'admin' WHERE id = 'agent-instance:piwork-lead'",
        "UPDATE agent_instances SET parallelism_override = 0 WHERE id = 'agent-instance:piwork-lead'",
        "UPDATE agent_instances SET parallelism_override = 9 WHERE id = 'agent-instance:piwork-lead'",
        "UPDATE agent_instances SET builtin = 2 WHERE id = 'agent-instance:piwork-lead'",
        "UPDATE agent_instances SET status = 'paused' WHERE id = 'agent-instance:piwork-lead'",
        "UPDATE capability_packs SET default_permission_scope = 'admin' WHERE id = 'capability-pack:lead-coordination:v1'",
        "UPDATE capability_packs SET version = 0 WHERE id = 'capability-pack:lead-coordination:v1'",
        "UPDATE capability_packs SET status = 'installed' WHERE id = 'capability-pack:lead-coordination:v1'",
    ] {
        assert_agent_domain_statement_rejected(&database, statement, "CHECK constraint failed")
            .await;
    }

    insert_work(&database, "constraint-work").await;
    sqlx::query(
        "INSERT INTO work_agents (work_id, agent_instance_id, role_kind, status, permission_policy, joined_at, updated_at) \
         VALUES ('constraint-work', 'agent-instance:piwork-engineer', 'engineer', 'joined', 'inherit_work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .execute(database.pool())
    .await
    .unwrap();
    for statement in [
        "UPDATE work_agents SET role_kind = 'operator' WHERE work_id = 'constraint-work'",
        "UPDATE work_agents SET status = 'pending' WHERE work_id = 'constraint-work'",
        "UPDATE work_agents SET permission_policy = 'admin' WHERE work_id = 'constraint-work'",
    ] {
        assert_agent_domain_statement_rejected(&database, statement, "CHECK constraint failed")
            .await;
    }
}

#[tokio::test]
async fn agent_domain_foreign_key_owner_policies_are_enforced() {
    let database = Database::open_in_memory().await.unwrap();

    assert_agent_domain_statement_rejected(
        &database,
        "DELETE FROM role_templates WHERE id = 'role-template:lead:v1'",
        "FOREIGN KEY constraint failed",
    )
    .await;
    assert_agent_domain_statement_rejected(
        &database,
        "DELETE FROM capability_packs WHERE id = 'capability-pack:lead-coordination:v1'",
        "FOREIGN KEY constraint failed",
    )
    .await;

    sqlx::query(
        "INSERT INTO role_templates SELECT 'role-template:test:v1', 'test-role', role_kind, name, description, base_instructions, responsibilities_json, non_responsibilities_json, base_result_contract_json, compatible_capability_kinds_json, 0, version, created_at, updated_at FROM role_templates WHERE id = 'role-template:engineer:v1'",
    ).execute(database.pool()).await.unwrap();
    sqlx::query(
        "INSERT INTO agent_definitions SELECT 'agent-definition:test:v1', 'role-template:test:v1', 'test-definition', name, description, instructions, responsibilities_json, non_responsibilities_json, input_contract_json, result_contract_json, quality_rubric_json, default_engine_kind, default_model_configuration_id, default_permission_policy, default_parallelism, memory_policy, 0, active, version, created_at, updated_at FROM agent_definitions WHERE id = 'agent-definition:piwork-engineer:v1'",
    ).execute(database.pool()).await.unwrap();
    sqlx::query(
        "INSERT INTO agent_instances SELECT 'agent-instance:test', 'agent-definition:test:v1', display_name, engine_override, model_configuration_override, permission_policy_override, parallelism_override, 0, status, created_at, updated_at FROM agent_instances WHERE id = 'agent-instance:piwork-engineer'",
    ).execute(database.pool()).await.unwrap();
    sqlx::query(
        "INSERT INTO agent_capability_bindings (agent_definition_id, capability_pack_id, installed_at) VALUES ('agent-definition:test:v1', 'catalog-capability:001', '2026-01-01T00:00:00Z')",
    ).execute(database.pool()).await.unwrap();

    assert_agent_domain_statement_rejected(
        &database,
        "DELETE FROM agent_definitions WHERE id = 'agent-definition:test:v1'",
        "FOREIGN KEY constraint failed",
    )
    .await;
    assert_agent_domain_statement_rejected(
        &database,
        "DELETE FROM capability_packs WHERE id = 'catalog-capability:001'",
        "FOREIGN KEY constraint failed",
    )
    .await;

    insert_work(&database, "owner-work").await;
    sqlx::query(
        "INSERT INTO work_agents (work_id, agent_instance_id, role_kind, status, permission_policy, joined_at, updated_at) \
         VALUES ('owner-work', 'agent-instance:test', 'engineer', 'joined', 'inherit_work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    ).execute(database.pool()).await.unwrap();
    assert_agent_domain_statement_rejected(
        &database,
        "DELETE FROM agent_instances WHERE id = 'agent-instance:test'",
        "FOREIGN KEY constraint failed",
    )
    .await;
    sqlx::query("DELETE FROM works WHERE id = 'owner-work'")
        .execute(database.pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM agent_instances WHERE id = 'agent-instance:test'")
        .execute(database.pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM agent_definitions WHERE id = 'agent-definition:test:v1'")
        .execute(database.pool())
        .await
        .unwrap();
    let binding_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_capability_bindings WHERE agent_definition_id = 'agent-definition:test:v1'",
    ).fetch_one(database.pool()).await.unwrap();
    assert_eq!(
        binding_count, 0,
        "definition deletion must cascade bindings"
    );
}

#[tokio::test]
async fn agent_domain_builtin_seed_contracts_are_exact() {
    let database = Database::open_in_memory().await.unwrap();
    let parse_json = |value: &str| serde_json::from_str::<serde_json::Value>(value).unwrap();

    let roles = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT role_kind, responsibilities_json, non_responsibilities_json, base_result_contract_json \
         FROM role_templates ORDER BY role_kind",
    )
    .fetch_all(database.pool())
    .await
    .unwrap();
    let role_contracts = roles
        .into_iter()
        .map(|(kind, responsibilities, non_responsibilities, result)| {
            (
                kind,
                parse_json(&responsibilities),
                parse_json(&non_responsibilities),
                parse_json(&result),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        role_contracts,
        vec![
            (
                "engineer".into(),
                serde_json::json!(["implement", "debug", "refactor", "test", "artifacts"]),
                serde_json::json!(["不扩大任务范围", "不隐瞒未验证结果"]),
                serde_json::json!({"changes":"array","tests":"array","artifacts":"array","risks":"array"})
            ),
            (
                "lead".into(),
                serde_json::json!([
                    "understand",
                    "decompose",
                    "dispatch",
                    "decide",
                    "synthesize",
                    "deliver"
                ]),
                serde_json::json!(["不伪造成员结论", "不绕过权限边界"]),
                serde_json::json!({"summary":"string","decisions":"array","deliverables":"array","open_risks":"array"})
            ),
            (
                "researcher".into(),
                serde_json::json!([
                    "source_evidence",
                    "fact_check",
                    "comparison",
                    "risk",
                    "confidence"
                ]),
                serde_json::json!(["不修改工作文件", "不把推测表述为事实"]),
                serde_json::json!({"findings":"array","sources":"array","risks":"array","confidence":"string"})
            ),
            (
                "reviewer".into(),
                serde_json::json!([
                    "claims_review",
                    "diff_review",
                    "tests_review",
                    "permission_review",
                    "omission_review"
                ]),
                serde_json::json!(["不替代实现者修改产出", "不在证据不足时宣称通过"]),
                serde_json::json!({"findings":"array","evidence":"array","verdict":"string"})
            ),
        ]
    );

    let definitions = sqlx::query_as::<_, (String, String, i64, String, i64, String, String, String)>(
        "SELECT id, default_permission_policy, default_parallelism, memory_policy, active, responsibilities_json, non_responsibilities_json, result_contract_json \
         FROM agent_definitions ORDER BY id",
    ).fetch_all(database.pool()).await.unwrap();
    let definition_contracts = definitions
        .into_iter()
        .map(
            |(
                id,
                permission,
                parallelism,
                memory,
                active,
                responsibilities,
                non_responsibilities,
                result,
            )| {
                (
                    id,
                    permission,
                    parallelism,
                    memory,
                    active,
                    parse_json(&responsibilities),
                    parse_json(&non_responsibilities),
                    parse_json(&result),
                )
            },
        )
        .collect::<Vec<_>>();
    assert_eq!(
        definition_contracts,
        vec![
            (
                "agent-definition:piwork-engineer:v1".into(),
                "inherit_work".into(),
                1,
                "confirmed_only".into(),
                1,
                serde_json::json!(["implement", "debug", "refactor", "test", "artifacts"]),
                serde_json::json!(["不扩大任务范围", "不隐瞒未验证结果"]),
                serde_json::json!({"changes":"array","tests":"array","artifacts":"array","risks":"array"})
            ),
            (
                "agent-definition:piwork-lead:v1".into(),
                "inherit_work".into(),
                1,
                "confirmed_only".into(),
                1,
                serde_json::json!([
                    "understand",
                    "decompose",
                    "dispatch",
                    "decide",
                    "synthesize",
                    "deliver"
                ]),
                serde_json::json!(["不伪造成员结论", "不绕过权限边界"]),
                serde_json::json!({"summary":"string","decisions":"array","deliverables":"array","open_risks":"array"})
            ),
            (
                "agent-definition:piwork-researcher:v1".into(),
                "read_only".into(),
                1,
                "confirmed_only".into(),
                1,
                serde_json::json!([
                    "source_evidence",
                    "fact_check",
                    "comparison",
                    "risk",
                    "confidence"
                ]),
                serde_json::json!(["不修改工作文件", "不把推测表述为事实"]),
                serde_json::json!({"findings":"array","sources":"array","risks":"array","confidence":"string"})
            ),
            (
                "agent-definition:piwork-reviewer:v1".into(),
                "read_only".into(),
                1,
                "confirmed_only".into(),
                1,
                serde_json::json!([
                    "claims_review",
                    "diff_review",
                    "tests_review",
                    "permission_review",
                    "omission_review"
                ]),
                serde_json::json!(["不替代实现者修改产出", "不在证据不足时宣称通过"]),
                serde_json::json!({"findings":"array","evidence":"array","verdict":"string"})
            ),
        ]
    );

    let instances = sqlx::query_as::<_, (String, String)>(
        "SELECT id, status FROM agent_instances WHERE builtin = 1 ORDER BY id",
    )
    .fetch_all(database.pool())
    .await
    .unwrap();
    assert_eq!(
        instances,
        vec![
            ("agent-instance:piwork-engineer".into(), "active".into()),
            ("agent-instance:piwork-lead".into(), "active".into()),
            ("agent-instance:piwork-researcher".into(), "active".into()),
            ("agent-instance:piwork-reviewer".into(), "active".into()),
        ]
    );
}

#[tokio::test]
async fn agent_domain_work_lead_membership_constraints_and_cascades_hold() {
    let database = Database::open_in_memory().await.unwrap();
    insert_work(&database, "agent-work").await;
    let missing_membership = sqlx::query(
        "INSERT INTO work_leads (work_id, agent_instance_id, created_at) VALUES ('agent-work', 'agent-instance:piwork-lead', '2026-01-01T00:00:00Z')",
    ).execute(database.pool()).await;
    assert_database_error_contains(missing_membership, "FOREIGN KEY constraint failed");

    sqlx::query(
        "INSERT INTO work_agents (work_id, agent_instance_id, role_kind, status, permission_policy, joined_at, updated_at) VALUES ('agent-work', 'agent-instance:piwork-lead', 'lead', 'joined', 'inherit_work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    ).execute(database.pool()).await.unwrap();
    sqlx::query(
        "INSERT INTO work_leads (work_id, agent_instance_id, created_at) VALUES ('agent-work', 'agent-instance:piwork-lead', '2026-01-01T00:00:00Z')",
    ).execute(database.pool()).await.unwrap();
    sqlx::query(
        "INSERT INTO work_agents (work_id, agent_instance_id, role_kind, status, permission_policy, joined_at, updated_at) VALUES ('agent-work', 'agent-instance:piwork-engineer', 'engineer', 'joined', 'inherit_work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    ).execute(database.pool()).await.unwrap();
    let second_lead = sqlx::query(
        "INSERT INTO work_leads (work_id, agent_instance_id, created_at) VALUES ('agent-work', 'agent-instance:piwork-engineer', '2026-01-01T00:00:00Z')",
    ).execute(database.pool()).await;
    assert_database_error_contains(second_lead, "UNIQUE constraint failed");
    let delete_membership = sqlx::query(
        "DELETE FROM work_agents WHERE work_id = 'agent-work' AND agent_instance_id = 'agent-instance:piwork-lead'",
    ).execute(database.pool()).await;
    assert_database_error_contains(delete_membership, "FOREIGN KEY constraint failed");

    sqlx::query("DELETE FROM works WHERE id = 'agent-work'")
        .execute(database.pool())
        .await
        .unwrap();
    for table in ["work_agents", "work_leads"] {
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM {table} WHERE work_id = 'agent-work'"
        ))
        .fetch_one(database.pool())
        .await
        .unwrap();
        assert_eq!(count, 0, "rows remain in {table}");
    }
}

async fn insert_work(database: &Database, id: &str) {
    insert_work_with_permission_mode(database, id, "balanced")
        .await
        .unwrap();
}

async fn insert_work_with_permission_mode(
    database: &Database,
    id: &str,
    permission_mode: &str,
) -> Result<SqliteQueryResult, sqlx::Error> {
    sqlx::query(
        "INSERT INTO works \
         (id, title, goal, root_path, permission_mode, status, created_at, updated_at) \
         VALUES (?, 'Work', 'Goal', '/workspace', ?, 'draft', ?, ?)",
    )
    .bind(id)
    .bind(permission_mode)
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await
}

async fn insert_run(database: &Database, id: &str, work_id: &str) {
    sqlx::query(
        "INSERT INTO runs \
         (id, work_id, engine_kind, model_label, status, created_at, updated_at) \
         VALUES (?, ?, 'test-engine', 'test-model', 'queued', ?, ?)",
    )
    .bind(id)
    .bind(work_id)
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await
    .unwrap();
}

async fn insert_event(database: &Database, id: &str, work_id: &str, run_id: &str, sequence: i64) {
    try_insert_event(database, id, work_id, run_id, sequence)
        .await
        .unwrap();
}

async fn try_insert_event(
    database: &Database,
    id: &str,
    work_id: &str,
    run_id: &str,
    sequence: i64,
) -> Result<SqliteQueryResult, sqlx::Error> {
    sqlx::query(
        "INSERT INTO events \
         (id, work_id, run_id, sequence, version, occurred_at, payload) \
         VALUES (?, ?, ?, ?, 1, ?, '{}')",
    )
    .bind(id)
    .bind(work_id)
    .bind(run_id)
    .bind(sequence)
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await
}

fn assert_database_error_contains(result: Result<SqliteQueryResult, sqlx::Error>, expected: &str) {
    match result {
        Err(sqlx::Error::Database(error)) => assert!(
            error.message().contains(expected),
            "expected database error containing {expected:?}, got {:?}",
            error.message()
        ),
        Err(error) => panic!("expected database error containing {expected:?}, got {error:?}"),
        Ok(_) => panic!("expected database error containing {expected:?}"),
    }
}

#[tokio::test]
async fn migration_creates_foundation_tables() {
    let database = Database::open_in_memory().await.unwrap();
    let names = database.table_names().await.unwrap();

    for expected in ["works", "runs", "messages", "events", "settings"] {
        assert!(names.contains(&expected.to_string()), "missing {expected}");
    }
}

#[tokio::test]
async fn activity_protocol_migration_adds_nullable_event_context_and_indexes() {
    let database = Database::open_in_memory().await.unwrap();
    let columns = sqlx::query_as::<_, (String, String, i64, Option<String>)>(
        "SELECT name, type, \"notnull\", dflt_value \
         FROM pragma_table_info('events') \
         WHERE name IN ( \
             'turn_id', 'session_id', 'agent_id', 'assignment_id', \
             'causation_id', 'correlation_id' \
         ) \
         ORDER BY cid",
    )
    .fetch_all(database.pool())
    .await
    .unwrap();
    let column_contract = columns
        .iter()
        .map(|(name, column_type, not_null, default)| {
            (
                name.as_str(),
                column_type.as_str(),
                *not_null,
                default.as_deref(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        column_contract,
        vec![
            ("turn_id", "TEXT", 0, None),
            ("session_id", "TEXT", 0, None),
            ("agent_id", "TEXT", 0, None),
            ("assignment_id", "TEXT", 0, None),
            ("causation_id", "TEXT", 0, None),
            ("correlation_id", "TEXT", 0, None),
        ]
    );

    for (index_name, expected_columns) in [
        (
            "idx_events_work_turn_sequence",
            vec!["work_id", "turn_id", "sequence"],
        ),
        (
            "idx_events_assignment_sequence",
            vec!["assignment_id", "sequence"],
        ),
    ] {
        let index_columns =
            sqlx::query_scalar::<_, String>("SELECT name FROM pragma_index_info(?) ORDER BY seqno")
                .bind(index_name)
                .fetch_all(database.pool())
                .await
                .unwrap();
        assert_eq!(index_columns, expected_columns);
    }

    let partial_indexes = sqlx::query_as::<_, (String, i64)>(
        "SELECT name, partial FROM pragma_index_list('events') \
         WHERE name IN ('idx_events_work_turn_sequence', 'idx_events_assignment_sequence') \
         ORDER BY name",
    )
    .fetch_all(database.pool())
    .await
    .unwrap();
    assert_eq!(
        partial_indexes,
        vec![
            ("idx_events_assignment_sequence".into(), 1),
            ("idx_events_work_turn_sequence".into(), 0),
        ]
    );

    let assignment_index_sql: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master \
         WHERE type = 'index' AND name = 'idx_events_assignment_sequence'",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(
        assignment_index_sql
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        "CREATE INDEX idx_events_assignment_sequence ON events(assignment_id, sequence) \
         WHERE assignment_id IS NOT NULL"
    );
}

#[tokio::test]
async fn migration_creates_resource_tables_and_local_personal_space() {
    let database = Database::open_in_memory().await.unwrap();
    let names = database.table_names().await.unwrap();

    for expected in [
        "spaces",
        "resource_blobs",
        "blob_replicas",
        "managed_resources",
        "resource_links",
    ] {
        assert!(names.contains(&expected.to_string()), "missing {expected}");
    }

    let kind: String = sqlx::query_scalar("SELECT kind FROM spaces WHERE id = 'local-personal'")
        .fetch_one(database.pool())
        .await
        .unwrap();
    assert_eq!(kind, "personal");

    let work_space_column: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('works') WHERE name = 'space_id'",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(work_space_column, 1);
}

#[tokio::test]
async fn migration_creates_constrained_document_derivatives() {
    let database = Database::open_in_memory().await.unwrap();
    let names = database.table_names().await.unwrap();
    assert!(names.contains(&"resource_derivatives".to_string()));

    let sql: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'resource_derivatives'",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert!(sql.contains("canonical_markdown"));
    assert!(sql.contains("processing"));
    assert!(sql.contains("ready"));
    assert!(sql.contains("failed"));
}

#[tokio::test]
async fn resource_links_require_exactly_one_work_or_draft_owner() {
    let database = Database::open_in_memory().await.unwrap();
    let result = sqlx::query(
        "INSERT INTO resource_links \
         (id, resource_id, work_id, draft_id, role, created_at) \
         VALUES ('link-1', 'missing', NULL, NULL, 'attached', ?)",
    )
    .bind("2026-07-31T00:00:00Z")
    .execute(database.pool())
    .await;

    assert_database_error_contains(result, "CHECK constraint failed");
}

#[tokio::test]
async fn runs_persist_engine_execution_identity() {
    let database = Database::open_in_memory().await.unwrap();

    let columns = sqlx::query_scalar::<_, String>("SELECT name FROM pragma_table_info('runs')")
        .fetch_all(database.pool())
        .await
        .unwrap();

    assert!(columns.contains(&"engine_kind".to_string()));
    assert!(columns.contains(&"engine_session_id".to_string()));
}

#[tokio::test]
async fn file_database_creates_parent_directories_and_reopens() {
    let temporary_directory = tempfile::tempdir().unwrap();
    let database_path = temporary_directory
        .path()
        .join("nested")
        .join("storage")
        .join("piwork.sqlite3");

    assert!(!database_path.parent().unwrap().exists());

    let database = Database::open(&database_path).await.unwrap();
    assert!(database_path.is_file());
    drop(database);

    let reopened = Database::open(&database_path).await.unwrap();
    assert!(
        reopened
            .table_names()
            .await
            .unwrap()
            .contains(&"works".to_string())
    );
}

#[tokio::test]
async fn cross_work_message_association_is_rejected() {
    let database = Database::open_in_memory().await.unwrap();
    insert_work(&database, "work-1").await;
    insert_work(&database, "work-2").await;
    insert_run(&database, "run-2", "work-2").await;

    let result = sqlx::query(
        "INSERT INTO messages (id, work_id, run_id, role, content, created_at) \
         VALUES ('message-1', 'work-1', 'run-2', 'user', 'hello', ?)",
    )
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await;
    assert_database_error_contains(result, "FOREIGN KEY constraint failed");

    sqlx::query(
        "INSERT INTO messages (id, work_id, run_id, role, content, created_at) \
         VALUES ('work-message', 'work-1', NULL, 'user', 'hello', ?)",
    )
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await
    .unwrap();
}

#[tokio::test]
async fn cross_work_event_association_is_rejected() {
    let database = Database::open_in_memory().await.unwrap();
    insert_work(&database, "work-1").await;
    insert_work(&database, "work-2").await;
    insert_run(&database, "run-2", "work-2").await;

    let result = sqlx::query(
        "INSERT INTO events \
         (id, work_id, run_id, sequence, version, occurred_at, payload) \
         VALUES ('event-1', 'work-1', 'run-2', 1, 1, ?, '{}')",
    )
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await;
    assert_database_error_contains(result, "FOREIGN KEY constraint failed");
}

#[tokio::test]
async fn file_connections_apply_sqlite_safety_pragmas() {
    let temporary_directory = tempfile::tempdir().unwrap();
    let database_path = temporary_directory.path().join("pragmas.sqlite3");
    let database = Database::open(database_path).await.unwrap();

    let mut first = database.pool().acquire().await.unwrap();
    let mut second = database.pool().acquire().await.unwrap();

    for connection in [&mut first, &mut second] {
        let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&mut **connection)
            .await
            .unwrap();
        let busy_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
            .fetch_one(&mut **connection)
            .await
            .unwrap();

        assert_eq!(foreign_keys, 1);
        assert_eq!(busy_timeout, 5_000);
    }
}

#[tokio::test]
async fn in_memory_database_uses_one_connection() {
    let database = Database::open_in_memory().await.unwrap();

    assert_eq!(database.pool().options().get_max_connections(), 1);
}

#[tokio::test]
async fn invalid_work_and_run_statuses_are_rejected() {
    let database = Database::open_in_memory().await.unwrap();

    let invalid_work = sqlx::query(
        "INSERT INTO works \
         (id, title, goal, root_path, permission_mode, status, created_at, updated_at) \
         VALUES ('invalid-work', 'Work', 'Goal', '/workspace', 'balanced', \
         'invalid', ?, ?)",
    )
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await;
    assert_database_error_contains(invalid_work, "CHECK constraint failed");

    insert_work(&database, "work-1").await;
    let invalid_run = sqlx::query(
        "INSERT INTO runs \
         (id, work_id, engine_kind, model_label, status, created_at, updated_at) \
         VALUES ('invalid-run', 'work-1', 'test-engine', 'test-model', 'invalid', ?, ?)",
    )
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await;
    assert_database_error_contains(invalid_run, "CHECK constraint failed");
}

#[tokio::test]
async fn permission_modes_are_constrained_to_approved_values() {
    let database = Database::open_in_memory().await.unwrap();

    for (index, permission_mode) in ["ask_every_step", "balanced", "auto_execute"]
        .into_iter()
        .enumerate()
    {
        insert_work_with_permission_mode(
            &database,
            &format!("valid-permission-{index}"),
            permission_mode,
        )
        .await
        .unwrap();

        let stored: String = sqlx::query_scalar("SELECT permission_mode FROM works WHERE id = ?")
            .bind(format!("valid-permission-{index}"))
            .fetch_one(database.pool())
            .await
            .unwrap();
        assert_eq!(stored, permission_mode);
    }

    let invalid =
        insert_work_with_permission_mode(&database, "invalid-permission", "unrestricted").await;
    assert_database_error_contains(invalid, "CHECK constraint failed");
}

#[tokio::test]
async fn json_columns_reject_invalid_json() {
    let database = Database::open_in_memory().await.unwrap();
    insert_work(&database, "work-1").await;
    insert_run(&database, "run-1", "work-1").await;

    let invalid_event = sqlx::query(
        "INSERT INTO events \
         (id, work_id, run_id, sequence, version, occurred_at, payload) \
         VALUES ('event-1', 'work-1', 'run-1', 1, 1, ?, 'not-json')",
    )
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await;
    assert_database_error_contains(invalid_event, "CHECK constraint failed");

    let invalid_setting = sqlx::query(
        "INSERT INTO settings (key, value, updated_at) VALUES ('theme', 'not-json', ?)",
    )
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await;
    assert_database_error_contains(invalid_setting, "CHECK constraint failed");
}

#[tokio::test]
async fn event_sequences_are_unique_within_a_run() {
    let database = Database::open_in_memory().await.unwrap();
    insert_work(&database, "work-1").await;
    insert_run(&database, "run-1", "work-1").await;
    insert_event(&database, "event-1", "work-1", "run-1", 1).await;

    let duplicate = sqlx::query(
        "INSERT INTO events \
         (id, work_id, run_id, sequence, version, occurred_at, payload) \
         VALUES ('event-2', 'work-1', 'run-1', 1, 1, ?, '{}')",
    )
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await;
    assert_database_error_contains(duplicate, "UNIQUE constraint failed");
}

#[tokio::test]
async fn event_sequence_zero_is_rejected() {
    let database = Database::open_in_memory().await.unwrap();
    insert_work(&database, "work-1").await;
    insert_run(&database, "run-1", "work-1").await;

    let result = try_insert_event(&database, "event-zero", "work-1", "run-1", 0).await;
    assert_database_error_contains(result, "CHECK constraint failed");
}

#[tokio::test]
async fn event_sequence_above_u32_is_rejected() {
    let database = Database::open_in_memory().await.unwrap();
    insert_work(&database, "work-1").await;
    insert_run(&database, "run-1", "work-1").await;

    let result = try_insert_event(
        &database,
        "event-too-large",
        "work-1",
        "run-1",
        4_294_967_296,
    )
    .await;
    assert_database_error_contains(result, "CHECK constraint failed");
}

#[tokio::test]
async fn event_sequence_u32_boundaries_are_accepted() {
    let database = Database::open_in_memory().await.unwrap();
    insert_work(&database, "work-1").await;
    insert_run(&database, "run-1", "work-1").await;

    insert_event(&database, "event-first", "work-1", "run-1", 1).await;
    insert_event(&database, "event-last", "work-1", "run-1", 4_294_967_295).await;
}

#[tokio::test]
async fn deleting_a_work_cascades_to_its_run_messages_and_events() {
    let database = Database::open_in_memory().await.unwrap();
    insert_work(&database, "work-1").await;
    insert_run(&database, "run-1", "work-1").await;
    insert_event(&database, "event-1", "work-1", "run-1", 1).await;

    for (id, run_id) in [("work-message", None), ("run-message", Some("run-1"))] {
        sqlx::query(
            "INSERT INTO messages (id, work_id, run_id, role, content, created_at) \
             VALUES (?, 'work-1', ?, 'user', 'hello', ?)",
        )
        .bind(id)
        .bind(run_id)
        .bind("2026-01-01T00:00:00Z")
        .execute(database.pool())
        .await
        .unwrap();
    }

    sqlx::query("DELETE FROM works WHERE id = 'work-1'")
        .execute(database.pool())
        .await
        .unwrap();

    for table in ["runs", "messages", "events"] {
        let statement = format!("SELECT COUNT(*) FROM {table} WHERE work_id = 'work-1'");
        let count: i64 = sqlx::query_scalar(&statement)
            .fetch_one(database.pool())
            .await
            .unwrap();
        assert_eq!(count, 0, "rows remain in {table}");
    }
}
