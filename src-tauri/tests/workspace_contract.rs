use chrono::Utc;
use piwork_lib::{
    domain::work::{CreateWorkInput, PermissionMode},
    storage::sqlite::Database,
    work::repository::WorkRepository,
};

#[tokio::test]
async fn equivalent_roots_share_workspace_identity_and_legacy_writes_are_repaired() {
    let root = tempfile::tempdir().unwrap();
    let database = Database::open_in_memory().await.unwrap();
    let repository = WorkRepository::new(database.pool().clone());
    let first = repository
        .create(CreateWorkInput {
            title: "First".into(),
            goal: "Share a Workspace".into(),
            root_path: root.path().to_string_lossy().into_owned(),
            permission_mode: PermissionMode::Balanced,
            resource_draft_id: None,
        })
        .await
        .unwrap();
    let second = repository
        .create(CreateWorkInput {
            title: "Second".into(),
            goal: "Reuse path identity".into(),
            root_path: root.path().join(".").to_string_lossy().into_owned(),
            permission_mode: PermissionMode::AutoExecute,
            resource_draft_id: None,
        })
        .await
        .unwrap();
    assert_eq!(first.summary.workspace_id, second.summary.workspace_id);
    assert_eq!(first.summary.root_path, second.summary.root_path);

    let now = Utc::now();
    sqlx::query("INSERT INTO works (id, title, goal, root_path, permission_mode, status, created_at, updated_at) VALUES ('legacy-workspace-writer', 'Legacy', 'Compatibility', ?, 'balanced', 'draft', ?, ?)")
        .bind(root.path().to_string_lossy().as_ref())
        .bind(now)
        .bind(now)
        .execute(database.pool())
        .await
        .unwrap();
    let legacy_workspace: Option<String> =
        sqlx::query_scalar("SELECT workspace_id FROM works WHERE id = 'legacy-workspace-writer'")
            .fetch_one(database.pool())
            .await
            .unwrap();
    assert_eq!(
        legacy_workspace.as_deref(),
        Some(first.summary.workspace_id.as_str())
    );

    let missing: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM works LEFT JOIN workspaces ON workspaces.id = works.workspace_id WHERE works.workspace_id IS NULL OR workspaces.id IS NULL",
    )
    .fetch_one(database.pool())
    .await
    .unwrap();
    assert_eq!(missing, 0);
}
