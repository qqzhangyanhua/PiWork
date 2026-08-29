use std::path::Path;

use piwork_lib::storage::sqlite::Database;
use sqlx::{
    Connection, Row, SqliteConnection,
    migrate::Migrate,
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

#[tokio::test]
async fn foundation_era_database_upgrades_to_current_schema_without_losing_core_facts() {
    let (_directory, database) = open_upgraded_historical_database(
        1,
        include_str!("fixtures/historical/v0001_foundation.sql"),
    )
    .await;

    assert_current_schema_applied(&database).await;
    assert_foreign_keys_and_quick_check(database.pool()).await;
    assert_eq!(table_count(database.pool(), "works").await, 3);
    assert_eq!(table_count(database.pool(), "runs").await, 2);
    assert_eq!(table_count(database.pool(), "events").await, 1);
    assert_eq!(table_count(database.pool(), "messages").await, 1);
    assert_eq!(table_count(database.pool(), "work_leads").await, 3);

    let theme: String = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'theme'")
        .fetch_one(database.pool())
        .await
        .unwrap();
    assert_eq!(theme, r#"{"mode":"system"}"#);

    let event: (String, String, String) = sqlx::query_as(
        "SELECT events.work_id, events.run_id, runs.status \
         FROM events INNER JOIN runs ON runs.id = events.run_id \
         WHERE events.id = 'event-alpha'",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(
        event,
        (
            "work-alpha-slash".into(),
            "run-alpha".into(),
            "completed".into()
        )
    );

    let lead: String = sqlx::query_scalar(
        "SELECT agent_instance_id FROM work_leads WHERE work_id = 'work-alpha-slash'",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(lead, "agent-instance:piwork-lead");

    assert_workspace_backfill_is_unique(database.pool()).await;
}

#[tokio::test]
async fn pre_workspace_database_upgrades_without_duplicating_identity_or_breaking_traceability() {
    let (_directory, database) = open_upgraded_historical_database(
        17,
        include_str!("fixtures/historical/v0017_pre_workspace.sql"),
    )
    .await;

    assert_current_schema_applied(&database).await;
    assert_foreign_keys_and_quick_check(database.pool()).await;
    assert_eq!(table_count(database.pool(), "works").await, 3);
    assert_eq!(table_count(database.pool(), "work_agents").await, 4);
    assert_eq!(table_count(database.pool(), "work_leads").await, 3);
    assert_eq!(table_count(database.pool(), "assignments").await, 2);
    assert_eq!(table_count(database.pool(), "runs").await, 1);
    assert_eq!(table_count(database.pool(), "events").await, 1);
    assert_eq!(table_count(database.pool(), "assignment_results").await, 1);
    assert_eq!(table_count(database.pool(), "work_deliveries").await, 1);
    assert_eq!(
        table_count(database.pool(), "delivery_validations").await,
        1
    );
    assert_eq!(
        table_count(database.pool(), "run_capability_snapshots").await,
        1
    );
    assert_eq!(table_count(database.pool(), "managed_resources").await, 1);
    assert_eq!(table_count(database.pool(), "resource_links").await, 1);
    assert_eq!(
        table_count(database.pool(), "workspace_memory_bindings").await,
        2
    );
    assert_eq!(
        table_count(database.pool(), "extension_workspace_policies").await,
        1
    );

    let trace: (String, String, String, String, String, String, String) = sqlx::query_as(
        "SELECT events.run_id, events.assignment_id, runs.work_id, assignments.kind, \
                assignment_results.status, work_deliveries.status, \
                delivery_validations.verification_status \
         FROM events \
         INNER JOIN runs ON runs.id = events.run_id AND runs.work_id = events.work_id \
         INNER JOIN assignments \
            ON assignments.id = events.assignment_id AND assignments.work_id = events.work_id \
         INNER JOIN assignment_results ON assignment_results.assignment_id = assignments.id \
         INNER JOIN work_deliveries ON work_deliveries.lead_assignment_id = assignments.id \
         INNER JOIN delivery_validations \
            ON delivery_validations.delivery_id = work_deliveries.id \
           AND delivery_validations.source_event_id = events.id \
         WHERE events.id = 'event-alpha'",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(
        trace,
        (
            "run-alpha".into(),
            "assignment-lead-alpha".into(),
            "work-alpha-slash".into(),
            "lead".into(),
            "valid".into(),
            "valid".into(),
            "verified".into()
        )
    );

    let snapshot_run: String = sqlx::query_scalar(
        "SELECT run_id FROM run_capability_snapshots WHERE id = 'snapshot-alpha'",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(snapshot_run, "run-alpha");

    let linked_work: String =
        sqlx::query_scalar("SELECT work_id FROM resource_links WHERE id = 'link-alpha'")
            .fetch_one(database.pool())
            .await
            .unwrap();
    assert_eq!(linked_work, "work-alpha-slash");

    let member_parent: String = sqlx::query_scalar(
        "SELECT parent_assignment_id FROM assignments WHERE id = 'assignment-member-alpha'",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(member_parent, "assignment-lead-alpha");

    let agents: Vec<(String, String)> = sqlx::query_as(
        "SELECT agent_instance_id, role_kind FROM work_agents \
         WHERE work_id = 'work-alpha-slash' ORDER BY role_kind",
    )
    .fetch_all(database.pool())
    .await
    .unwrap();
    assert_eq!(
        agents,
        vec![
            ("agent-instance:piwork-engineer".into(), "engineer".into()),
            ("agent-instance:piwork-lead".into(), "lead".into()),
        ]
    );
    let lead: String = sqlx::query_scalar(
        "SELECT agent_instance_id FROM work_leads WHERE work_id = 'work-alpha-slash'",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(lead, "agent-instance:piwork-lead");

    assert_workspace_backfill_is_unique(database.pool()).await;
}

async fn open_upgraded_historical_database(
    up_to_version: i64,
    seed_sql: &str,
) -> (tempfile::TempDir, Database) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("piwork.sqlite3");
    write_historical_database(&path, up_to_version, seed_sql).await;
    let database = Database::open(&path).await.unwrap_or_else(|error| {
        panic!(
            "production migrator failed to upgrade historical schema v{up_to_version}: {error:?}"
        )
    });
    (directory, database)
}

async fn write_historical_database(path: &Path, up_to_version: i64, seed_sql: &str) {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal);
    let mut connection = SqliteConnection::connect_with(&options).await.unwrap();
    connection.ensure_migrations_table().await.unwrap();
    for migration in MIGRATOR.iter() {
        if migration.version > up_to_version {
            break;
        }
        connection.apply(migration).await.unwrap_or_else(|error| {
            panic!(
                "failed to apply historical migration {}: {error:?}",
                migration.version
            )
        });
    }

    let applied: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&mut connection)
            .await
            .unwrap();
    let expected: Vec<i64> = MIGRATOR
        .iter()
        .map(|migration| migration.version)
        .filter(|version| *version <= up_to_version)
        .collect();
    assert_eq!(
        applied, expected,
        "historical fixture must stop at schema v{up_to_version}"
    );

    execute_sql_script(&mut connection, seed_sql).await;
    connection.close().await.unwrap();
}

async fn execute_sql_script(connection: &mut SqliteConnection, script: &str) {
    for statement in script.split(';') {
        let statement = statement.trim();
        if statement.is_empty() {
            continue;
        }
        sqlx::query(statement)
            .execute(&mut *connection)
            .await
            .unwrap_or_else(|error| {
                panic!("failed to execute historical fixture statement:\n{statement}\n{error:?}")
            });
    }
}

async fn assert_current_schema_applied(database: &Database) {
    let applied: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(database.pool())
            .await
            .unwrap();
    let expected: Vec<i64> = MIGRATOR.iter().map(|migration| migration.version).collect();
    assert_eq!(applied, expected);

    let names = database.table_names().await.unwrap();
    for table in [
        "works",
        "runs",
        "events",
        "assignments",
        "workspaces",
        "work_deliveries",
        "run_capability_snapshots",
        "connector_agent_grants",
    ] {
        assert!(names.contains(&table.to_owned()), "missing {table}");
    }
}

async fn assert_foreign_keys_and_quick_check(pool: &sqlx::SqlitePool) {
    let foreign_key_violations: Vec<String> = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| {
            format!(
                "{} {} {} {}",
                row.get::<String, _>(0),
                row.get::<i64, _>(1),
                row.get::<String, _>(2),
                row.get::<i64, _>(3)
            )
        })
        .collect();
    assert!(
        foreign_key_violations.is_empty(),
        "foreign key violations after upgrade: {foreign_key_violations:?}"
    );

    let quick_check: String = sqlx::query_scalar("PRAGMA quick_check")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(quick_check, "ok");
}

async fn assert_workspace_backfill_is_unique(pool: &sqlx::SqlitePool) {
    let missing: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM works \
         LEFT JOIN workspaces ON workspaces.id = works.workspace_id \
         WHERE works.workspace_id IS NULL OR workspaces.id IS NULL",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(missing, 0);

    let duplicates: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM (
             SELECT path_identity
             FROM workspaces
             WHERE lifecycle_status = 'active'
             GROUP BY path_identity
             HAVING COUNT(*) > 1
         ) AS duplicated",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(duplicates, 0);

    let alpha: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT works.id, works.workspace_id, workspaces.lifecycle_status \
         FROM works \
         INNER JOIN workspaces ON workspaces.id = works.workspace_id \
         WHERE works.id IN ('work-alpha-slash', 'work-alpha-backslash') \
         ORDER BY works.id",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        alpha.iter().map(|row| row.0.as_str()).collect::<Vec<_>>(),
        vec!["work-alpha-backslash", "work-alpha-slash"]
    );
    assert_eq!(alpha[0].1, alpha[1].1);
    assert!(alpha.iter().all(|row| row.2 == "active"));

    let beta: String = sqlx::query_scalar("SELECT workspace_id FROM works WHERE id = 'work-beta'")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_ne!(beta, alpha[0].1);

    let active_for_alpha: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM workspaces \
         WHERE lifecycle_status = 'active' AND id = ?",
    )
    .bind(&alpha[0].1)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(active_for_alpha, 1);
}

async fn table_count(pool: &sqlx::SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}
