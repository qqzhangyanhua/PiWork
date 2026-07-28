use piwork_lib::storage::sqlite::Database;
use sqlx::sqlite::SqliteQueryResult;

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
         (id, work_id, model_label, status, created_at, updated_at) \
         VALUES (?, ?, 'test-model', 'queued', ?, ?)",
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
         (id, work_id, model_label, status, created_at, updated_at) \
         VALUES ('invalid-run', 'work-1', 'test-model', 'invalid', ?, ?)",
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
