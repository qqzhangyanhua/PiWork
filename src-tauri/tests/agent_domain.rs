use piwork_lib::{
    agent::repository::AgentRepository, domain::agent::CapabilityPackStatus,
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
