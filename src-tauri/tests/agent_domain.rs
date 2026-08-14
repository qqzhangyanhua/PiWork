use piwork_lib::{
    agent::repository::AgentRepository,
    domain::agent::{AgentStatus, CapabilityPackStatus, RoleKind},
    storage::sqlite::Database,
};

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

    let catalog = packs
        .iter()
        .filter(|pack| pack.catalog_capability_id.is_some())
        .collect::<Vec<_>>();
    assert_eq!(catalog.len(), 96);
    assert!(
        catalog
            .iter()
            .all(|pack| pack.status == CapabilityPackStatus::CatalogOnly)
    );
    assert_eq!(
        catalog
            .iter()
            .map(|pack| pack.catalog_capability_id.as_deref().unwrap())
            .collect::<Vec<_>>(),
        (1..=96)
            .map(|index| format!("catalog-capability:{index:03}"))
            .collect::<Vec<_>>()
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
