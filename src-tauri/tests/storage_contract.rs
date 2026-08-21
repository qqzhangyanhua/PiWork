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

#[test]
fn assignment_session_migration_uses_stable_lf_line_endings() {
    let migration = include_str!("../migrations/0006_assignment_sessions.sql");
    assert!(!migration.contains('\r'));
}

#[test]
fn assignment_outbox_migration_uses_stable_lf_line_endings() {
    let migration = include_str!("../migrations/0007_assignment_event_outbox.sql");
    assert!(!migration.contains('\r'));
}

#[test]
fn work_memory_migration_uses_stable_lf_line_endings() {
    let migration = include_str!("../migrations/0008_work_memory_and_results.sql");
    assert!(!migration.contains('\r'));
}

#[test]
fn memory_candidate_assignment_migration_uses_stable_lf_line_endings() {
    let migration = include_str!("../migrations/0009_memory_candidate_assignment.sql");
    assert!(!migration.contains('\r'));
}

#[test]
fn extensions_and_connectors_migration_uses_stable_lf_line_endings() {
    let migration = include_str!("../migrations/0010_extensions_and_connectors.sql");
    assert!(!migration.contains('\r'));
}

#[tokio::test]
async fn assignment_outbox_has_explicit_global_ordinal_and_delivery_metadata() {
    let database = Database::open_in_memory().await.unwrap();
    let table_sql: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = 'assignment_event_outbox'",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert!(table_sql.contains("ordinal INTEGER PRIMARY KEY AUTOINCREMENT"));
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT name FROM pragma_table_info('assignment_event_outbox') ORDER BY cid"
        )
        .fetch_all(database.pool())
        .await
        .unwrap(),
        vec![
            "ordinal",
            "event_id",
            "assignment_id",
            "status",
            "attempt_count",
            "last_attempt_at",
            "last_error",
            "lease_token",
            "lease_expires_at",
            "delivered_at",
            "created_at",
            "updated_at",
        ]
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT name FROM sqlite_schema WHERE type = 'index' AND tbl_name = 'assignment_event_outbox' AND sql IS NOT NULL ORDER BY name"
        )
        .fetch_all(database.pool())
        .await
        .unwrap(),
        vec!["idx_assignment_event_outbox_pending"]
    );
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
    assert_eq!(catalog_count, 0);
    let system_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM capability_packs WHERE status = 'executable' AND catalog_capability_id IS NULL",
    ).fetch_one(database.pool()).await.unwrap();
    assert_eq!(system_count, 4);

    let executable_registries = sqlx::query_as::<_, (String, String, String)>(
        "SELECT id, required_tools_json, required_engine_capabilities_json \
         FROM capability_packs WHERE status = 'executable' ORDER BY id",
    )
    .fetch_all(database.pool())
    .await
    .unwrap();
    let executable_registries = executable_registries
        .into_iter()
        .map(|(id, tools, engine_capabilities)| {
            let tools = serde_json::from_str::<Vec<String>>(&tools).unwrap();
            let engine_capabilities =
                serde_json::from_str::<Vec<String>>(&engine_capabilities).unwrap();
            for tool in &tools {
                assert!(
                    ["read", "grep", "find", "ls", "edit", "write", "bash"]
                        .contains(&tool.as_str()),
                    "undocumented tool identifier {tool:?}"
                );
            }
            assert!(
                engine_capabilities.is_empty(),
                "executable pack {id} requires undocumented engine capabilities"
            );
            (id, tools, engine_capabilities)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        executable_registries,
        vec![
            (
                "capability-pack:engineering-execution:v1".into(),
                vec!["read", "grep", "find", "ls", "edit", "write", "bash"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                vec![],
            ),
            (
                "capability-pack:independent-review:v1".into(),
                vec!["read", "grep", "find", "ls"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                vec![],
            ),
            (
                "capability-pack:lead-coordination:v1".into(),
                vec!["read", "grep", "find", "ls"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                vec![],
            ),
            (
                "capability-pack:source-research:v1".into(),
                vec!["read", "grep", "find", "ls"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                vec![],
            ),
        ]
    );

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

    let membership = sqlx::query_as::<_, (String, String, String, String, String)>(
        "SELECT role_kind, status, permission_policy, joined_at, updated_at FROM work_agents WHERE work_id = 'legacy-work' AND agent_instance_id = 'agent-instance:piwork-lead'",
    ).fetch_all(&mut connection).await.unwrap();
    assert_eq!(
        membership,
        vec![(
            "lead".into(),
            "joined".into(),
            "inherit_work".into(),
            "2026-01-01T00:00:00Z".into(),
            "2026-01-01T00:00:00Z".into(),
        )]
    );
    let leads = sqlx::query_as::<_, (String, String, String)>(
        "SELECT work_id, agent_instance_id, created_at FROM work_leads WHERE work_id = 'legacy-work'",
    )
    .fetch_all(&mut connection)
    .await
    .unwrap();
    assert_eq!(
        leads,
        vec![(
            "legacy-work".into(),
            "agent-instance:piwork-lead".into(),
            "2026-01-01T00:00:00Z".into(),
        )]
    );
}

#[tokio::test]
async fn agent_domain_constraints_reject_duplicate_versions_and_invalid_json() {
    let database = Database::open_in_memory().await.unwrap();
    let duplicate_role = sqlx::query(
        "INSERT INTO role_templates (id, slug, role_kind, name, description, base_instructions, responsibilities_json, non_responsibilities_json, base_result_contract_json, compatible_capability_kinds_json, builtin, version, created_at, updated_at) SELECT 'other-role-id', slug, role_kind, name, description, base_instructions, responsibilities_json, non_responsibilities_json, base_result_contract_json, compatible_capability_kinds_json, builtin, version, created_at, updated_at FROM role_templates WHERE id = 'role-template:lead:v1'",
    ).execute(database.pool()).await;
    assert_database_error_contains(duplicate_role, "UNIQUE constraint failed");
    let duplicate_definition = sqlx::query(
        "INSERT INTO agent_definitions (id, role_template_id, slug, name, description, instructions, responsibilities_json, non_responsibilities_json, input_contract_json, result_contract_json, quality_rubric_json, default_engine_kind, default_model_configuration_id, default_permission_policy, default_parallelism, memory_policy, builtin, active, version, created_at, updated_at) SELECT 'other-definition-id', role_template_id, slug, name, description, instructions, responsibilities_json, non_responsibilities_json, input_contract_json, result_contract_json, quality_rubric_json, default_engine_kind, default_model_configuration_id, default_permission_policy, default_parallelism, memory_policy, builtin, active, version, created_at, updated_at FROM agent_definitions WHERE id = 'agent-definition:piwork-lead:v1'",
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

#[tokio::test]
async fn agent_domain_json_columns_reject_valid_values_with_wrong_shapes() {
    let database = Database::open_in_memory().await.unwrap();
    for statement in [
        "UPDATE role_templates SET responsibilities_json = '{}' WHERE id = 'role-template:lead:v1'",
        "UPDATE role_templates SET base_result_contract_json = '[]' WHERE id = 'role-template:lead:v1'",
        "UPDATE agent_definitions SET responsibilities_json = '{}' WHERE id = 'agent-definition:piwork-lead:v1'",
        "UPDATE agent_definitions SET input_contract_json = 'null' WHERE id = 'agent-definition:piwork-lead:v1'",
        "UPDATE capability_packs SET required_tools_json = '{}' WHERE id = 'capability-pack:lead-coordination:v1'",
        "UPDATE capability_packs SET input_schema_json = '[]' WHERE id = 'capability-pack:lead-coordination:v1'",
    ] {
        assert_agent_domain_statement_rejected(&database, statement, "CHECK constraint failed")
            .await;
    }
}

#[tokio::test]
async fn agent_domain_json_string_array_elements_are_enforced_on_insert_and_update() {
    let database = Database::open_in_memory().await.unwrap();
    let fixtures = [
        (
            "role_templates",
            "role-template:lead:v1",
            &[
                "id",
                "slug",
                "role_kind",
                "name",
                "description",
                "base_instructions",
                "responsibilities_json",
                "non_responsibilities_json",
                "base_result_contract_json",
                "compatible_capability_kinds_json",
                "builtin",
                "version",
                "created_at",
                "updated_at",
            ][..],
            &[
                "responsibilities_json",
                "non_responsibilities_json",
                "compatible_capability_kinds_json",
            ][..],
        ),
        (
            "agent_definitions",
            "agent-definition:piwork-lead:v1",
            &[
                "id",
                "role_template_id",
                "slug",
                "name",
                "description",
                "instructions",
                "responsibilities_json",
                "non_responsibilities_json",
                "input_contract_json",
                "result_contract_json",
                "quality_rubric_json",
                "default_engine_kind",
                "default_model_configuration_id",
                "default_permission_policy",
                "default_parallelism",
                "memory_policy",
                "builtin",
                "active",
                "version",
                "created_at",
                "updated_at",
            ][..],
            &["responsibilities_json", "non_responsibilities_json"][..],
        ),
        (
            "capability_packs",
            "capability-pack:lead-coordination:v1",
            &[
                "id",
                "catalog_capability_id",
                "name",
                "description",
                "instructions",
                "input_schema_json",
                "output_schema_json",
                "procedure_json",
                "validation_rubric_json",
                "required_tools_json",
                "default_permission_scope",
                "compatible_role_template_ids_json",
                "required_engine_capabilities_json",
                "conflicts_with_capability_pack_ids_json",
                "version",
                "status",
                "created_at",
                "updated_at",
            ][..],
            &[
                "required_tools_json",
                "compatible_role_template_ids_json",
                "required_engine_capabilities_json",
                "conflicts_with_capability_pack_ids_json",
            ][..],
        ),
    ];
    let invalid_values = ["[1]", "[null]", "[\"read\",{}]"];

    for (table, source_id, columns, string_array_columns) in fixtures {
        for (index, target_column) in string_array_columns.iter().enumerate() {
            let invalid_value = invalid_values[index % invalid_values.len()];
            let inserted_id = format!("string-array-invalid-{table}-{index}");
            let selected = columns
                .iter()
                .map(|column| match *column {
                    "id" => format!("'{inserted_id}'"),
                    "slug" => format!("'{inserted_id}'"),
                    column if column == *target_column => format!("'{invalid_value}'"),
                    column => column.to_string(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            let invalid_insert = sqlx::query(&format!(
                "INSERT INTO {table} ({}) SELECT {selected} FROM {table} WHERE id = ?",
                columns.join(", ")
            ))
            .bind(source_id)
            .execute(database.pool())
            .await;
            assert_database_error_contains(
                invalid_insert,
                "JSON string array contains non-text element",
            );

            let invalid_update = sqlx::query(&format!(
                "UPDATE {table} SET {target_column} = ? WHERE id = ?"
            ))
            .bind(invalid_value)
            .bind(source_id)
            .execute(database.pool())
            .await;
            assert_database_error_contains(
                invalid_update,
                "JSON string array contains non-text element",
            );

            for valid_value in ["[]", "[\"read\",\"grep\"]"] {
                sqlx::query(&format!(
                    "UPDATE {table} SET {target_column} = ? WHERE id = ?"
                ))
                .bind(valid_value)
                .bind(source_id)
                .execute(database.pool())
                .await
                .unwrap();
            }

            for (valid_index, valid_value) in ["[]", "[\"read\",\"grep\"]"].iter().enumerate() {
                let allowed_id = format!("string-array-valid-{table}-{index}-{valid_index}");
                let selected = columns
                    .iter()
                    .map(|column| match *column {
                        "id" => format!("'{allowed_id}'"),
                        "slug" => format!("'{allowed_id}'"),
                        column if column == *target_column => format!("'{valid_value}'"),
                        column => column.to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                sqlx::query(&format!(
                    "INSERT INTO {table} ({}) SELECT {selected} FROM {table} WHERE id = ?",
                    columns.join(", ")
                ))
                .bind(source_id)
                .execute(database.pool())
                .await
                .unwrap();
            }
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
        "INSERT INTO role_templates (id, slug, role_kind, name, description, base_instructions, responsibilities_json, non_responsibilities_json, base_result_contract_json, compatible_capability_kinds_json, builtin, version, created_at, updated_at) SELECT 'role-template:test:v1', 'test-role', role_kind, name, description, base_instructions, responsibilities_json, non_responsibilities_json, base_result_contract_json, compatible_capability_kinds_json, 0, version, created_at, updated_at FROM role_templates WHERE id = 'role-template:engineer:v1'",
    ).execute(database.pool()).await.unwrap();
    sqlx::query(
        "INSERT INTO agent_definitions (id, role_template_id, slug, name, description, instructions, responsibilities_json, non_responsibilities_json, input_contract_json, result_contract_json, quality_rubric_json, default_engine_kind, default_model_configuration_id, default_permission_policy, default_parallelism, memory_policy, builtin, active, version, created_at, updated_at) SELECT 'agent-definition:test:v1', 'role-template:test:v1', 'test-definition', name, description, instructions, responsibilities_json, non_responsibilities_json, input_contract_json, result_contract_json, quality_rubric_json, default_engine_kind, default_model_configuration_id, default_permission_policy, default_parallelism, memory_policy, 0, active, version, created_at, updated_at FROM agent_definitions WHERE id = 'agent-definition:piwork-engineer:v1'",
    ).execute(database.pool()).await.unwrap();
    sqlx::query(
        "INSERT INTO agent_instances (id, definition_id, display_name, engine_override, model_configuration_override, permission_policy_override, parallelism_override, builtin, status, created_at, updated_at) SELECT 'agent-instance:test', 'agent-definition:test:v1', display_name, engine_override, model_configuration_override, permission_policy_override, parallelism_override, 0, status, created_at, updated_at FROM agent_instances WHERE id = 'agent-instance:piwork-engineer'",
    ).execute(database.pool()).await.unwrap();
    sqlx::query(
        "INSERT INTO capability_packs (id, catalog_capability_id, name, description, instructions, input_schema_json, output_schema_json, procedure_json, validation_rubric_json, required_tools_json, default_permission_scope, compatible_role_template_ids_json, required_engine_capabilities_json, conflicts_with_capability_pack_ids_json, version, status, created_at, updated_at) SELECT 'capability-pack:test:v1', NULL, name, description, instructions, input_schema_json, output_schema_json, procedure_json, validation_rubric_json, required_tools_json, default_permission_scope, compatible_role_template_ids_json, required_engine_capabilities_json, conflicts_with_capability_pack_ids_json, version, status, created_at, updated_at FROM capability_packs WHERE id = 'capability-pack:engineering-execution:v1'",
    ).execute(database.pool()).await.unwrap();
    sqlx::query(
        "INSERT INTO agent_capability_bindings (agent_definition_id, capability_pack_id, installed_at) VALUES ('agent-definition:test:v1', 'capability-pack:test:v1', '2026-01-01T00:00:00Z')",
    ).execute(database.pool()).await.unwrap();

    assert_agent_domain_statement_rejected(
        &database,
        "DELETE FROM agent_definitions WHERE id = 'agent-definition:test:v1'",
        "FOREIGN KEY constraint failed",
    )
    .await;
    assert_agent_domain_statement_rejected(
        &database,
        "DELETE FROM capability_packs WHERE id = 'capability-pack:test:v1'",
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

    let roles = sqlx::query_as::<_, (String, String, String, String, String)>(
        "SELECT role_kind, responsibilities_json, non_responsibilities_json, base_result_contract_json, compatible_capability_kinds_json \
         FROM role_templates ORDER BY role_kind",
    )
    .fetch_all(database.pool())
    .await
    .unwrap();
    let role_contracts = roles
        .into_iter()
        .map(
            |(kind, responsibilities, non_responsibilities, result, compatible_kinds)| {
                (
                    kind,
                    parse_json(&responsibilities),
                    parse_json(&non_responsibilities),
                    parse_json(&result),
                    parse_json(&compatible_kinds),
                )
            },
        )
        .collect::<Vec<_>>();
    assert_eq!(
        role_contracts,
        vec![
            (
                "engineer".into(),
                serde_json::json!(["implement", "debug", "refactor", "test", "artifacts"]),
                serde_json::json!(["不扩大任务范围", "不隐瞒未验证结果"]),
                serde_json::json!({"changes":"array","tests":"array","artifacts":"array","risks":"array"}),
                serde_json::json!([])
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
                serde_json::json!({"summary":"string","decisions":"array","deliverables":"array","open_risks":"array"}),
                serde_json::json!([])
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
                serde_json::json!({"findings":"array","sources":"array","risks":"array","confidence":"string"}),
                serde_json::json!([])
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
                serde_json::json!({"findings":"array","evidence":"array","verdict":"string"}),
                serde_json::json!([])
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
    assert_database_error_contains(missing_membership, "work lead must be joined lead");

    sqlx::query(
        "INSERT INTO work_agents (work_id, agent_instance_id, role_kind, status, permission_policy, joined_at, updated_at) VALUES ('agent-work', 'agent-instance:piwork-lead', 'lead', 'joined', 'inherit_work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    ).execute(database.pool()).await.unwrap();
    sqlx::query(
        "INSERT INTO work_leads (work_id, agent_instance_id, created_at) VALUES ('agent-work', 'agent-instance:piwork-lead', '2026-01-01T00:00:00Z')",
    ).execute(database.pool()).await.unwrap();
    sqlx::query(
        "INSERT INTO work_agents (work_id, agent_instance_id, role_kind, status, permission_policy, joined_at, updated_at) VALUES ('agent-work', 'agent-instance:piwork-engineer', 'engineer', 'joined', 'inherit_work', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    ).execute(database.pool()).await.unwrap();
    sqlx::query(
        "UPDATE work_agents SET role_kind = 'lead' WHERE work_id = 'agent-work' AND agent_instance_id = 'agent-instance:piwork-engineer'",
    ).execute(database.pool()).await.unwrap();
    let second_lead = sqlx::query(
        "INSERT INTO work_leads (work_id, agent_instance_id, created_at) VALUES ('agent-work', 'agent-instance:piwork-engineer', '2026-01-01T00:00:00Z')",
    ).execute(database.pool()).await;
    assert_database_error_contains(second_lead, "UNIQUE constraint failed");
    sqlx::query(
        "UPDATE work_agents SET role_kind = 'engineer' WHERE work_id = 'agent-work' AND agent_instance_id = 'agent-instance:piwork-engineer'",
    ).execute(database.pool()).await.unwrap();
    let delete_membership = sqlx::query(
        "DELETE FROM work_agents WHERE work_id = 'agent-work' AND agent_instance_id = 'agent-instance:piwork-lead'",
    ).execute(database.pool()).await;
    assert_database_error_contains(delete_membership, "FOREIGN KEY constraint failed");

    sqlx::query("DELETE FROM work_leads WHERE work_id = 'agent-work'")
        .execute(database.pool())
        .await
        .unwrap();
    let engineer_designation = sqlx::query(
        "INSERT INTO work_leads (work_id, agent_instance_id, created_at) VALUES ('agent-work', 'agent-instance:piwork-engineer', '2026-01-01T00:00:00Z')",
    ).execute(database.pool()).await;
    assert_database_error_contains(engineer_designation, "work lead must be joined lead");

    sqlx::query(
        "UPDATE work_agents SET status = 'inactive' WHERE work_id = 'agent-work' AND agent_instance_id = 'agent-instance:piwork-lead'",
    ).execute(database.pool()).await.unwrap();
    let inactive_designation = sqlx::query(
        "INSERT INTO work_leads (work_id, agent_instance_id, created_at) VALUES ('agent-work', 'agent-instance:piwork-lead', '2026-01-01T00:00:00Z')",
    ).execute(database.pool()).await;
    assert_database_error_contains(inactive_designation, "work lead must be joined lead");
    sqlx::query(
        "UPDATE work_agents SET status = 'joined' WHERE work_id = 'agent-work' AND agent_instance_id = 'agent-instance:piwork-lead'",
    ).execute(database.pool()).await.unwrap();
    sqlx::query(
        "INSERT INTO work_leads (work_id, agent_instance_id, created_at) VALUES ('agent-work', 'agent-instance:piwork-lead', '2026-01-01T00:00:00Z')",
    ).execute(database.pool()).await.unwrap();
    let engineer_reassignment = sqlx::query(
        "UPDATE work_leads SET agent_instance_id = 'agent-instance:piwork-engineer' WHERE work_id = 'agent-work'",
    ).execute(database.pool()).await;
    assert_database_error_contains(engineer_reassignment, "work lead must be joined lead");
    for statement in [
        "UPDATE work_agents SET status = 'inactive' WHERE work_id = 'agent-work' AND agent_instance_id = 'agent-instance:piwork-lead'",
        "UPDATE work_agents SET role_kind = 'reviewer' WHERE work_id = 'agent-work' AND agent_instance_id = 'agent-instance:piwork-lead'",
    ] {
        let invalidation = sqlx::query(statement).execute(database.pool()).await;
        assert_database_error_contains(invalidation, "current work lead must remain joined lead");
    }

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

async fn insert_assignment_member(database: &Database, work_id: &str, agent_instance_id: &str) {
    sqlx::query(
        "INSERT INTO work_agents \
         (work_id, agent_instance_id, role_kind, status, permission_policy, joined_at, updated_at) \
         VALUES (?, ?, 'engineer', 'joined', 'inherit_work', ?, ?)",
    )
    .bind(work_id)
    .bind(agent_instance_id)
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await
    .unwrap();
}

async fn try_insert_assignment(
    database: &Database,
    id: &str,
    work_id: &str,
    assigned_agent_id: &str,
    status: &str,
) -> Result<SqliteQueryResult, sqlx::Error> {
    sqlx::query(
        "INSERT INTO assignments ( \
             id, work_id, assigned_agent_id, kind, side_effect, title, instruction, \
             context_manifest_json, expected_result_schema_json, acceptance_criteria_json, \
             permission_scope_json, priority, status, attempt_count, max_attempts, \
             created_at, updated_at \
         ) VALUES (?, ?, ?, 'member', 'read_only', 'Assignment', 'Do the work', \
             '{}', '{}', '[]', '{}', 10, ?, 0, 3, ?, ?)",
    )
    .bind(id)
    .bind(work_id)
    .bind(assigned_agent_id)
    .bind(status)
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await
}

async fn insert_assignment(
    database: &Database,
    id: &str,
    work_id: &str,
    assigned_agent_id: &str,
    status: &str,
) {
    try_insert_assignment(database, id, work_id, assigned_agent_id, status)
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
        ("idx_events_run_sequence", vec!["run_id", "sequence"]),
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
         WHERE name IN ( \
             'idx_events_work_turn_sequence', 'idx_events_assignment_sequence', \
             'idx_events_run_sequence' \
         ) \
         ORDER BY name",
    )
    .fetch_all(database.pool())
    .await
    .unwrap();
    assert_eq!(
        partial_indexes,
        vec![
            ("idx_events_assignment_sequence".into(), 1),
            ("idx_events_run_sequence".into(), 1),
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
        "CREATE UNIQUE INDEX idx_events_assignment_sequence ON events(assignment_id, sequence) \
         WHERE run_id IS NULL AND assignment_id IS NOT NULL"
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

#[tokio::test]
async fn assignment_migration_creates_complete_schema_and_indexes() {
    let database = Database::open_in_memory().await.unwrap();
    let names = database.table_names().await.unwrap();
    for expected in ["assignments", "assignment_dependencies", "agent_sessions"] {
        assert!(names.contains(&expected.to_string()), "missing {expected}");
    }

    let assignment_columns =
        sqlx::query_scalar::<_, String>("SELECT name FROM pragma_table_info('assignments')")
            .fetch_all(database.pool())
            .await
            .unwrap();
    for expected in [
        "id",
        "work_id",
        "parent_assignment_id",
        "created_by_agent_id",
        "assigned_agent_id",
        "capability_pack_id",
        "kind",
        "side_effect",
        "title",
        "instruction",
        "context_manifest_json",
        "expected_result_schema_json",
        "acceptance_criteria_json",
        "permission_scope_json",
        "priority",
        "status",
        "attempt_count",
        "max_attempts",
        "not_before",
        "result_summary",
        "last_error",
        "next_attempt_at",
        "recovery_reason",
        "runtime_owner_id",
        "created_at",
        "claimed_at",
        "started_at",
        "completed_at",
        "updated_at",
    ] {
        assert!(
            assignment_columns.contains(&expected.to_string()),
            "missing assignments.{expected}"
        );
    }

    let run_columns = sqlx::query_scalar::<_, String>("SELECT name FROM pragma_table_info('runs')")
        .fetch_all(database.pool())
        .await
        .unwrap();
    for expected in ["assignment_id", "agent_instance_id", "attempt_number"] {
        assert!(
            run_columns.contains(&expected.to_string()),
            "missing runs.{expected}"
        );
    }

    let event_run_not_null: i64 = sqlx::query_scalar(
        "SELECT \"notnull\" FROM pragma_table_info('events') WHERE name = 'run_id'",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(event_run_not_null, 0);

    for expected in [
        "idx_assignments_one_inflight_per_work",
        "idx_assignments_schedulable",
        "idx_events_run_sequence",
        "idx_events_assignment_sequence",
        "idx_runs_assignment_attempt",
    ] {
        let exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'index' AND name = ?",
        )
        .bind(expected)
        .fetch_one(database.pool())
        .await
        .unwrap();
        assert_eq!(exists, 1, "missing index {expected}");
    }

    let session_key_columns = sqlx::query_scalar::<_, String>(
        "SELECT name FROM pragma_index_info( \
             (SELECT name FROM pragma_index_list('agent_sessions') \
              WHERE \"unique\" = 1 AND origin = 'u' LIMIT 1) \
         ) ORDER BY seqno",
    )
    .fetch_all(database.pool())
    .await
    .unwrap();
    assert_eq!(
        session_key_columns,
        ["agent_instance_id", "work_id", "engine_kind", "generation"]
    );
}

#[tokio::test]
async fn assignment_rows_dependencies_and_agent_sessions_round_trip() {
    let database = Database::open_in_memory().await.unwrap();
    insert_work(&database, "assignment-work").await;
    insert_assignment_member(
        &database,
        "assignment-work",
        "agent-instance:piwork-engineer",
    )
    .await;
    insert_assignment(
        &database,
        "assignment-parent",
        "assignment-work",
        "agent-instance:piwork-engineer",
        "queued",
    )
    .await;
    insert_assignment(
        &database,
        "assignment-child",
        "assignment-work",
        "agent-instance:piwork-engineer",
        "waiting",
    )
    .await;
    sqlx::query(
        "UPDATE assignments SET parent_assignment_id = 'assignment-parent', \
             created_by_agent_id = 'agent-instance:piwork-engineer', \
             capability_pack_id = 'capability-pack:engineering-execution:v1', \
             not_before = '2026-01-01T00:01:00Z', result_summary = 'pending', \
             last_error = 'transient', next_attempt_at = '2026-01-01T00:02:00Z', \
             recovery_reason = 'resume safely' WHERE id = 'assignment-child'",
    )
    .execute(database.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO assignment_dependencies (assignment_id, depends_on_assignment_id) \
         VALUES ('assignment-child', 'assignment-parent')",
    )
    .execute(database.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO agent_sessions ( \
             id, work_id, agent_instance_id, engine_kind, engine_reference, generation, \
             current_assignment_id, last_successful_turn_id, status, rotation_reason, \
             created_at, updated_at \
         ) VALUES ('session-1', 'assignment-work', 'agent-instance:piwork-engineer', \
             'pi', 'opaque-engine-reference', 1, 'assignment-child', 'turn-1', \
             'running', 'continued', '2026-01-01T00:00:00Z', '2026-01-01T00:03:00Z')",
    )
    .execute(database.pool())
    .await
    .unwrap();

    let assignment = sqlx::query_as::<_, (String, String, String, String, String)>(
        "SELECT parent_assignment_id, created_by_agent_id, capability_pack_id, status, \
             recovery_reason FROM assignments WHERE id = 'assignment-child'",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(
        assignment,
        (
            "assignment-parent".into(),
            "agent-instance:piwork-engineer".into(),
            "capability-pack:engineering-execution:v1".into(),
            "waiting".into(),
            "resume safely".into(),
        )
    );
    let session = sqlx::query_as::<_, (String, String, i64, String, String)>(
        "SELECT engine_reference, current_assignment_id, generation, \
             last_successful_turn_id, status FROM agent_sessions WHERE id = 'session-1'",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(
        session,
        (
            "opaque-engine-reference".into(),
            "assignment-child".into(),
            1,
            "turn-1".into(),
            "running".into(),
        )
    );
}

#[tokio::test]
async fn assignment_agent_session_engine_reference_attaches_after_creation() {
    let database = Database::open_in_memory().await.unwrap();
    insert_work(&database, "assignment-session-attach").await;
    insert_assignment_member(
        &database,
        "assignment-session-attach",
        "agent-instance:piwork-engineer",
    )
    .await;
    insert_assignment(
        &database,
        "assignment-session-attach-1",
        "assignment-session-attach",
        "agent-instance:piwork-engineer",
        "queued",
    )
    .await;
    sqlx::query(
        "INSERT INTO agent_sessions (id, work_id, agent_instance_id, engine_kind, \
             engine_reference, generation, current_assignment_id, status, created_at, updated_at) \
         VALUES ('assignment-session-pending', 'assignment-session-attach', \
             'agent-instance:piwork-engineer', 'pi', NULL, 1, \
             'assignment-session-attach-1', 'ready', ?, ?)",
    )
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await
    .unwrap();

    let before_attach: Option<String> = sqlx::query_scalar(
        "SELECT engine_reference FROM agent_sessions WHERE id = 'assignment-session-pending'",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(before_attach, None);

    let empty_attach = sqlx::query(
        "UPDATE agent_sessions SET engine_reference = '' \
         WHERE id = 'assignment-session-pending'",
    )
    .execute(database.pool())
    .await;
    assert_database_error_contains(empty_attach, "CHECK constraint failed");

    sqlx::query(
        "UPDATE agent_sessions SET engine_reference = 'opaque-engine-reference' \
         WHERE id = 'assignment-session-pending'",
    )
    .execute(database.pool())
    .await
    .unwrap();
    let attached: String = sqlx::query_scalar(
        "SELECT engine_reference FROM agent_sessions WHERE id = 'assignment-session-pending'",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(attached, "opaque-engine-reference");
}

#[tokio::test]
async fn assignment_value_constraints_reject_invalid_wire_values_json_and_counters() {
    let database = Database::open_in_memory().await.unwrap();
    insert_work(&database, "assignment-constraints").await;
    insert_assignment_member(
        &database,
        "assignment-constraints",
        "agent-instance:piwork-engineer",
    )
    .await;
    insert_assignment(
        &database,
        "assignment-valid",
        "assignment-constraints",
        "agent-instance:piwork-engineer",
        "queued",
    )
    .await;

    for statement in [
        "UPDATE assignments SET kind = 'worker' WHERE id = 'assignment-valid'",
        "UPDATE assignments SET side_effect = 'write' WHERE id = 'assignment-valid'",
        "UPDATE assignments SET status = 'retrying' WHERE id = 'assignment-valid'",
        "UPDATE assignments SET context_manifest_json = '[]' WHERE id = 'assignment-valid'",
        "UPDATE assignments SET expected_result_schema_json = '[]' WHERE id = 'assignment-valid'",
        "UPDATE assignments SET acceptance_criteria_json = '{}' WHERE id = 'assignment-valid'",
        "UPDATE assignments SET permission_scope_json = '[]' WHERE id = 'assignment-valid'",
        "UPDATE assignments SET context_manifest_json = 'not-json' WHERE id = 'assignment-valid'",
        "UPDATE assignments SET priority = -1 WHERE id = 'assignment-valid'",
        "UPDATE assignments SET priority = 4294967296 WHERE id = 'assignment-valid'",
        "UPDATE assignments SET attempt_count = -1 WHERE id = 'assignment-valid'",
        "UPDATE assignments SET attempt_count = 4 WHERE id = 'assignment-valid'",
        "UPDATE assignments SET attempt_count = 4294967296 WHERE id = 'assignment-valid'",
        "UPDATE assignments SET max_attempts = 0 WHERE id = 'assignment-valid'",
        "UPDATE assignments SET max_attempts = 4294967296 WHERE id = 'assignment-valid'",
        "UPDATE assignments SET title = '' WHERE id = 'assignment-valid'",
        "UPDATE assignments SET instruction = '   ' WHERE id = 'assignment-valid'",
        "UPDATE assignments SET updated_at = '2025-12-31T23:59:59Z' WHERE id = 'assignment-valid'",
        "UPDATE assignments SET not_before = '2025-12-31T23:59:59Z' WHERE id = 'assignment-valid'",
        "UPDATE assignments SET claimed_at = '2025-12-31T23:59:59Z' WHERE id = 'assignment-valid'",
        "UPDATE assignments SET started_at = '2025-12-31T23:59:59Z' WHERE id = 'assignment-valid'",
        "UPDATE assignments SET completed_at = '2025-12-31T23:59:59Z' WHERE id = 'assignment-valid'",
        "UPDATE assignments SET next_attempt_at = '2025-12-31T23:59:59Z' WHERE id = 'assignment-valid'",
    ] {
        let result = sqlx::query(statement).execute(database.pool()).await;
        assert_database_error_contains(result, "CHECK constraint failed");
    }
}

#[tokio::test]
async fn assignment_relationship_inflight_and_dependency_constraints_are_enforced() {
    let database = Database::open_in_memory().await.unwrap();
    for work_id in ["assignment-work-a", "assignment-work-b"] {
        insert_work(&database, work_id).await;
        insert_assignment_member(&database, work_id, "agent-instance:piwork-engineer").await;
    }
    insert_assignment(
        &database,
        "assignment-a",
        "assignment-work-a",
        "agent-instance:piwork-engineer",
        "queued",
    )
    .await;
    insert_assignment(
        &database,
        "assignment-b",
        "assignment-work-b",
        "agent-instance:piwork-engineer",
        "queued",
    )
    .await;

    let missing_member = try_insert_assignment(
        &database,
        "assignment-unowned",
        "assignment-work-a",
        "agent-instance:piwork-reviewer",
        "queued",
    )
    .await;
    assert_database_error_contains(missing_member, "FOREIGN KEY constraint failed");

    for statement in [
        "UPDATE assignments SET parent_assignment_id = id WHERE id = 'assignment-a'",
        "UPDATE assignments SET parent_assignment_id = 'assignment-b' WHERE id = 'assignment-a'",
        "UPDATE assignments SET created_by_agent_id = 'agent-instance:piwork-reviewer' WHERE id = 'assignment-a'",
        "UPDATE assignments SET capability_pack_id = 'missing-pack' WHERE id = 'assignment-a'",
        "INSERT INTO assignment_dependencies (assignment_id, depends_on_assignment_id) VALUES ('assignment-a', 'assignment-a')",
        "INSERT INTO assignment_dependencies (assignment_id, depends_on_assignment_id) VALUES ('assignment-a', 'missing-assignment')",
    ] {
        let result = sqlx::query(statement).execute(database.pool()).await;
        let expected = if statement.contains("id WHERE")
            || statement.contains("assignment-a', 'assignment-a")
        {
            "CHECK constraint failed"
        } else {
            "FOREIGN KEY constraint failed"
        };
        assert_database_error_contains(result, expected);
    }

    sqlx::query(
        "INSERT INTO assignment_dependencies (assignment_id, depends_on_assignment_id) \
         VALUES ('assignment-a', 'assignment-b')",
    )
    .execute(database.pool())
    .await
    .unwrap();

    sqlx::query("UPDATE assignments SET status = 'claimed' WHERE id = 'assignment-a'")
        .execute(database.pool())
        .await
        .unwrap();
    let second_inflight = try_insert_assignment(
        &database,
        "assignment-a-2",
        "assignment-work-a",
        "agent-instance:piwork-engineer",
        "running",
    )
    .await;
    assert_database_error_contains(second_inflight, "UNIQUE constraint failed");
}

#[tokio::test]
async fn assignment_parent_restricts_direct_delete_but_work_delete_cascades_the_graph() {
    let database = Database::open_in_memory().await.unwrap();
    insert_work(&database, "assignment-cascade-work").await;
    insert_assignment_member(
        &database,
        "assignment-cascade-work",
        "agent-instance:piwork-engineer",
    )
    .await;
    insert_assignment(
        &database,
        "assignment-cascade-parent",
        "assignment-cascade-work",
        "agent-instance:piwork-engineer",
        "queued",
    )
    .await;
    insert_assignment(
        &database,
        "assignment-cascade-child",
        "assignment-cascade-work",
        "agent-instance:piwork-engineer",
        "queued",
    )
    .await;
    sqlx::query(
        "UPDATE assignments SET parent_assignment_id = 'assignment-cascade-parent' \
         WHERE id = 'assignment-cascade-child'",
    )
    .execute(database.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO assignment_dependencies (assignment_id, depends_on_assignment_id) \
         VALUES ('assignment-cascade-child', 'assignment-cascade-parent')",
    )
    .execute(database.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO agent_sessions (id, work_id, agent_instance_id, engine_kind, \
             engine_reference, generation, current_assignment_id, status, created_at, updated_at) \
         VALUES ('assignment-cascade-session', 'assignment-cascade-work', \
             'agent-instance:piwork-engineer', 'pi', 'engine-reference', 1, \
             'assignment-cascade-child', 'ready', ?, ?)",
    )
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO runs (id, work_id, engine_kind, model_label, status, assignment_id, \
             agent_instance_id, attempt_number, created_at, updated_at) \
         VALUES ('assignment-cascade-run', 'assignment-cascade-work', 'pi', 'test', 'queued', \
             'assignment-cascade-child', 'agent-instance:piwork-engineer', 1, ?, ?)",
    )
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO events (id, work_id, run_id, assignment_id, sequence, version, occurred_at, payload) \
         VALUES ('assignment-cascade-event', 'assignment-cascade-work', \
             'assignment-cascade-run', 'assignment-cascade-child', 1, 1, ?, '{}')",
    )
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await
    .unwrap();

    let direct_parent_delete =
        sqlx::query("DELETE FROM assignments WHERE id = 'assignment-cascade-parent'")
            .execute(database.pool())
            .await;
    assert_database_error_contains(direct_parent_delete, "FOREIGN KEY constraint failed");

    sqlx::query("DELETE FROM works WHERE id = 'assignment-cascade-work'")
        .execute(database.pool())
        .await
        .unwrap();
    for (table, statement) in [
        (
            "assignments",
            "SELECT COUNT(*) FROM assignments WHERE work_id = 'assignment-cascade-work'",
        ),
        (
            "assignment_dependencies",
            "SELECT COUNT(*) FROM assignment_dependencies \
             WHERE assignment_id LIKE 'assignment-cascade-%' \
                OR depends_on_assignment_id LIKE 'assignment-cascade-%'",
        ),
        (
            "agent_sessions",
            "SELECT COUNT(*) FROM agent_sessions WHERE work_id = 'assignment-cascade-work'",
        ),
        (
            "runs",
            "SELECT COUNT(*) FROM runs WHERE work_id = 'assignment-cascade-work'",
        ),
        (
            "events",
            "SELECT COUNT(*) FROM events WHERE work_id = 'assignment-cascade-work'",
        ),
    ] {
        let count: i64 = sqlx::query_scalar(statement)
            .fetch_one(database.pool())
            .await
            .unwrap();
        assert_eq!(count, 0, "rows remain in {table}");
    }
}

#[tokio::test]
async fn assignment_agent_session_and_run_identity_constraints_are_enforced() {
    let database = Database::open_in_memory().await.unwrap();
    insert_work(&database, "assignment-execution").await;
    for agent in [
        "agent-instance:piwork-engineer",
        "agent-instance:piwork-reviewer",
    ] {
        insert_assignment_member(&database, "assignment-execution", agent).await;
    }
    insert_assignment(
        &database,
        "assignment-execution-1",
        "assignment-execution",
        "agent-instance:piwork-engineer",
        "queued",
    )
    .await;

    sqlx::query(
        "INSERT INTO agent_sessions (id, work_id, agent_instance_id, engine_kind, \
             engine_reference, generation, status, created_at, updated_at) \
         VALUES ('session-valid', 'assignment-execution', 'agent-instance:piwork-engineer', \
             'pi', 'opaque', 1, 'ready', ?, ?)",
    )
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await
    .unwrap();
    for statement in [
        "INSERT INTO agent_sessions (id, work_id, agent_instance_id, engine_kind, engine_reference, generation, status, created_at, updated_at) VALUES ('session-duplicate', 'assignment-execution', 'agent-instance:piwork-engineer', 'pi', 'other', 1, 'ready', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        "INSERT INTO agent_sessions (id, work_id, agent_instance_id, engine_kind, engine_reference, generation, status, created_at, updated_at) VALUES ('session-no-member', 'assignment-execution', 'agent-instance:piwork-lead', 'pi', 'opaque', 1, 'ready', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        "INSERT INTO agent_sessions (id, work_id, agent_instance_id, engine_kind, engine_reference, generation, current_assignment_id, status, created_at, updated_at) VALUES ('session-wrong-owner', 'assignment-execution', 'agent-instance:piwork-reviewer', 'pi', 'opaque', 1, 'assignment-execution-1', 'ready', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        "UPDATE agent_sessions SET generation = 0 WHERE id = 'session-valid'",
        "UPDATE agent_sessions SET generation = 4294967296 WHERE id = 'session-valid'",
        "UPDATE agent_sessions SET status = 'paused' WHERE id = 'session-valid'",
        "UPDATE agent_sessions SET status = 'invalidated' WHERE id = 'session-valid'",
        "UPDATE agent_sessions SET updated_at = '2025-12-31T00:00:00Z' WHERE id = 'session-valid'",
        "UPDATE agent_sessions SET invalidated_at = '2025-12-31T00:00:00Z' WHERE id = 'session-valid'",
    ] {
        let result = sqlx::query(statement).execute(database.pool()).await;
        let expected = if statement.contains("generation = ")
            || statement.contains("status = 'paused'")
            || statement.contains("status = 'invalidated'")
            || statement.contains("SET updated_at")
            || statement.contains("SET invalidated_at")
        {
            "CHECK constraint failed"
        } else if statement.contains("session-duplicate") {
            "UNIQUE constraint failed"
        } else {
            "FOREIGN KEY constraint failed"
        };
        assert_database_error_contains(result, expected);
    }

    insert_run(&database, "legacy-run", "assignment-execution").await;
    sqlx::query(
        "INSERT INTO runs (id, work_id, engine_kind, model_label, status, \
             assignment_id, agent_instance_id, attempt_number, created_at, updated_at) \
         VALUES ('assignment-run-1', 'assignment-execution', 'pi', 'test', 'queued', \
             'assignment-execution-1', 'agent-instance:piwork-engineer', 1, ?, ?)",
    )
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await
    .unwrap();
    for statement in [
        "INSERT INTO runs (id, work_id, engine_kind, model_label, status, assignment_id, agent_instance_id, attempt_number, created_at, updated_at) VALUES ('assignment-run-duplicate', 'assignment-execution', 'pi', 'test', 'queued', 'assignment-execution-1', 'agent-instance:piwork-engineer', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        "INSERT INTO runs (id, work_id, engine_kind, model_label, status, assignment_id, attempt_number, created_at, updated_at) VALUES ('assignment-run-partial', 'assignment-execution', 'pi', 'test', 'queued', 'assignment-execution-1', 2, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        "INSERT INTO runs (id, work_id, engine_kind, model_label, status, assignment_id, agent_instance_id, attempt_number, created_at, updated_at) VALUES ('assignment-run-zero', 'assignment-execution', 'pi', 'test', 'queued', 'assignment-execution-1', 'agent-instance:piwork-engineer', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        "INSERT INTO runs (id, work_id, engine_kind, model_label, status, assignment_id, agent_instance_id, attempt_number, created_at, updated_at) VALUES ('assignment-run-too-large', 'assignment-execution', 'pi', 'test', 'queued', 'assignment-execution-1', 'agent-instance:piwork-engineer', 4294967296, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        "INSERT INTO runs (id, work_id, engine_kind, model_label, status, assignment_id, agent_instance_id, attempt_number, created_at, updated_at) VALUES ('assignment-run-wrong-agent', 'assignment-execution', 'pi', 'test', 'queued', 'assignment-execution-1', 'agent-instance:piwork-reviewer', 2, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    ] {
        let result = sqlx::query(statement).execute(database.pool()).await;
        let expected = if statement.contains("duplicate") {
            "UNIQUE constraint failed"
        } else if statement.contains("partial")
            || statement.contains("zero")
            || statement.contains("too-large")
        {
            "CHECK constraint failed"
        } else {
            "FOREIGN KEY constraint failed"
        };
        assert_database_error_contains(result, expected);
    }
}

#[tokio::test]
async fn assignment_events_support_pre_run_round_trip_and_scoped_sequences() {
    let database = Database::open_in_memory().await.unwrap();
    insert_work(&database, "assignment-events").await;
    insert_assignment_member(
        &database,
        "assignment-events",
        "agent-instance:piwork-engineer",
    )
    .await;
    insert_assignment(
        &database,
        "assignment-event-1",
        "assignment-events",
        "agent-instance:piwork-engineer",
        "queued",
    )
    .await;
    let payload = r#"{"type":"assignmentQueued","assignmentId":"assignment-event-1","assignedAgentId":"agent-instance:piwork-engineer","title":"Investigate","priority":10}"#;
    sqlx::query(
        "INSERT INTO events (id, work_id, run_id, assignment_id, sequence, version, \
             occurred_at, payload, turn_id, session_id, agent_id, causation_id, correlation_id) \
         VALUES ('assignment-event', 'assignment-events', NULL, 'assignment-event-1', 1, 1, \
             ?, ?, 'turn-1', 'session-1', 'agent-instance:piwork-engineer', 'cause-1', 'correlation-1')",
    )
    .bind("2026-01-01T00:00:00Z")
    .bind(payload)
    .execute(database.pool())
    .await
    .unwrap();
    let stored = sqlx::query_as::<_, (Option<String>, String, String, String, String, String)>(
        "SELECT run_id, assignment_id, payload, turn_id, causation_id, correlation_id \
         FROM events WHERE id = 'assignment-event'",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(stored.0, None);
    assert_eq!(stored.1, "assignment-event-1");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&stored.2).unwrap()["type"],
        "assignmentQueued"
    );
    assert_eq!(
        (&stored.3, &stored.4, &stored.5),
        (&"turn-1".into(), &"cause-1".into(), &"correlation-1".into())
    );

    let duplicate = sqlx::query(
        "INSERT INTO events (id, work_id, assignment_id, sequence, version, occurred_at, payload) \
         VALUES ('assignment-event-duplicate', 'assignment-events', 'assignment-event-1', 1, 1, ?, '{}')",
    )
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await;
    assert_database_error_contains(duplicate, "UNIQUE constraint failed");

    let no_identity = sqlx::query(
        "INSERT INTO events (id, work_id, run_id, assignment_id, sequence, version, occurred_at, payload) \
         VALUES ('identity-less-event', 'assignment-events', NULL, NULL, 2, 1, ?, '{}')",
    )
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await;
    assert_database_error_contains(no_identity, "CHECK constraint failed");

    let missing_assignment = sqlx::query(
        "INSERT INTO events (id, work_id, assignment_id, sequence, version, occurred_at, payload) \
         VALUES ('missing-assignment-event', 'assignment-events', 'missing-assignment', 2, 1, ?, '{}')",
    )
    .bind("2026-01-01T00:00:00Z")
    .execute(database.pool())
    .await;
    assert_database_error_contains(missing_assignment, "FOREIGN KEY constraint failed");

    insert_run(&database, "assignment-legacy-run", "assignment-events").await;
    insert_event(
        &database,
        "assignment-legacy-event",
        "assignment-events",
        "assignment-legacy-run",
        1,
    )
    .await;
    let legacy: (String, i64) =
        sqlx::query_as("SELECT run_id, sequence FROM events WHERE id = 'assignment-legacy-event'")
            .fetch_one(database.pool())
            .await
            .unwrap();
    assert_eq!(legacy, ("assignment-legacy-run".into(), 1));
}

#[tokio::test]
async fn assignment_migration_upgrades_legacy_runs_messages_and_events() {
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
         VALUES ('legacy-assignment-work', 'Legacy', 'Goal', '/workspace', 'balanced', 'draft', ?, ?)",
    )
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(&mut connection)
    .await
    .unwrap();
    apply_agent_domain_migration(
        &mut connection,
        5,
        "agent domain",
        include_str!("../migrations/0005_agent_domain.sql"),
    )
    .await;
    sqlx::query(
        "INSERT INTO runs (id, work_id, engine_kind, engine_session_id, model_label, status, created_at, updated_at) \
         VALUES ('legacy-assignment-run', 'legacy-assignment-work', 'pi', 'engine-session', 'model', 'completed', ?, ?)",
    )
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:01:00Z")
    .execute(&mut connection)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO messages (id, work_id, run_id, role, content, created_at) \
         VALUES ('legacy-assignment-message', 'legacy-assignment-work', 'legacy-assignment-run', 'assistant', 'done', ?)",
    )
    .bind("2026-01-01T00:01:00Z")
    .execute(&mut connection)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO resource_blobs (id, plaintext_sha256, size, created_at) \
         VALUES ('legacy-assignment-blob', ?, 4, ?)",
    )
    .bind("a".repeat(64))
    .bind("2026-01-01T00:00:00Z")
    .execute(&mut connection)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO managed_resources (id, space_id, blob_id, original_name, media_type, size, \
             origin, status, created_at, updated_at) \
         VALUES ('legacy-assignment-resource', 'local-personal', 'legacy-assignment-blob', \
             'legacy.txt', 'text/plain', 4, 'user_upload', 'ready', ?, ?)",
    )
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(&mut connection)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO resource_links (id, resource_id, work_id, draft_id, message_id, run_id, role, created_at) \
         VALUES ('legacy-assignment-link', 'legacy-assignment-resource', 'legacy-assignment-work', \
             NULL, 'legacy-assignment-message', 'legacy-assignment-run', 'attached', ?)",
    )
    .bind("2026-01-01T00:01:00Z")
    .execute(&mut connection)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO events (id, work_id, run_id, sequence, version, occurred_at, payload, \
             turn_id, session_id, agent_id, assignment_id, causation_id, correlation_id) \
         VALUES ('legacy-assignment-event', 'legacy-assignment-work', 'legacy-assignment-run', 1, 1, ?, '{}', \
             'turn-legacy', 'session-legacy', 'agent-legacy', 'legacy-event-assignment', \
             'cause-legacy', 'correlation-legacy')",
    )
    .bind("2026-01-01T00:01:00Z")
    .execute(&mut connection)
    .await
    .unwrap();

    let migration_sql = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("migrations/0006_assignment_sessions.sql"),
    )
    .unwrap();
    let migration = Migration::new(
        6,
        Cow::Borrowed("assignment sessions"),
        MigrationType::Simple,
        Cow::Owned(migration_sql),
        false,
    );
    connection.apply(&migration).await.unwrap();

    let run = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, Option<i64>)>(
        "SELECT engine_session_id, status, assignment_id, agent_instance_id, attempt_number \
         FROM runs WHERE id = 'legacy-assignment-run'",
    )
    .fetch_one(&mut connection)
    .await
    .unwrap();
    assert_eq!(
        run,
        (
            "engine-session".into(),
            "completed".into(),
            None,
            None,
            None
        )
    );
    let message: (String, String) = sqlx::query_as(
        "SELECT run_id, content FROM messages WHERE id = 'legacy-assignment-message'",
    )
    .fetch_one(&mut connection)
    .await
    .unwrap();
    assert_eq!(message, ("legacy-assignment-run".into(), "done".into()));
    let resource_link: (String, String, String) = sqlx::query_as(
        "SELECT resource_id, message_id, run_id FROM resource_links \
         WHERE id = 'legacy-assignment-link'",
    )
    .fetch_one(&mut connection)
    .await
    .unwrap();
    assert_eq!(
        resource_link,
        (
            "legacy-assignment-resource".into(),
            "legacy-assignment-message".into(),
            "legacy-assignment-run".into(),
        )
    );
    let event = sqlx::query_as::<_, (String, String, String, String, String, String, String)>(
        "SELECT run_id, turn_id, session_id, agent_id, assignment_id, causation_id, correlation_id \
         FROM events WHERE id = 'legacy-assignment-event'",
    )
    .fetch_one(&mut connection)
    .await
    .unwrap();
    let mapped_legacy_assignment_id = format!(
        "legacy-assignment:{}:{}:{}",
        "legacy-assignment-work".len(),
        "legacy-assignment-work",
        "legacy-event-assignment"
    );
    assert_eq!(
        event,
        (
            "legacy-assignment-run".into(),
            "turn-legacy".into(),
            "session-legacy".into(),
            "agent-legacy".into(),
            mapped_legacy_assignment_id.clone(),
            "cause-legacy".into(),
            "correlation-legacy".into(),
        )
    );
    let legacy_assignment: (String, String, String, String) = sqlx::query_as(
        "SELECT assigned_agent_id, side_effect, status, context_manifest_json \
         FROM assignments WHERE id = ?",
    )
    .bind(&mapped_legacy_assignment_id)
    .fetch_one(&mut connection)
    .await
    .unwrap();
    assert_eq!(legacy_assignment.0, "agent-instance:piwork-lead");
    assert_eq!(legacy_assignment.1, "unknown");
    assert_eq!(legacy_assignment.2, "interrupted");
    let legacy_context = serde_json::from_str::<serde_json::Value>(&legacy_assignment.3).unwrap();
    assert_eq!(legacy_context["legacy"], true);
    assert_eq!(
        legacy_context["originalAssignmentId"],
        "legacy-event-assignment"
    );
    assert_eq!(legacy_context["workId"], "legacy-assignment-work");
    let foreign_key_violations: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_foreign_key_check")
            .fetch_one(&mut connection)
            .await
            .unwrap();
    assert_eq!(foreign_key_violations, 0);
    let quick_check: String = sqlx::query_scalar("PRAGMA quick_check")
        .fetch_one(&mut connection)
        .await
        .unwrap();
    assert_eq!(quick_check, "ok");
}

#[tokio::test]
async fn assignment_migration_remaps_same_legacy_id_per_work_without_losing_resources() {
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
    let work_ids = ["legacy-collision-work-a", "legacy-collision-work-b"];
    for work_id in work_ids {
        sqlx::query(
            "INSERT INTO works (id, title, goal, root_path, permission_mode, status, created_at, updated_at) \
             VALUES (?, 'Legacy', 'Goal', '/workspace', 'balanced', 'draft', ?, ?)",
        )
        .bind(work_id)
        .bind("2026-01-01T00:00:00Z")
        .bind("2026-01-01T00:00:00Z")
        .execute(&mut connection)
        .await
        .unwrap();
    }
    apply_agent_domain_migration(
        &mut connection,
        5,
        "agent domain",
        include_str!("../migrations/0005_agent_domain.sql"),
    )
    .await;

    for (suffix, work_id, hash_character) in [("a", work_ids[0], "b"), ("b", work_ids[1], "c")] {
        let run_id = format!("legacy-collision-run-{suffix}");
        let message_id = format!("legacy-collision-message-{suffix}");
        let blob_id = format!("legacy-collision-blob-{suffix}");
        let resource_id = format!("legacy-collision-resource-{suffix}");
        sqlx::query(
            "INSERT INTO runs (id, work_id, engine_kind, model_label, status, created_at, updated_at) \
             VALUES (?, ?, 'pi', 'model', 'completed', ?, ?)",
        )
        .bind(&run_id)
        .bind(work_id)
        .bind("2026-01-01T00:00:00Z")
        .bind("2026-01-01T00:01:00Z")
        .execute(&mut connection)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO messages (id, work_id, run_id, role, content, created_at) \
             VALUES (?, ?, ?, 'assistant', 'done', ?)",
        )
        .bind(&message_id)
        .bind(work_id)
        .bind(&run_id)
        .bind("2026-01-01T00:01:00Z")
        .execute(&mut connection)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO resource_blobs (id, plaintext_sha256, size, created_at) \
             VALUES (?, ?, 4, ?)",
        )
        .bind(&blob_id)
        .bind(hash_character.repeat(64))
        .bind("2026-01-01T00:00:00Z")
        .execute(&mut connection)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO managed_resources (id, space_id, blob_id, original_name, media_type, size, \
                 origin, status, created_at, updated_at) \
             VALUES (?, 'local-personal', ?, 'legacy.txt', 'text/plain', 4, \
                 'user_upload', 'ready', ?, ?)",
        )
        .bind(&resource_id)
        .bind(&blob_id)
        .bind("2026-01-01T00:00:00Z")
        .bind("2026-01-01T00:00:00Z")
        .execute(&mut connection)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO resource_links (id, resource_id, work_id, draft_id, message_id, run_id, role, created_at) \
             VALUES (?, ?, ?, NULL, ?, ?, 'attached', ?)",
        )
        .bind(format!("legacy-collision-link-{suffix}"))
        .bind(&resource_id)
        .bind(work_id)
        .bind(&message_id)
        .bind(&run_id)
        .bind("2026-01-01T00:01:00Z")
        .execute(&mut connection)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO events (id, work_id, run_id, sequence, version, occurred_at, payload, assignment_id) \
             VALUES (?, ?, ?, 1, 1, ?, '{}', 'shared-legacy-assignment')",
        )
        .bind(format!("legacy-collision-event-{suffix}"))
        .bind(work_id)
        .bind(&run_id)
        .bind("2026-01-01T00:01:00Z")
        .execute(&mut connection)
        .await
        .unwrap();
    }

    let migration_sql = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("migrations/0006_assignment_sessions.sql"),
    )
    .unwrap();
    let migration = Migration::new(
        6,
        Cow::Borrowed("assignment sessions"),
        MigrationType::Simple,
        Cow::Owned(migration_sql),
        false,
    );
    connection.apply(&migration).await.unwrap();

    let event_assignments = sqlx::query_as::<_, (String, String)>(
        "SELECT work_id, assignment_id FROM events ORDER BY work_id",
    )
    .fetch_all(&mut connection)
    .await
    .unwrap();
    let expected_assignments = work_ids
        .into_iter()
        .map(|work_id| {
            (
                work_id.to_string(),
                format!(
                    "legacy-assignment:{}:{}:{}",
                    work_id.len(),
                    work_id,
                    "shared-legacy-assignment"
                ),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(event_assignments, expected_assignments);

    let anchors = sqlx::query_as::<_, (String, String, String)>(
        "SELECT id, work_id, context_manifest_json FROM assignments ORDER BY work_id",
    )
    .fetch_all(&mut connection)
    .await
    .unwrap();
    assert_eq!(anchors.len(), 2);
    for ((id, work_id, context), (_, expected_id)) in anchors.iter().zip(&expected_assignments) {
        assert_eq!(id, expected_id);
        let context = serde_json::from_str::<serde_json::Value>(context).unwrap();
        assert_eq!(context["legacy"], true);
        assert_eq!(context["originalAssignmentId"], "shared-legacy-assignment");
        assert_eq!(context["workId"], work_id.as_str());
    }

    for table in ["runs", "messages", "events", "resource_links"] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&mut connection)
            .await
            .unwrap();
        assert_eq!(count, 2, "legacy rows lost from {table}");
    }
    let links = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT work_id, resource_id, message_id, run_id \
         FROM resource_links ORDER BY work_id",
    )
    .fetch_all(&mut connection)
    .await
    .unwrap();
    assert_eq!(
        links,
        vec![
            (
                work_ids[0].into(),
                "legacy-collision-resource-a".into(),
                "legacy-collision-message-a".into(),
                "legacy-collision-run-a".into(),
            ),
            (
                work_ids[1].into(),
                "legacy-collision-resource-b".into(),
                "legacy-collision-message-b".into(),
                "legacy-collision-run-b".into(),
            ),
        ]
    );
    let foreign_key_violations: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_foreign_key_check")
            .fetch_one(&mut connection)
            .await
            .unwrap();
    assert_eq!(foreign_key_violations, 0);
    let quick_check: String = sqlx::query_scalar("PRAGMA quick_check")
        .fetch_one(&mut connection)
        .await
        .unwrap();
    assert_eq!(quick_check, "ok");
}

#[tokio::test]
async fn assignment_migration_failure_rolls_back_legacy_resources_atomically() {
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
        (
            5,
            "agent domain",
            include_str!("../migrations/0005_agent_domain.sql"),
        ),
    ] {
        apply_agent_domain_migration(&mut connection, version, description, sql).await;
    }
    sqlx::query(
        "INSERT INTO works (id, title, goal, root_path, permission_mode, status, created_at, updated_at) \
         VALUES ('legacy-atomic-work', 'Legacy', 'Goal', '/workspace', 'balanced', 'draft', ?, ?)",
    )
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(&mut connection)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO runs (id, work_id, engine_kind, model_label, status, created_at, updated_at) \
         VALUES ('legacy-atomic-run', 'legacy-atomic-work', 'pi', 'model', 'completed', ?, ?)",
    )
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:01:00Z")
    .execute(&mut connection)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO messages (id, work_id, run_id, role, content, created_at) \
         VALUES ('legacy-atomic-message', 'legacy-atomic-work', 'legacy-atomic-run', \
             'assistant', 'done', ?)",
    )
    .bind("2026-01-01T00:01:00Z")
    .execute(&mut connection)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO resource_blobs (id, plaintext_sha256, size, created_at) \
         VALUES ('legacy-atomic-blob', ?, 4, ?)",
    )
    .bind("d".repeat(64))
    .bind("2026-01-01T00:00:00Z")
    .execute(&mut connection)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO managed_resources (id, space_id, blob_id, original_name, media_type, size, \
             origin, status, created_at, updated_at) \
         VALUES ('legacy-atomic-resource', 'local-personal', 'legacy-atomic-blob', \
             'legacy.txt', 'text/plain', 4, 'user_upload', 'ready', ?, ?)",
    )
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(&mut connection)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO resource_links (id, resource_id, work_id, draft_id, message_id, run_id, role, created_at) \
         VALUES ('legacy-atomic-link', 'legacy-atomic-resource', 'legacy-atomic-work', NULL, \
             'legacy-atomic-message', 'legacy-atomic-run', 'attached', ?)",
    )
    .bind("2026-01-01T00:01:00Z")
    .execute(&mut connection)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO events (id, work_id, run_id, sequence, version, occurred_at, payload, assignment_id) \
         VALUES ('legacy-atomic-event', 'legacy-atomic-work', 'legacy-atomic-run', 1, 1, \
             ?, '{}', 'legacy-atomic-assignment')",
    )
    .bind("2026-01-01T00:01:00Z")
    .execute(&mut connection)
    .await
    .unwrap();

    let migration_sql = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("migrations/0006_assignment_sessions.sql"),
    )
    .unwrap();
    let migration = Migration::new(
        6,
        Cow::Borrowed("assignment sessions"),
        MigrationType::Simple,
        Cow::Owned(migration_sql),
        false,
    );
    assert!(connection.apply(&migration).await.is_err());

    let assignment_table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name = 'assignments'",
    )
    .fetch_one(&mut connection)
    .await
    .unwrap();
    assert_eq!(assignment_table_count, 0);
    let temp_mapping_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_temp_schema \
         WHERE type = 'table' AND name = 'migration_0006_legacy_assignment_map'",
    )
    .fetch_one(&mut connection)
    .await
    .unwrap();
    assert_eq!(temp_mapping_count, 0);
    for (table, id) in [
        ("runs", "legacy-atomic-run"),
        ("messages", "legacy-atomic-message"),
        ("events", "legacy-atomic-event"),
        ("resource_links", "legacy-atomic-link"),
    ] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE id = ?"))
            .bind(id)
            .fetch_one(&mut connection)
            .await
            .unwrap();
        assert_eq!(count, 1, "legacy row lost from {table}");
    }
    let legacy_event: (String, String) =
        sqlx::query_as("SELECT run_id, assignment_id FROM events WHERE id = 'legacy-atomic-event'")
            .fetch_one(&mut connection)
            .await
            .unwrap();
    assert_eq!(
        legacy_event,
        (
            "legacy-atomic-run".into(),
            "legacy-atomic-assignment".into(),
        )
    );
    let legacy_link: (String, String, String) = sqlx::query_as(
        "SELECT resource_id, message_id, run_id FROM resource_links \
         WHERE id = 'legacy-atomic-link'",
    )
    .fetch_one(&mut connection)
    .await
    .unwrap();
    assert_eq!(
        legacy_link,
        (
            "legacy-atomic-resource".into(),
            "legacy-atomic-message".into(),
            "legacy-atomic-run".into(),
        )
    );
    let applied_v6: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 6")
            .fetch_one(&mut connection)
            .await
            .unwrap();
    assert_eq!(applied_v6, 0);
    let foreign_key_violations: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_foreign_key_check")
            .fetch_one(&mut connection)
            .await
            .unwrap();
    assert_eq!(foreign_key_violations, 0);
    let quick_check: String = sqlx::query_scalar("PRAGMA quick_check")
        .fetch_one(&mut connection)
        .await
        .unwrap();
    assert_eq!(quick_check, "ok");
}

