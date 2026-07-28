use piwork_lib::storage::sqlite::Database;

#[tokio::test]
async fn migration_creates_foundation_tables() {
    let database = Database::open_in_memory().await.unwrap();
    let names = database.table_names().await.unwrap();

    for expected in ["works", "runs", "messages", "events", "settings"] {
        assert!(names.contains(&expected.to_string()), "missing {expected}");
    }
}
