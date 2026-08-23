use std::collections::BTreeSet;

use piwork_lib::{
    agent::{
        assembly::{MAX_ASSEMBLY_INSTRUCTION_CHARS, assembly_context_chars, validate_assembly},
        repository::AgentRepository,
        service::AgentService,
    },
    domain::agent::{
        AgentStatus, AssemblyDiagnosticCode, CapabilityPackStatus, PermissionPolicy, RoleKind,
        SaveAgentAssemblyInput,
    },
    storage::sqlite::Database,
};
use serde_json::json;

#[tokio::test]
async fn repository_lists_builtin_instances_and_capabilities() {
    let database = Database::open_in_memory().await.unwrap();
    let repository = AgentRepository::new(database.pool().clone());

    let roles = repository.list_role_templates().await.unwrap();
    assert_eq!(roles.len(), 4);
    assert!(roles.iter().all(|role| role.builtin));

    let instances = repository.list_agent_instances().await.unwrap();
    assert_eq!(
        instances
            .iter()
            .map(|instance| instance.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "agent-instance:piwork-engineer",
            "agent-instance:piwork-lead",
            "agent-instance:piwork-researcher",
            "agent-instance:piwork-reviewer",
        ]
    );
    let expected_instances = [
        (
            "agent-instance:piwork-engineer",
            "agent-definition:piwork-engineer:v1",
            "role-template:engineer:v1",
            RoleKind::Engineer,
            "capability-pack:engineering-execution:v1",
        ),
        (
            "agent-instance:piwork-lead",
            "agent-definition:piwork-lead:v1",
            "role-template:lead:v1",
            RoleKind::Lead,
            "capability-pack:lead-coordination:v1",
        ),
        (
            "agent-instance:piwork-researcher",
            "agent-definition:piwork-researcher:v1",
            "role-template:researcher:v1",
            RoleKind::Researcher,
            "capability-pack:source-research:v1",
        ),
        (
            "agent-instance:piwork-reviewer",
            "agent-definition:piwork-reviewer:v1",
            "role-template:reviewer:v1",
            RoleKind::Reviewer,
            "capability-pack:independent-review:v1",
        ),
    ];
    for (instance, expected) in instances.iter().zip(expected_instances) {
        let (instance_id, definition_id, role_template_id, role_kind, pack_id) = expected;
        assert_eq!(instance.id, instance_id);
        assert_eq!(instance.definition.id, definition_id);
        assert_eq!(instance.definition.role_template_id, role_template_id);
        assert_eq!(instance.definition.role_kind, role_kind);
        assert_eq!(
            instance
                .definition
                .capability_packs
                .iter()
                .map(|pack| pack.id.as_str())
                .collect::<Vec<_>>(),
            vec![pack_id]
        );
        assert!(instance.builtin);
        assert_eq!(instance.status, AgentStatus::Active);
        assert_eq!(instance.engine_override, None);
        assert_eq!(instance.model_configuration_override, None);
        assert_eq!(instance.permission_policy_override, None);
        assert_eq!(instance.parallelism_override, None);
    }
    assert_eq!(
        repository
            .get_agent_instance("agent-instance:piwork-lead")
            .await
            .unwrap()
            .unwrap(),
        instances[1]
    );
    assert!(
        repository
            .get_agent_instance("agent-instance:missing")
            .await
            .unwrap()
            .is_none()
    );

    let packs = repository.list_capability_packs().await.unwrap();
    let executable_ids = packs
        .iter()
        .filter(|pack| pack.status == CapabilityPackStatus::Executable)
        .map(|pack| pack.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        executable_ids,
        vec![
            "capability-pack:engineering-execution:v1",
            "capability-pack:independent-review:v1",
            "capability-pack:lead-coordination:v1",
            "capability-pack:source-research:v1",
        ]
    );

    assert!(
        packs
            .iter()
            .all(|pack| pack.status != CapabilityPackStatus::CatalogOnly),
        "migration 0010 removes presentation-only catalog packs"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_instance_read_keeps_definition_and_packs_in_one_snapshot() {
    let temporary_directory = tempfile::tempdir().unwrap();
    let database_path = temporary_directory.path().join("agent-snapshot.sqlite3");
    let database = Database::open(&database_path).await.unwrap();
    let repository = AgentRepository::new(database.pool().clone());
    let before = repository
        .get_agent_instance("agent-instance:piwork-lead")
        .await
        .unwrap()
        .unwrap();
    let before_definition_name = before.definition.name;
    let before_pack_name = before.definition.capability_packs[0].name.clone();
    let writer_pool = database.pool().clone();
    let (start_writer, writer_started) = tokio::sync::oneshot::channel();
    let (writer_finished, wait_for_writer) = std::sync::mpsc::sync_channel(0);
    let writer = tokio::spawn(async move {
        writer_started.await.unwrap();
        let mut transaction = writer_pool.begin().await.unwrap();
        sqlx::query("UPDATE agent_definitions SET name = 'snapshot definition after' WHERE id = ?")
            .bind("agent-definition:piwork-lead:v1")
            .execute(&mut *transaction)
            .await
            .unwrap();
        sqlx::query("UPDATE capability_packs SET name = 'snapshot pack after' WHERE id = ?")
            .bind("capability-pack:lead-coordination:v1")
            .execute(&mut *transaction)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        writer_finished.send(()).unwrap();
    });

    let during = repository
        .list_agent_instances_after_instances_loaded(move || {
            start_writer.send(()).unwrap();
            wait_for_writer
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("concurrent Agent update did not finish");
        })
        .await
        .unwrap()
        .into_iter()
        .find(|instance| instance.id == "agent-instance:piwork-lead")
        .unwrap();
    writer.await.unwrap();
    let after = repository
        .get_agent_instance("agent-instance:piwork-lead")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(during.definition.name, before_definition_name);
    assert_eq!(during.definition.capability_packs[0].name, before_pack_name);
    assert_eq!(after.definition.name, "snapshot definition after");
    assert_eq!(
        after.definition.capability_packs[0].name,
        "snapshot pack after"
    );
}

#[tokio::test]
async fn getting_one_agent_instance_ignores_malformed_unrelated_packs() {
    let database = Database::open_in_memory().await.unwrap();
    let repository = AgentRepository::new(database.pool().clone());
    sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(database.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE capability_packs SET required_tools_json = 'not-json' WHERE id = ?")
        .bind("capability-pack:independent-review:v1")
        .execute(database.pool())
        .await
        .unwrap();

    let lead = repository
        .get_agent_instance("agent-instance:piwork-lead")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(lead.definition.id, "agent-definition:piwork-lead:v1");
    assert_eq!(
        lead.definition.capability_packs[0].id,
        "capability-pack:lead-coordination:v1"
    );
}

async fn builtin_fixture(
    instance_id: &str,
) -> (
    Database,
    AgentRepository,
    piwork_lib::domain::agent::RoleTemplateSummary,
    piwork_lib::domain::agent::AgentDefinitionSummary,
) {
    let database = Database::open_in_memory().await.unwrap();
    let repository = AgentRepository::new(database.pool().clone());
    let source = repository
        .get_agent_instance(instance_id)
        .await
        .unwrap()
        .unwrap()
        .definition;
    let role = repository
        .list_role_templates()
        .await
        .unwrap()
        .into_iter()
        .find(|role| role.id == source.role_template_id)
        .unwrap();
    (database, repository, role, source)
}

fn codes(
    result: Result<
        piwork_lib::agent::assembly::ResolvedAgentAssembly,
        Vec<piwork_lib::domain::agent::AssemblyDiagnostic>,
    >,
) -> Vec<AssemblyDiagnosticCode> {
    result
        .unwrap_err()
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect()
}

#[tokio::test]
async fn assembly_rejects_catalog_only_pack() {
    let (_database, repository, role, source) = builtin_fixture("agent-instance:piwork-lead").await;
    let mut catalog_pack = repository
        .list_capability_packs()
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    catalog_pack.catalog_capability_id = Some("catalog-capability:test".into());
    catalog_pack.status = CapabilityPackStatus::CatalogOnly;

    let diagnostic_codes = codes(validate_assembly(
        &role,
        &source,
        &[catalog_pack],
        &BTreeSet::new(),
        &BTreeSet::new(),
        PermissionPolicy::ReadOnly,
    ));
    assert_eq!(diagnostic_codes[0], AssemblyDiagnosticCode::NotExecutable);
    assert!(diagnostic_codes.contains(&AssemblyDiagnosticCode::NotExecutable));
}

#[tokio::test]
async fn assembly_rejects_incompatible_role() {
    let (_database, repository, role, source) = builtin_fixture("agent-instance:piwork-lead").await;
    let pack = repository
        .get_agent_instance("agent-instance:piwork-reviewer")
        .await
        .unwrap()
        .unwrap()
        .definition
        .capability_packs
        .into_iter()
        .next()
        .unwrap();

    assert_eq!(
        codes(validate_assembly(
            &role,
            &source,
            &[pack],
            &["read", "grep", "find", "ls"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            &BTreeSet::new(),
            PermissionPolicy::ReadOnly,
        )),
        vec![AssemblyDiagnosticCode::IncompatibleRole]
    );
}

#[tokio::test]
async fn assembly_rejects_missing_tool_and_engine_capability() {
    let (_database, _repository, role, source) =
        builtin_fixture("agent-instance:piwork-lead").await;
    let mut pack = source.capability_packs[0].clone();
    pack.required_tools = vec!["missing-tool".into()];
    pack.required_engine_capabilities = vec!["missing-capability".into()];

    assert_eq!(
        codes(validate_assembly(
            &role,
            &source,
            &[pack],
            &BTreeSet::new(),
            &BTreeSet::new(),
            PermissionPolicy::InheritWork,
        )),
        vec![
            AssemblyDiagnosticCode::MissingTool,
            AssemblyDiagnosticCode::MissingEngineCapability,
        ]
    );
}

#[tokio::test]
async fn assembly_rejects_permission_escalation_and_explicit_conflicts() {
    let (_database, _repository, role, source) =
        builtin_fixture("agent-instance:piwork-lead").await;
    let mut first = source.capability_packs[0].clone();
    first.default_permission_scope = PermissionPolicy::WorkWrite;
    first.conflicts_with_capability_pack_ids = vec!["pack:second".into()];
    let mut second = first.clone();
    second.id = "pack:second".into();
    second.conflicts_with_capability_pack_ids.clear();

    assert_eq!(
        codes(validate_assembly(
            &role,
            &source,
            &[first, second],
            &["read", "grep", "find", "ls"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            &BTreeSet::new(),
            PermissionPolicy::ReadOnly,
        )),
        vec![
            AssemblyDiagnosticCode::PermissionEscalation,
            AssemblyDiagnosticCode::PermissionEscalation,
            AssemblyDiagnosticCode::CapabilityConflict,
        ]
    );
}

#[tokio::test]
async fn assembly_rejects_instruction_budget_over_approved_limit() {
    let (_database, _repository, role, mut source) =
        builtin_fixture("agent-instance:piwork-lead").await;
    source.instructions = "x".repeat(MAX_ASSEMBLY_INSTRUCTION_CHARS + 1);

    assert_eq!(
        codes(validate_assembly(
            &role,
            &source,
            &[],
            &BTreeSet::new(),
            &BTreeSet::new(),
            PermissionPolicy::ReadOnly,
        )),
        vec![AssemblyDiagnosticCode::ContextBudgetExceeded]
    );
}

#[tokio::test]
async fn valid_assembly_resolves_least_privilege_and_parallelism() {
    let (_database, _repository, role, mut source) =
        builtin_fixture("agent-instance:piwork-reviewer").await;
    source.default_permission_policy = PermissionPolicy::WorkWrite;
    source.default_parallelism = 8;
    let pack = source.capability_packs[0].clone();
    let resolved = validate_assembly(
        &role,
        &source,
        &[pack],
        &["read", "grep", "find", "ls"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        &BTreeSet::new(),
        PermissionPolicy::ReadOnly,
    )
    .unwrap();

    assert_eq!(resolved.permission_policy(), PermissionPolicy::ReadOnly);
    assert_eq!(resolved.parallelism(), 1);
}

#[tokio::test]
async fn service_copy_is_local_read_only_and_does_not_modify_builtin_source() {
    let (database, repository, _role, _source) =
        builtin_fixture("agent-instance:piwork-reviewer").await;
    let before = repository
        .get_agent_instance("agent-instance:piwork-reviewer")
        .await
        .unwrap()
        .unwrap();
    let service = AgentService::new(
        repository.clone(),
        ["read", "grep", "find", "ls"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        BTreeSet::new(),
    );
    let copied = service
        .save_agent_copy(SaveAgentAssemblyInput {
            source_instance_id: before.id.clone(),
            display_name: "本地审阅者".into(),
            capability_pack_ids: vec!["capability-pack:independent-review:v1".into()],
            engine_override: Some("local-engine".into()),
            model_configuration_override: Some("model:test".into()),
            permission_policy_override: Some(PermissionPolicy::ReadOnly),
            parallelism_override: Some(4),
        })
        .await
        .unwrap();

    assert!(!copied.builtin);
    assert!(!copied.definition.builtin);
    assert_eq!(copied.definition.version, 1);
    assert_eq!(copied.display_name, "本地审阅者");
    assert_eq!(
        copied.permission_policy_override,
        Some(PermissionPolicy::ReadOnly)
    );
    assert_eq!(copied.parallelism_override, Some(4));
    assert_eq!(
        repository
            .get_agent_instance(&before.id)
            .await
            .unwrap()
            .unwrap(),
        before
    );
    let local_definitions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_definitions WHERE builtin = 0 AND version = 1",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(local_definitions, 1);
}

#[tokio::test]
async fn service_rejects_removed_catalog_pack_without_persisting_rows() {
    let (database, repository, _role, _source) =
        builtin_fixture("agent-instance:piwork-lead").await;
    let service = AgentService::new(repository, BTreeSet::new(), BTreeSet::new());
    let input = SaveAgentAssemblyInput {
        source_instance_id: "agent-instance:piwork-lead".into(),
        display_name: "copy".into(),
        capability_pack_ids: vec!["catalog-capability:001".into()],
        engine_override: None,
        model_configuration_override: None,
        permission_policy_override: Some(PermissionPolicy::ReadOnly),
        parallelism_override: None,
    };

    assert!(
        service
            .validate_agent_assembly(input.clone())
            .await
            .is_err()
    );
    assert!(service.save_agent_copy(input).await.is_err());
    let local_instances: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM agent_instances WHERE builtin = 0")
            .fetch_one(database.pool())
            .await
            .unwrap();
    assert_eq!(local_instances, 0);
}

#[tokio::test]
async fn service_rejects_permission_override_that_would_expand_builtin_authority() {
    let (_database, repository, _role, _source) =
        builtin_fixture("agent-instance:piwork-reviewer").await;
    let service = AgentService::new(
        repository,
        ["read", "grep", "find", "ls"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        BTreeSet::new(),
    );
    let diagnostics = service
        .validate_agent_assembly(SaveAgentAssemblyInput {
            source_instance_id: "agent-instance:piwork-reviewer".into(),
            display_name: "expanded reviewer".into(),
            capability_pack_ids: vec!["capability-pack:independent-review:v1".into()],
            engine_override: None,
            model_configuration_override: None,
            permission_policy_override: Some(PermissionPolicy::WorkWrite),
            parallelism_override: None,
        })
        .await
        .unwrap();

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == AssemblyDiagnosticCode::PermissionEscalation)
    );
}

#[tokio::test]
async fn service_intersects_definition_and_instance_permissions_before_copying() {
    let (database, repository, _role, _source) =
        builtin_fixture("agent-instance:piwork-reviewer").await;
    sqlx::query(
        "UPDATE agent_instances SET permission_policy_override = 'work_write' WHERE id = ?",
    )
    .bind("agent-instance:piwork-reviewer")
    .execute(database.pool())
    .await
    .unwrap();
    let service = AgentService::new(
        repository,
        ["read", "grep", "find", "ls"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        BTreeSet::new(),
    );

    let copied = service
        .save_agent_copy(SaveAgentAssemblyInput {
            source_instance_id: "agent-instance:piwork-reviewer".into(),
            display_name: "bounded reviewer".into(),
            capability_pack_ids: vec!["capability-pack:independent-review:v1".into()],
            engine_override: None,
            model_configuration_override: None,
            permission_policy_override: None,
            parallelism_override: None,
        })
        .await
        .unwrap();

    assert_eq!(
        copied.definition.default_permission_policy,
        PermissionPolicy::ReadOnly
    );
    assert_ne!(
        copied.permission_policy_override,
        Some(PermissionPolicy::WorkWrite)
    );
}

#[tokio::test]
async fn save_rejects_pack_drift_after_validation_without_writing_a_copy() {
    let (database, repository, _role, _source) =
        builtin_fixture("agent-instance:piwork-reviewer").await;
    let service = AgentService::new(
        repository,
        ["read", "grep", "find", "ls"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        BTreeSet::new(),
    );
    let pool = database.pool().clone();
    let result = service
        .save_agent_copy_after_validation(
            SaveAgentAssemblyInput {
                source_instance_id: "agent-instance:piwork-reviewer".into(),
                display_name: "stale reviewer".into(),
                capability_pack_ids: vec!["capability-pack:independent-review:v1".into()],
                engine_override: None,
                model_configuration_override: None,
                permission_policy_override: Some(PermissionPolicy::ReadOnly),
                parallelism_override: None,
            },
            || async move {
                sqlx::query(
                    "UPDATE capability_packs SET version = version + 1, status = 'deprecated', instructions = 'changed after validation' WHERE id = ?",
                )
                .bind("capability-pack:independent-review:v1")
                .execute(&pool)
                .await
                .unwrap();
            },
        )
        .await;

    assert!(result.is_err());
    let local_rows: i64 = sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM agent_definitions WHERE builtin = 0) + \
                (SELECT COUNT(*) FROM agent_instances WHERE builtin = 0)",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(local_rows, 0);
}

#[tokio::test]
async fn assembly_budget_counts_non_instruction_contract_context_and_allows_exact_limit() {
    let (_database, _repository, mut role, mut source) =
        builtin_fixture("agent-instance:piwork-reviewer").await;
    role.base_instructions.clear();
    role.responsibilities.clear();
    role.non_responsibilities.clear();
    role.base_result_contract = json!({});
    source.instructions.clear();
    source.responsibilities.clear();
    source.non_responsibilities.clear();
    source.input_contract = json!({});
    source.result_contract = json!({});
    source.quality_rubric = json!({});
    let mut pack = source.capability_packs[0].clone();
    pack.instructions.clear();
    pack.input_schema = json!({});
    pack.output_schema = json!({});
    pack.procedure = json!({});
    pack.validation_rubric = json!({});

    let base = assembly_context_chars(&role, &source, &[pack.clone()]);
    role.responsibilities = vec!["x".repeat(MAX_ASSEMBLY_INSTRUCTION_CHARS - base - 2)];
    assert_eq!(
        assembly_context_chars(&role, &source, &[pack.clone()]),
        MAX_ASSEMBLY_INSTRUCTION_CHARS
    );
    assert!(
        validate_assembly(
            &role,
            &source,
            &[pack.clone()],
            &["read", "grep", "find", "ls"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            &BTreeSet::new(),
            PermissionPolicy::ReadOnly,
        )
        .is_ok()
    );

    role.responsibilities[0].push('x');
    let diagnostics = validate_assembly(
        &role,
        &source,
        &[pack],
        &["read", "grep", "find", "ls"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        &BTreeSet::new(),
        PermissionPolicy::ReadOnly,
    )
    .unwrap_err();
    assert_eq!(
        diagnostics.last().unwrap().code,
        AssemblyDiagnosticCode::ContextBudgetExceeded
    );
}

#[tokio::test]
async fn repository_maps_malformed_json_and_out_of_range_versions_to_database_errors() {
    let database = Database::open_in_memory().await.unwrap();
    let repository = AgentRepository::new(database.pool().clone());
    sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(database.pool())
        .await
        .unwrap();
    sqlx::query(
        "UPDATE capability_packs SET required_tools_json = 'not-json' \
         WHERE id = 'capability-pack:lead-coordination:v1'",
    )
    .execute(database.pool())
    .await
    .unwrap();

    assert!(matches!(
        repository.list_capability_packs().await,
        Err(piwork_lib::error::AppError::Database(sqlx::Error::Decode(
            _
        )))
    ));

    sqlx::query(
        "UPDATE capability_packs SET required_tools_json = '[]', version = 4294967296 \
         WHERE id = 'capability-pack:lead-coordination:v1'",
    )
    .execute(database.pool())
    .await
    .unwrap();
    assert!(matches!(
        repository.list_capability_packs().await,
        Err(piwork_lib::error::AppError::Database(sqlx::Error::Decode(
            _
        )))
    ));
}