#[tokio::test]
async fn collaboration_migration_creates_memory_and_result_tables() {
    let database = Database::open_in_memory().await.unwrap();
    insert_work(&database, "work-collab").await;

    for table in [
        "agent_memory",
        "work_memory",
        "assignment_results",
        "memory_candidates",
    ] {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?",
        )
        .bind(table)
        .fetch_one(database.pool())
        .await
        .unwrap();
        assert_eq!(count, 1, "missing collaboration table {table}");
    }

    // work_memory is one projection row per (work, revision).
    sqlx::query(
        "INSERT INTO work_memory (id, work_id, revision, ledger_json, source_sequence, created_at, updated_at) \
         VALUES ('wm-1', 'work-collab', 1, '{}', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .execute(database.pool())
    .await
    .unwrap();
    let duplicate = sqlx::query(
        "INSERT INTO work_memory (id, work_id, revision, ledger_json, source_sequence, created_at, updated_at) \
         VALUES ('wm-2', 'work-collab', 1, '{}', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .execute(database.pool())
    .await;
    assert!(
        duplicate.is_err(),
        "work_memory must be unique per (work, revision)"
    );

    // memory_candidates rejects unknown statuses and a confirmed candidate
    // without a resolver.
    let bad_status = sqlx::query(
        "INSERT INTO memory_candidates (id, source_work_id, author_agent_id, content, reason, version, status, created_at) \
         VALUES ('mc-1', 'work-collab', 'agent-1', 'content', 'reason', 1, 'bogus', '2026-01-01T00:00:00Z')",
    )
    .execute(database.pool())
    .await;
    assert!(
        bad_status.is_err(),
        "memory candidate status must be constrained"
    );

    let bad_resolve = sqlx::query(
        "INSERT INTO memory_candidates (id, source_work_id, author_agent_id, content, reason, version, status, created_at) \
         VALUES ('mc-2', 'work-collab', 'agent-1', 'content', 'reason', 1, 'confirmed', '2026-01-01T00:00:00Z')",
    )
    .execute(database.pool())
    .await;
    assert!(
        bad_resolve.is_err(),
        "confirmed candidates require resolved_at and resolved_by"
    );
}
