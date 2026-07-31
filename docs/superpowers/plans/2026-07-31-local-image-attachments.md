# Local Image Attachments Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the first end-to-end managed-attachment slice: users can upload durable local images from either composer, reuse them within a Work, see them on messages and in the Work inspector, and send selected images through Pi's native RPC image field without injecting file bytes into prompt text.

**Architecture:** Add an engine-neutral resource module backed by immutable SHA-256-addressed local blobs and SQLite metadata. New-Work uploads are linked to a client-generated draft ID and adopted when the Work is created; existing-Work uploads are linked immediately. Run creation validates and records message/resource links transactionally, while the engine receives a typed `EngineInput` containing prompt text and bounded image bytes.

**Tech Stack:** Rust 2024, Tauri 2, SQLite/sqlx, `sha2`, `base64`, `image`, async traits, React 19, TypeScript, Zustand, Vitest/Testing Library, ts-rs.

**Scope boundary:** This plan accepts PNG, JPEG, GIF, and WebP images only. The approved PDF/DOCX/Excel/scanned-PDF runtime is the next implementation plan and will build on the resource and blob contracts established here. Personal encrypted S3 replicas and Team Spaces remain later plans.

`@` project files remain live, non-copied `WorkspaceReference` values. Explicit uploads become immutable `ManagedResource` values owned by a Space; their bytes live in `ResourceBlob` objects with a local `BlobReplica`. The two paths meet only in typed Run input and never share retention semantics.

---

## Locked file structure

### New backend files

- `src-tauri/migrations/0002_resources.sql` — Personal Space, resource, blob replica, and resource-link schema.
- `src-tauri/src/domain/resource.rs` — serialized resource DTOs and import input.
- `src-tauri/src/resource/mod.rs` — resource module exports.
- `src-tauri/src/resource/blob_store.rs` — engine-neutral blob store contract and storage reference types.
- `src-tauri/src/resource/local_blob_store.rs` — immutable SHA-256-addressed local storage.
- `src-tauri/src/resource/repository.rs` — resource metadata, draft/Work links, thumbnail lookup, and recovery queries.
- `src-tauri/src/resource/service.rs` — per-file import validation, limits, thumbnail generation, and engine-image materialization.
- `src-tauri/src/resource/commands.rs` — Tauri resource commands.
- `src-tauri/tests/resource_lifecycle.rs` — durable import, deduplication, draft adoption, message linking, and recovery integration tests.

### Modified backend files

- `src-tauri/Cargo.toml` — hashing, base64, and bounded image decoding dependencies.
- `src-tauri/src/domain/mod.rs` — export resource DTO bindings.
- `src-tauri/src/domain/work.rs` — resource IDs on create/start/message DTOs.
- `src-tauri/src/paths.rs` — durable resource root and local derivative-cache root.
- `src-tauri/src/error.rs` — product-safe resource errors and serialized codes.
- `src-tauri/src/work/repository.rs` — adopt a draft during Work creation and atomically attach ready Work resources during Run creation.
- `src-tauri/src/work/service.rs` — resolve uploaded images separately from `@` project-file prompt expansion.
- `src-tauri/src/engine/mod.rs` — typed text-plus-image `EngineInput`.
- `src-tauri/src/engine/fake.rs` — consume the typed input in tests.
- `src-tauri/src/engine/supervisor.rs` — carry resource IDs into authoritative Run creation and `EngineInput` into the adapter.
- `src-tauri/src/engine/pi/mod.rs` — encode native Pi RPC `images` entries.
- `src-tauri/src/app_state.rs` — expose `ResourceService` to commands.
- `src-tauri/src/lib.rs` — assemble storage/repository/service, run recovery, and register commands.
- `src-tauri/tests/storage_contract.rs` — verify the resource migration constraints.
- `src-tauri/tests/work_lifecycle.rs` — verify attachment-only starts and authoritative message links.
- `src-tauri/tests/pi_engine.rs` — verify Pi RPC image payload shape.

### New frontend files

- `src/app/attachmentPicker.ts` — native multi-image picker behind an injectable function.
- `src/features/workspace/AttachmentButton.tsx` — compact composer add button and import coordination.
- `src/features/workspace/AttachmentDraftList.tsx` — selected/failed attachment chips and lazy thumbnails.
- `src/features/workspace/AttachmentChips.tsx` — read-only message/inspector attachment rendering.

### Modified frontend files

- `src/app/tauriClient.ts` — typed resource commands and resource IDs on start.
- `src/test/mockTauriClient.ts` — deterministic in-memory resource behavior.
- `src/features/works/workStore.ts` — Work resource cache, import actions, and queued resource-ID copies.
- `src/features/works/workStore.test.ts` — transport, hydration, and queue tests.
- `src/features/workspace/WorkComposer.tsx` — existing-Work upload/select/remove/send behavior.
- `src/features/workspace/NewWorkStart.tsx` — stable draft ID and draft adoption.
- `src/features/workspace/WorkSurface.tsx` — attachment-picker dependency injection and resource props.
- `src/features/workspace/WorkSurface.test.tsx` — end-to-end composer behavior.
- `src/features/workspace/WorkTimeline.tsx` — message attachment chips.
- `src/features/workspace/WorkTimeline.test.tsx` — attachment rendering and scroll-follow regression coverage.
- `src/features/workspace/WorkInspector.tsx` — Work-level Attachments tab.
- `src/i18n/locales/en.json` and `src/i18n/locales/zh-CN.json` — attachment copy and safe errors.
- `src/styles/workspace.css` — compact chips, thumbnails, add button, and inspector grid.
- `src/bindings/*.ts` and `src/bindings/index.ts` — generated ts-rs bindings.

## Fixed contracts and limits

Use these values consistently across the implementation:

```rust
pub const LOCAL_PERSONAL_SPACE_ID: &str = "local-personal";
pub const LOCAL_BLOB_STORE_ID: &str = "local-default";
pub const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;
pub const MAX_IMAGE_DIMENSION: u32 = 16_384;
pub const MAX_DECODE_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_IMAGES_PER_RUN: usize = 8;
pub const MAX_RUN_IMAGE_BYTES: u64 = 24 * 1024 * 1024;
pub const THUMBNAIL_EDGE: u32 = 192;
pub const STAGING_GRACE_HOURS: i64 = 24;
```

The allowed signatures are PNG (`image/png`), JPEG (`image/jpeg`), GIF (`image/gif`), and WebP (`image/webp`). The source path is accepted only by `import_resources`, never returned in a DTO, never stored as resource identity, and never included in the Pi prompt.

### Task 1: Add the resource schema and shared DTOs

**Files:**
- Create: `src-tauri/migrations/0002_resources.sql`
- Create: `src-tauri/src/domain/resource.rs`
- Modify: `src-tauri/src/domain/mod.rs`
- Modify: `src-tauri/src/domain/work.rs`
- Modify: `src-tauri/tests/storage_contract.rs`
- Generate: `src/bindings/ResourceStatus.ts`
- Generate: `src/bindings/ResourceOrigin.ts`
- Generate: `src/bindings/ResourceSummary.ts`
- Generate: `src/bindings/ResourceThumbnail.ts`
- Generate: `src/bindings/ImportResourcesInput.ts`
- Generate: `src/bindings/StartWorkInput.ts`
- Generate: `src/bindings/CreateWorkInput.ts`
- Generate: `src/bindings/MessageSummary.ts`
- Modify: `src/bindings/index.ts`

- [ ] **Step 1: Write failing migration and DTO tests**

Append these assertions to `src-tauri/tests/storage_contract.rs`:

```rust
#[test]
fn resource_migration_uses_stable_lf_line_endings() {
    let migration = include_bytes!("../migrations/0002_resources.sql");
    assert!(!migration.contains(&b'\r'));
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
    ).fetch_one(database.pool()).await.unwrap();
    assert_eq!(work_space_column, 1);
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
async fn resource_links_cannot_cross_space_boundaries() {
    let database = Database::open_in_memory().await.unwrap();
    insert_work(&database, "team-work").await;
    sqlx::query("INSERT INTO spaces (id, kind, created_at) VALUES ('team-1', 'team', ?)")
        .bind("2026-07-31T00:00:00Z")
        .execute(database.pool()).await.unwrap();
    sqlx::query("UPDATE works SET space_id = 'team-1' WHERE id = 'team-work'")
        .execute(database.pool()).await.unwrap();
    sqlx::query(
        "INSERT INTO managed_resources \
         (id, space_id, original_name, media_type, size, origin, status, created_at, updated_at) \
         VALUES ('personal-resource', 'local-personal', 'x.png', 'image/png', 1, \
                 'user_upload', 'staging', ?, ?)",
    ).bind("2026-07-31T00:00:00Z").bind("2026-07-31T00:00:00Z")
        .execute(database.pool()).await.unwrap();
    let result = sqlx::query(
        "INSERT INTO resource_links \
         (id, resource_id, work_id, role, created_at) \
         VALUES ('cross-space', 'personal-resource', 'team-work', 'attached', ?)",
    ).bind("2026-07-31T00:00:00Z").execute(database.pool()).await;
    assert_database_error_contains(result, "resource link space mismatch");
}
```

Add this serialization test to the new `src-tauri/src/domain/resource.rs` test module:

```rust
#[test]
fn import_input_hides_source_paths_from_resource_output() {
    let input = ImportResourcesInput {
        source_paths: vec!["C:/secret/photo.png".into()],
        draft_id: Some("draft-1".into()),
        work_id: None,
    };
    assert_eq!(serde_json::to_value(input).unwrap()["draftId"], "draft-1");

    let summary = ResourceSummary {
        id: "resource-1".into(),
        original_name: "photo.png".into(),
        media_type: "image/png".into(),
        size: 42,
        origin: ResourceOrigin::UserUpload,
        status: ResourceStatus::Ready,
        failure_code: None,
        created_at: Utc.with_ymd_and_hms(2026, 7, 31, 0, 0, 0).unwrap(),
    };
    let value = serde_json::to_value(summary).unwrap();
    assert!(value.get("sourcePath").is_none());
    assert_eq!(value["originalName"], "photo.png");
}
```

- [ ] **Step 2: Run the tests and verify the schema/types are missing**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test storage_contract migration_creates_resource_tables_and_local_personal_space
cargo test --manifest-path src-tauri/Cargo.toml domain::resource::tests::import_input_hides_source_paths_from_resource_output
```

Expected: the first test fails because the resource tables do not exist; the second command fails because `domain::resource` and its DTOs do not exist.

- [ ] **Step 3: Add the migration**

Create `src-tauri/migrations/0002_resources.sql` with this schema:

```sql
CREATE TABLE spaces (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('personal', 'team')),
    created_at TEXT NOT NULL
);

INSERT INTO spaces (id, kind, created_at)
VALUES ('local-personal', 'personal', '2026-07-31T00:00:00Z');

ALTER TABLE works
ADD COLUMN space_id TEXT NOT NULL DEFAULT 'local-personal' REFERENCES spaces(id);

CREATE TABLE resource_blobs (
    id TEXT PRIMARY KEY NOT NULL,
    plaintext_sha256 TEXT NOT NULL CHECK (length(plaintext_sha256) = 64),
    size INTEGER NOT NULL CHECK (size >= 0),
    created_at TEXT NOT NULL,
    UNIQUE (plaintext_sha256, size)
);

CREATE TABLE blob_replicas (
    blob_id TEXT NOT NULL REFERENCES resource_blobs(id) ON DELETE CASCADE,
    store_id TEXT NOT NULL,
    object_key TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending', 'ready', 'failed', 'deleting')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (blob_id, store_id),
    UNIQUE (store_id, object_key)
);

CREATE TABLE managed_resources (
    id TEXT PRIMARY KEY NOT NULL,
    space_id TEXT NOT NULL REFERENCES spaces(id),
    blob_id TEXT REFERENCES resource_blobs(id),
    original_name TEXT NOT NULL,
    media_type TEXT NOT NULL,
    size INTEGER NOT NULL CHECK (size >= 0),
    origin TEXT NOT NULL CHECK (origin IN ('user_upload', 'generated_artifact')),
    status TEXT NOT NULL CHECK (
        status IN ('staging', 'processing', 'ready', 'failed', 'deleting')
    ),
    failure_code TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK ((status = 'ready' AND blob_id IS NOT NULL AND failure_code IS NULL)
        OR status <> 'ready')
);

CREATE INDEX idx_managed_resources_blob_id ON managed_resources(blob_id);
CREATE INDEX idx_managed_resources_status ON managed_resources(status, updated_at);
CREATE UNIQUE INDEX idx_messages_work_id_id ON messages(work_id, id);

CREATE TABLE resource_links (
    id TEXT PRIMARY KEY NOT NULL,
    resource_id TEXT NOT NULL REFERENCES managed_resources(id) ON DELETE CASCADE,
    work_id TEXT REFERENCES works(id) ON DELETE CASCADE,
    draft_id TEXT,
    message_id TEXT,
    run_id TEXT,
    role TEXT NOT NULL CHECK (role IN ('attached', 'pinned', 'memory_source')),
    created_at TEXT NOT NULL,
    CHECK ((work_id IS NOT NULL AND draft_id IS NULL)
        OR (work_id IS NULL AND draft_id IS NOT NULL)),
    CHECK (message_id IS NULL OR work_id IS NOT NULL),
    CHECK (run_id IS NULL OR work_id IS NOT NULL),
    CHECK ((message_id IS NULL AND run_id IS NULL)
        OR (message_id IS NOT NULL AND run_id IS NOT NULL)),
    FOREIGN KEY (work_id, message_id) REFERENCES messages(work_id, id) ON DELETE CASCADE,
    FOREIGN KEY (work_id, run_id) REFERENCES runs(work_id, id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX idx_resource_links_draft
ON resource_links(resource_id, draft_id, role)
WHERE draft_id IS NOT NULL;

CREATE UNIQUE INDEX idx_resource_links_work
ON resource_links(resource_id, work_id, role)
WHERE work_id IS NOT NULL AND message_id IS NULL AND run_id IS NULL;

CREATE UNIQUE INDEX idx_resource_links_message
ON resource_links(resource_id, work_id, message_id, run_id, role)
WHERE message_id IS NOT NULL AND run_id IS NOT NULL;

CREATE INDEX idx_resource_links_work_id ON resource_links(work_id, created_at);
CREATE INDEX idx_resource_links_draft_id ON resource_links(draft_id, created_at);

CREATE TRIGGER resource_links_same_space_insert
BEFORE INSERT ON resource_links
WHEN NEW.work_id IS NOT NULL
 AND (SELECT space_id FROM works WHERE id = NEW.work_id)
     <> (SELECT space_id FROM managed_resources WHERE id = NEW.resource_id)
BEGIN
    SELECT RAISE(ABORT, 'resource link space mismatch');
END;

CREATE TRIGGER resource_links_same_space_update
BEFORE UPDATE OF work_id, resource_id ON resource_links
WHEN NEW.work_id IS NOT NULL
 AND (SELECT space_id FROM works WHERE id = NEW.work_id)
     <> (SELECT space_id FROM managed_resources WHERE id = NEW.resource_id)
BEGIN
    SELECT RAISE(ABORT, 'resource link space mismatch');
END;
```

- [ ] **Step 4: Add DTOs and resource IDs to Work DTOs**

Create `src-tauri/src/domain/resource.rs` with these public contracts:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

macro_rules! binding_path {
    () => { concat!(env!("CARGO_MANIFEST_DIR"), "/../src/bindings/") };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, TS)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[ts(rename_all = "snake_case", export_to = binding_path!())]
pub enum ResourceStatus {
    Staging,
    Processing,
    Ready,
    Failed,
    Deleting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, TS)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
#[ts(rename_all = "snake_case", export_to = binding_path!())]
pub enum ResourceOrigin {
    UserUpload,
    GeneratedArtifact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct ResourceSummary {
    pub id: String,
    pub original_name: String,
    pub media_type: String,
    pub size: u64,
    pub origin: ResourceOrigin,
    pub status: ResourceStatus,
    pub failure_code: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct ResourceThumbnail {
    pub media_type: String,
    pub data_base64: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = binding_path!())]
pub struct ImportResourcesInput {
    pub source_paths: Vec<String>,
    pub draft_id: Option<String>,
    pub work_id: Option<String>,
}
```

Add `pub mod resource;` to `src-tauri/src/domain/mod.rs`, export the five types in `export_bindings`, and add their names to the generated-file assertion.

Change the Work DTO fields to exactly these shapes in `src-tauri/src/domain/work.rs`:

```rust
pub struct MessageSummary {
    pub id: String,
    pub work_id: String,
    pub run_id: String,
    pub role: MessageRole,
    pub content: String,
    pub resource_ids: Vec<String>,
    pub created_at: DateTime<Utc>,
}

pub struct CreateWorkInput {
    pub title: String,
    pub goal: String,
    pub root_path: String,
    pub permission_mode: PermissionMode,
    pub resource_draft_id: Option<String>,
}

pub struct StartWorkInput {
    pub prompt: String,
    pub referenced_files: Vec<String>,
    pub resource_ids: Vec<String>,
}
```

Update all existing Rust test fixtures to supply `resource_ids: Vec::new()` and `resource_draft_id: None`. Extend the `StartWorkInput` serialization assertion with `"resourceIds": []`.

- [ ] **Step 5: Generate bindings and run the focused suite**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml domain::tests::export_bindings
cargo test --manifest-path src-tauri/Cargo.toml --test storage_contract
```

Expected: both commands pass and the generated TypeScript DTOs use camel-case fields and snake-case status/origin values. Uploaded fixtures use `origin: "user_upload"`; the schema can also represent future `generated_artifact` resources without a migration.

- [ ] **Step 6: Commit the schema and contracts**

```powershell
git add src-tauri/migrations/0002_resources.sql src-tauri/src/domain/resource.rs src-tauri/src/domain/mod.rs src-tauri/src/domain/work.rs src-tauri/tests/storage_contract.rs src/bindings
git commit -m "feat: add managed resource schema and contracts"
```

### Task 2: Implement immutable local blob storage and resource paths

**Files:**
- Create: `src-tauri/src/resource/mod.rs`
- Create: `src-tauri/src/resource/blob_store.rs`
- Create: `src-tauri/src/resource/local_blob_store.rs`
- Modify: `src-tauri/src/paths.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Write failing path and blob-store tests**

Add these assertions to `src-tauri/src/paths.rs`:

```rust
assert_eq!(paths.resources_dir(), roaming_root.join("resources"));
assert_eq!(
    paths.resource_cache_dir(),
    local_root.join("resource-cache")
);
```

Add the following tests to `src-tauri/src/resource/local_blob_store.rs`:

```rust
#[tokio::test]
async fn put_is_content_addressed_and_deduplicated() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.png");
    std::fs::write(&source, b"same bytes").unwrap();
    let store = LocalBlobStore::new(root.path().join("objects"));

    let first = store.put(BlobInput { source_path: source.clone() }).await.unwrap();
    let second = store.put(BlobInput { source_path: source }).await.unwrap();

    assert_eq!(first, second);
    assert_eq!(first.sha256.len(), 64);
    assert_eq!(first.size, 10);
    assert_eq!(store.path_for(&first.reference).unwrap(), root.path().join("objects").join(&first.object_key));
}

#[tokio::test]
async fn storage_references_cannot_escape_the_resource_root() {
    let root = tempfile::tempdir().unwrap();
    let store = LocalBlobStore::new(root.path());
    let error = store
        .read(&StorageRef { object_key: "../secret".into() })
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "invalid blob object key");
}
```

- [ ] **Step 2: Run the focused tests and verify the APIs are absent**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml paths::tests::application_paths_stay_under_their_configured_roots
cargo test --manifest-path src-tauri/Cargo.toml resource::local_blob_store::tests
```

Expected: compilation fails because the resource module, path methods, and blob types do not exist.

- [ ] **Step 3: Add dependencies and the blob-store contract**

Add these dependencies to `src-tauri/Cargo.toml`:

```toml
base64 = "0.22"
image = { version = "0.25", default-features = false, features = ["gif", "jpeg", "png", "webp"] }
sha2 = "0.10"
```

Create `src-tauri/src/resource/blob_store.rs`:

```rust
use std::path::PathBuf;

use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobInput {
    pub source_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StorageRef {
    pub object_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredBlob {
    pub reference: StorageRef,
    pub object_key: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobMetadata {
    pub size: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ResourceStorageError {
    #[error("invalid blob object key")]
    InvalidObjectKey,
    #[error("blob I/O failed")]
    Io(#[source] std::io::Error),
}

#[async_trait]
pub trait BlobStore: Send + Sync {
    async fn put(&self, input: BlobInput) -> Result<StoredBlob, ResourceStorageError>;
    async fn read(&self, reference: &StorageRef) -> Result<Vec<u8>, ResourceStorageError>;
    async fn stat(&self, reference: &StorageRef) -> Result<BlobMetadata, ResourceStorageError>;
    async fn delete(&self, reference: &StorageRef) -> Result<(), ResourceStorageError>;
}
```

- [ ] **Step 4: Implement `LocalBlobStore` and durable paths**

Implement `LocalBlobStore` with `spawn_blocking`: stream the source through `Sha256`, write `<resources>/staging/<uuid>.part`, call `sync_all`, then rename it to `blobs/<hash[0..2]>/<hash>`. If the target already exists, delete only the newly written staging file. Validate every `StorageRef` as exactly `blobs/<two lowercase hex>/<64 lowercase hex>` before joining it to the root. `delete` ignores `NotFound` and never removes a directory.

The public shape in `src-tauri/src/resource/local_blob_store.rs` must be:

```rust
#[derive(Clone)]
pub struct LocalBlobStore {
    root: PathBuf,
}

impl LocalBlobStore {
    pub fn new(root: impl Into<PathBuf>) -> Self;
    pub fn path_for(&self, reference: &StorageRef) -> Result<PathBuf, ResourceStorageError>;
    pub fn staging_dir(&self) -> PathBuf;
    pub async fn list_objects_older_than(
        &self,
        cutoff: SystemTime,
    ) -> Result<Vec<StorageRef>, ResourceStorageError>;
}
```

`list_objects_older_than` scans only the exact `blobs/<two hex>/<64 hex>` layout, ignores symlinks and unexpected entries, and returns validated references whose file modification time is older than the cutoff. It never follows or deletes entries itself.

Create `src-tauri/src/resource/mod.rs`:

```rust
pub mod blob_store;
pub mod local_blob_store;
```

Add these methods to `AppPaths`:

```rust
pub fn resources_dir(&self) -> PathBuf {
    self.roaming_root.join("resources")
}

pub fn resource_cache_dir(&self) -> PathBuf {
    self.local_root.join("resource-cache")
}
```

Add `pub mod resource;` to `src-tauri/src/lib.rs`.

- [ ] **Step 5: Run blob and path tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml resource::local_blob_store::tests
cargo test --manifest-path src-tauri/Cargo.toml paths::tests
```

Expected: all blob-store and path tests pass; the object appears once at its hash-derived location.

- [ ] **Step 6: Commit local storage**

```powershell
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/lib.rs src-tauri/src/paths.rs src-tauri/src/resource
git commit -m "feat: add immutable local blob storage"
```

### Task 3: Import images, isolate per-file failures, and recover interrupted imports

**Files:**
- Create: `src-tauri/src/resource/repository.rs`
- Create: `src-tauri/src/resource/service.rs`
- Create: `src-tauri/tests/resource_lifecycle.rs`
- Modify: `src-tauri/src/error.rs`

- [ ] **Step 1: Write failing resource lifecycle tests**

Create `src-tauri/tests/resource_lifecycle.rs` with helpers that open an in-memory `Database`, create a temporary `LocalBlobStore`, and construct `ResourceService`. Add these tests:

```rust
#[tokio::test]
async fn imports_a_png_without_retaining_its_source_path() {
    let fixture = Fixture::new().await;
    let source = fixture.write_png("private/source/avatar.png");
    let imported = fixture.service.import_resources(ImportResourcesInput {
        source_paths: vec![source.to_string_lossy().into_owned()],
        draft_id: Some("draft-1".into()),
        work_id: None,
    }).await.unwrap();

    assert_eq!(imported.len(), 1);
    assert_eq!(imported[0].status, ResourceStatus::Ready);
    assert_eq!(imported[0].original_name, "avatar.png");
    let stored: String = sqlx::query_scalar(
        "SELECT object_key FROM blob_replicas WHERE store_id = 'local-default'",
    ).fetch_one(fixture.database.pool()).await.unwrap();
    assert!(!stored.contains("private"));
    assert!(!stored.contains("avatar.png"));
}

#[tokio::test]
async fn one_invalid_file_does_not_discard_a_valid_file() {
    let fixture = Fixture::new().await;
    let valid = fixture.write_png("valid.png");
    let invalid = fixture.write_bytes("script.png", b"not an image");
    let imported = fixture.service.import_resources(ImportResourcesInput {
        source_paths: vec![valid.to_string_lossy().into_owned(), invalid.to_string_lossy().into_owned()],
        draft_id: Some("draft-2".into()),
        work_id: None,
    }).await.unwrap();

    assert_eq!(imported[0].status, ResourceStatus::Ready);
    assert_eq!(imported[1].status, ResourceStatus::Failed);
    assert_eq!(imported[1].failure_code.as_deref(), Some("unsupported_image"));
}

#[tokio::test]
async fn repeated_bytes_share_a_blob_but_keep_distinct_resources() {
    let fixture = Fixture::new().await;
    let first = fixture.write_png("first.png");
    let second = fixture.copy_file(&first, "second.png");
    let imported = fixture.service.import_resources(ImportResourcesInput {
        source_paths: vec![first.to_string_lossy().into_owned(), second.to_string_lossy().into_owned()],
        draft_id: Some("draft-3".into()),
        work_id: None,
    }).await.unwrap();

    assert_ne!(imported[0].id, imported[1].id);
    let blobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM resource_blobs")
        .fetch_one(fixture.database.pool()).await.unwrap();
    assert_eq!(blobs, 1);
}

#[tokio::test]
async fn imported_image_survives_source_deletion_and_database_reopen() {
    let fixture = Fixture::new_file_backed().await;
    let source = fixture.write_png("temporary/source.png");
    let resource = fixture.import_draft_path("draft-durable", &source).await;
    std::fs::remove_file(&source).unwrap();
    let reopened = fixture.reopen().await;

    let storage_ref = reopened.resource_repository
        .storage_ref(&resource.id).await.unwrap();
    let bytes = reopened.blob_store.read(&storage_ref).await.unwrap();
    assert!(!bytes.is_empty());
}

#[tokio::test]
async fn recovery_garbage_collects_abandoned_drafts_without_deleting_shared_blobs() {
    let fixture = Fixture::new().await;
    let abandoned = fixture.import_ready_png("stale-draft", "abandoned.png").await;
    let retained = fixture.import_ready_png("current-draft", "retained.png").await;
    fixture.force_link_age(&abandoned.id, 25).await;
    fixture.force_link_age(&retained.id, 1).await;

    fixture.service.recover_interrupted_imports().await.unwrap();

    assert!(fixture.resource_repository.find(&abandoned.id).await.unwrap().is_none());
    assert!(fixture.resource_repository.find(&retained.id).await.unwrap().is_some());
    assert!(fixture.service.thumbnail(&retained.id).await.is_ok());
}

#[tokio::test]
async fn recovery_removes_old_local_objects_that_have_no_replica_record() {
    let fixture = Fixture::new().await;
    let source = fixture.write_png("orphan-source.png");
    let stored = fixture.blob_store.put(BlobInput { source_path: source }).await.unwrap();
    fixture.force_object_age(&stored.reference, 25);

    fixture.service.recover_interrupted_imports().await.unwrap();

    assert!(!fixture.blob_store.path_for(&stored.reference).unwrap().exists());
}

#[tokio::test]
async fn recovery_marks_stale_staging_resources_failed_and_removes_part_files() {
    let fixture = Fixture::new().await;
    fixture.insert_stale_staging_resource("resource-stale").await;
    let part = fixture.write_staging_part("abandoned.part");

    let recovered = fixture.service.recover_interrupted_imports().await.unwrap();

    assert_eq!(recovered, 1);
    assert!(!part.exists());
    let row: (String, Option<String>) = sqlx::query_as(
        "SELECT status, failure_code FROM managed_resources WHERE id = 'resource-stale'",
    ).fetch_one(fixture.database.pool()).await.unwrap();
    assert_eq!(row, ("failed".into(), Some("import_interrupted".into())));
}
```

Use a fixed valid 1×1 PNG byte array in `Fixture::write_png`; do not depend on a repository binary fixture.

- [ ] **Step 2: Run lifecycle tests and verify the repository/service are absent**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test resource_lifecycle
```

Expected: compilation fails because `ResourceRepository`, `ResourceService`, and resource error variants do not exist.

- [ ] **Step 3: Add product-safe resource errors**

Add these `AppError` variants:

```rust
#[error("resource not found: {resource_id}")]
ResourceNotFound { resource_id: String },

#[error("resource import failed: {code}")]
ResourceImport { code: String },

#[error("resource storage operation failed")]
ResourceStorage,
```

Serialize them with these stable codes and details:

```rust
AppError::ResourceNotFound { resource_id } => (
    "resource_not_found",
    json!({ "resourceId": resource_id }),
),
AppError::ResourceImport { code } => (
    "resource_import",
    json!({ "reason": code }),
),
AppError::ResourceStorage => ("resource_storage", json!({})),
```

Never include source paths, hashes, blob keys, decoder diagnostics, or raw I/O errors in serialized `message` or `details`.

- [ ] **Step 4: Implement `ResourceRepository`**

Give `ResourceRepository` this public API:

```rust
#[derive(Clone)]
pub struct ResourceRepository {
    pool: SqlitePool,
}

impl ResourceRepository {
    pub fn new(pool: SqlitePool) -> Self;
    pub async fn create_staging(
        &self,
        resource_id: &str,
        original_name: &str,
        media_type: &str,
        size: u64,
        draft_id: Option<&str>,
        work_id: Option<&str>,
    ) -> Result<ResourceSummary, AppError>;
    pub async fn complete_import(
        &self,
        resource_id: &str,
        sha256: &str,
        size: u64,
        object_key: &str,
    ) -> Result<ResourceSummary, AppError>;
    pub async fn fail_import(
        &self,
        resource_id: &str,
        failure_code: &str,
    ) -> Result<ResourceSummary, AppError>;
    pub async fn list_for_work(&self, work_id: &str) -> Result<Vec<ResourceSummary>, AppError>;
    pub async fn find(&self, resource_id: &str) -> Result<Option<ResourceSummary>, AppError>;
    pub async fn storage_ref(&self, resource_id: &str) -> Result<StorageRef, AppError>;
    pub async fn recover_stale_staging(&self, cutoff: DateTime<Utc>) -> Result<u64, AppError>;
    pub async fn collect_abandoned_drafts(
        &self,
        cutoff: DateTime<Utc>,
    ) -> Result<Vec<StorageRef>, AppError>;
    pub async fn local_replica_refs(&self) -> Result<HashSet<StorageRef>, AppError>;
}
```

Add `pub mod repository;` and `pub mod service;` to `src-tauri/src/resource/mod.rs` in this task, after both files exist.

`create_staging` validates exactly one non-empty `draft_id` or `work_id`, verifies the Work exists when supplied, inserts the staging resource with `space_id = local-personal` and `origin = user_upload`, inserts its `attached` link in one `BEGIN IMMEDIATE` transaction, and returns the inserted row. `complete_import` uses `INSERT OR IGNORE` plus a lookup on `(plaintext_sha256, size)` so duplicate bytes reuse one blob, upserts the `local-default` ready replica, and marks the logical resource ready in one transaction. `list_for_work` returns distinct resources ordered by link creation and resource ID.

Represent SQLite sizes as `i64` in private `ResourceRow`/`BlobRow` structs and convert with `u64::try_from` when building DTOs. Convert validated input sizes with `i64::try_from` before binding. Never bind or decode SQLite integers directly as `u64`.

- [ ] **Step 5: Implement bounded per-file image import**

Give `ResourceService` this public shape:

```rust
#[derive(Clone)]
pub struct ResourceService {
    repository: ResourceRepository,
    blob_store: Arc<LocalBlobStore>,
    cache_root: PathBuf,
}

impl ResourceService {
    pub fn new(
        repository: ResourceRepository,
        blob_store: Arc<LocalBlobStore>,
        cache_root: PathBuf,
    ) -> Self;
    pub async fn import_resources(
        &self,
        input: ImportResourcesInput,
    ) -> Result<Vec<ResourceSummary>, AppError>;
    pub async fn list_work_resources(
        &self,
        work_id: &str,
    ) -> Result<Vec<ResourceSummary>, AppError>;
    pub async fn thumbnail(
        &self,
        resource_id: &str,
    ) -> Result<ResourceThumbnail, AppError>;
    pub async fn recover_interrupted_imports(&self) -> Result<u64, AppError>;
}
```

For every source, derive only `file_name`, reject symlinks, reject non-files, reject files larger than `MAX_IMAGE_BYTES`, and inspect magic bytes before decoding. Create the staging record with the detected media type, or `application/octet-stream` when signature validation itself fails, so every selected file produces a durable ready/failed result. Decode through `image::ImageReader` with maximum width/height `MAX_IMAGE_DIMENSION` and allocation `MAX_DECODE_BYTES`; do not use the unbounded convenience decoder. Create staging before blob promotion; after staging exists, convert a per-file validation/storage failure into `fail_import` and continue the batch. Generate `<cache_root>/thumbnails/<resource-id>.png` with `thumbnail(192, 192)` and PNG encoding. A thumbnail-generation failure marks only that resource failed. The command itself returns `Err` only when target validation or the metadata database is unavailable.

`recover_interrupted_imports` marks staging/processing records older than 24 hours failed with `import_interrupted`, unlinks draft links older than 24 hours, removes logical resources with no remaining links, removes blob/replica rows with no remaining logical resources, and asks `LocalBlobStore` to delete the `StorageRef` values returned by `collect_abandoned_drafts`. It then compares `list_objects_older_than` against `local_replica_refs` and deletes only old validated local objects that have no replica row. Finally it removes only files whose direct parent is `LocalBlobStore::staging_dir()` and whose extension is `.part`. A blob shared by any retained Work/draft resource must remain.

- [ ] **Step 6: Run lifecycle and error serialization tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test resource_lifecycle
cargo test --manifest-path src-tauri/Cargo.toml error::tests
```

Expected: valid images are ready, invalid images are isolated failures, identical bytes share one blob, stale imports recover, and serialized errors contain no local paths.

- [ ] **Step 7: Commit the import lifecycle**

```powershell
git add src-tauri/src/error.rs src-tauri/src/resource/repository.rs src-tauri/src/resource/service.rs src-tauri/tests/resource_lifecycle.rs
git commit -m "feat: import durable local image resources"
```

### Task 4: Expose resource commands and assemble the production service

**Files:**
- Create: `src-tauri/src/resource/commands.rs`
- Modify: `src-tauri/src/app_state.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/resource/mod.rs`

- [ ] **Step 1: Write failing command and state wiring tests**

Add this compile-time command test to `src-tauri/src/resource/commands.rs`:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn typed_resource_command_items_typecheck() {
        let _ = super::import_resources;
        let _ = super::list_work_resources;
        let _ = super::get_resource_thumbnail;
        let _ = super::detach_draft_resource;
    }
}
```

Add a source-wiring assertion beside the existing startup tests in `src-tauri/src/lib.rs`:

```rust
#[test]
fn production_registers_every_resource_command() {
    let source = include_str!("lib.rs");
    for command in [
        "resource::commands::import_resources",
        "resource::commands::list_work_resources",
        "resource::commands::get_resource_thumbnail",
        "resource::commands::detach_draft_resource",
    ] {
        assert!(source.contains(command), "missing {command}");
    }
}
```

- [ ] **Step 2: Run tests and verify the commands are missing**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml resource::commands::tests
cargo test --manifest-path src-tauri/Cargo.toml production_registers_every_resource_command
```

Expected: compilation or assertions fail because the commands and managed state wiring are absent.

- [ ] **Step 3: Add resource commands and draft detachment**

Add this repository/service method pair:

```rust
pub async fn detach_draft_resource(
    &self,
    draft_id: &str,
    resource_id: &str,
) -> Result<(), AppError> {
    self.repository.detach_draft_resource(draft_id, resource_id).await
}
```

The repository implementation must execute this exact scoped deletion and return `ResourceNotFound` when no row changes:

```sql
DELETE FROM resource_links
WHERE draft_id = ? AND resource_id = ? AND work_id IS NULL
```

Create `src-tauri/src/resource/commands.rs`:

```rust
use tauri::State;

use crate::{
    app_state::AppState,
    domain::resource::{ImportResourcesInput, ResourceSummary, ResourceThumbnail},
    error::AppError,
};

#[tauri::command(rename_all = "camelCase")]
pub async fn import_resources(
    state: State<'_, AppState>,
    input: ImportResourcesInput,
) -> Result<Vec<ResourceSummary>, AppError> {
    state.resource_service().import_resources(input).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn list_work_resources(
    state: State<'_, AppState>,
    work_id: String,
) -> Result<Vec<ResourceSummary>, AppError> {
    state.resource_service().list_work_resources(&work_id).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn get_resource_thumbnail(
    state: State<'_, AppState>,
    resource_id: String,
) -> Result<ResourceThumbnail, AppError> {
    state.resource_service().thumbnail(&resource_id).await
}

#[tauri::command(rename_all = "camelCase")]
pub async fn detach_draft_resource(
    state: State<'_, AppState>,
    draft_id: String,
    resource_id: String,
) -> Result<(), AppError> {
    state.resource_service().detach_draft_resource(&draft_id, &resource_id).await
}
```

Add `pub mod commands;` to `src-tauri/src/resource/mod.rs` after creating the command file.

- [ ] **Step 4: Put `ResourceService` in managed state and startup recovery**

Extend `AppState` with `resource_service: Option<Arc<ResourceService>>`, keep `AppState::new` and `with_model_service` for existing tests, and add this production constructor/accessor:

```rust
pub fn with_services(
    work_service: Arc<WorkService>,
    model_service: Arc<ModelService>,
    resource_service: Arc<ResourceService>,
) -> Self {
    Self {
        work_service,
        model_service: Some(model_service),
        resource_service: Some(resource_service),
    }
}

pub fn resource_service(&self) -> &Arc<ResourceService> {
    self.resource_service
        .as_ref()
        .expect("production AppState must include ResourceService")
}
```

In `application_builder`, capture `paths.resources_dir()` and `paths.resource_cache_dir()`. During the startup prepare closure, create `ResourceRepository`, `LocalBlobStore`, and `ResourceService`, then call `recover_interrupted_imports()` after database migrations and Work recovery. Return the service into assembly, use `AppState::with_services`, and register the four resource commands in `tauri::generate_handler!`.

The startup ordering must remain:

```text
open database -> migrate -> recover interrupted Runs -> recover interrupted imports
-> assemble model/engine/services -> manage state -> show main window
```

- [ ] **Step 5: Run command and startup tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml resource::commands::tests
cargo test --manifest-path src-tauri/Cargo.toml production_registers_every_resource_command
cargo test --manifest-path src-tauri/Cargo.toml startup
```

Expected: commands typecheck, registration is present, and startup still fails closed without showing the main window when preparation fails.

- [ ] **Step 6: Commit command wiring**

```powershell
git add src-tauri/src/resource/commands.rs src-tauri/src/resource/mod.rs src-tauri/src/app_state.rs src-tauri/src/lib.rs
git commit -m "feat: expose managed resource commands"
```

### Task 5: Make Run/message attachment association authoritative and pass typed images to engines

**Files:**
- Modify: `src-tauri/src/work/repository.rs`
- Modify: `src-tauri/src/work/service.rs`
- Modify: `src-tauri/src/engine/mod.rs`
- Modify: `src-tauri/src/engine/fake.rs`
- Modify: `src-tauri/src/engine/supervisor.rs`
- Modify: `src-tauri/src/resource/service.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/tests/work_lifecycle.rs`
- Modify: `src-tauri/tests/resource_lifecycle.rs`

- [ ] **Step 1: Write failing draft-adoption and message-link tests**

Add these integration tests to `src-tauri/tests/resource_lifecycle.rs`:

```rust
#[tokio::test]
async fn creating_a_work_adopts_its_resource_draft() {
    let fixture = Fixture::new().await;
    let resource = fixture.import_ready_png("draft-42", "brief.png").await;
    let work = fixture.work_repository.create(CreateWorkInput {
        title: "Review brief".into(),
        goal: "Review the attached image".into(),
        root_path: fixture.workspace_path(),
        permission_mode: PermissionMode::Balanced,
        resource_draft_id: Some("draft-42".into()),
    }).await.unwrap();

    let resources = fixture.resource_repository
        .list_for_work(&work.summary.id).await.unwrap();
    assert_eq!(resources, vec![resource]);
    let draft_links: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM resource_links WHERE draft_id = 'draft-42'",
    ).fetch_one(fixture.database.pool()).await.unwrap();
    assert_eq!(draft_links, 0);
}

#[tokio::test]
async fn begin_run_links_only_ready_resources_owned_by_the_work() {
    let fixture = Fixture::new().await;
    let work = fixture.create_work("Attached run").await;
    let resource = fixture.import_ready_work_png(&work.summary.id, "chart.png").await;

    let started = fixture.work_repository.begin_run(
        &work.summary.id,
        "Explain this",
        &[resource.id.clone()],
        "fake",
        "Fake model",
    ).await.unwrap();

    assert_eq!(started.user_message.resource_ids, vec![resource.id.clone()]);
    let reloaded = fixture.work_repository.get(&work.summary.id).await.unwrap().unwrap();
    assert_eq!(reloaded.messages[0].resource_ids, vec![resource.id]);
}

#[tokio::test]
async fn attachment_only_run_is_valid_but_empty_run_is_rejected() {
    let fixture = Fixture::new().await;
    let work = fixture.create_work("Image only").await;
    let resource = fixture.import_ready_work_png(&work.summary.id, "photo.png").await;

    fixture.work_repository.begin_run(
        &work.summary.id,
        "",
        &[resource.id],
        "fake",
        "Fake model",
    ).await.unwrap();

    let other = fixture.create_work("Empty").await;
    let error = fixture.work_repository.begin_run(
        &other.summary.id,
        "",
        &[],
        "fake",
        "Fake model",
    ).await.unwrap_err();
    assert!(matches!(error, AppError::InvalidInput { field, .. } if field == "prompt"));
}
```

Add a unit test to `src-tauri/src/engine/fake.rs` that starts the fake engine with `EngineInput { message: "Inspect".into(), images: vec![EngineImage { media_type: "image/png".into(), data: vec![1, 2, 3] }] }` and asserts its assistant delta still says `Working on: Inspect`.

- [ ] **Step 2: Run focused tests and verify current signatures fail**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test resource_lifecycle begin_run_links_only_ready_resources_owned_by_the_work
cargo test --manifest-path src-tauri/Cargo.toml engine::fake::tests
```

Expected: compilation fails because `begin_run` has no resource IDs and `EngineInput`/`EngineImage` do not exist.

- [ ] **Step 3: Add typed engine input**

Add these definitions to `src-tauri/src/engine/mod.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineImage {
    pub media_type: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineInput {
    pub message: String,
    pub images: Vec<EngineImage>,
}
```

Change the adapter signature to:

```rust
async fn start(
    &self,
    context: EngineRunContext,
    input: EngineInput,
    sink: mpsc::Sender<EngineEvent>,
) -> Result<EngineSessionRef, EngineError>;
```

Update `FakeEngineAdapter` and every test adapter to read `input.message`. Existing tests that do not exercise attachments pass `EngineInput { message: prompt.into(), images: Vec::new() }`.

Update every direct `WorkRepository::begin_run` call to pass `&[]` between the prompt and engine kind unless that test is explicitly selecting resource IDs.

- [ ] **Step 4: Adopt drafts and atomically link message resources**

In `WorkRepository::create`, keep `resource_draft_id` before moving the remaining fields, open one `BEGIN IMMEDIATE` transaction for the Work insert, and run:

```sql
UPDATE resource_links
SET work_id = ?, draft_id = NULL
WHERE draft_id = ? AND work_id IS NULL
```

before committing. This ensures a successful `create` response already owns every resource selected in the new-Work composer.

Change `begin_run` to this signature:

```rust
pub async fn begin_run(
    &self,
    work_id: &str,
    prompt: &str,
    resource_ids: &[String],
    engine_kind: &str,
    model_label: &str,
) -> Result<StartWorkOutput, AppError>
```

Trim the prompt, deduplicate resource IDs without reordering, and reject only when both prompt and resource IDs are empty. Inside the existing transaction, verify the count of ready resources that have a Work-level `attached`/`pinned` link equals the deduplicated count:

```sql
SELECT COUNT(DISTINCT managed_resources.id)
FROM managed_resources
INNER JOIN resource_links ON resource_links.resource_id = managed_resources.id
WHERE managed_resources.id IN (SELECT value FROM json_each(?))
  AND managed_resources.status = 'ready'
  AND resource_links.work_id = ?
```

Bind a JSON array produced by `serde_json::to_string`. If the counts differ, return `AppError::ResourceImport { code: "resource_not_ready_or_unlinked".into() }` before inserting the Run. After inserting the user message, insert one `resource_links` row per ID with the new `work_id`, `message_id`, `run_id`, role `attached`, and the same timestamp, then commit.

Set `user_message.resource_ids` to the deduplicated IDs. In `load_detail`, load all message links in one query ordered by link creation, group them by `message_id`, and assign the resulting IDs after converting `MessageRow`; do not issue one query per message.

- [ ] **Step 5: Materialize bounded engine images and connect WorkService**

Add to `ResourceService`:

```rust
pub async fn engine_images(
    &self,
    work_id: &str,
    resource_ids: &[String],
) -> Result<Vec<EngineImage>, AppError>;
```

The repository query behind it must return `media_type`, `size`, and `object_key` only for ready resources linked to that Work. Preserve request order, reject more than 8 images, reject cumulative stored bytes above 24 MiB, and load each object through `BlobStore::read`. Verify the loaded length equals the stored size before constructing `EngineImage`.

Extend `WorkService` with `resource_service: Option<Arc<ResourceService>>` and add a production constructor:

```rust
pub fn with_supervisor_and_resources(
    repository: WorkRepository,
    supervisor: Arc<EngineSupervisor>,
    resource_service: Arc<ResourceService>,
) -> Self;
```

In `start_work`, keep `input.resource_ids`, build the `@`-expanded text exactly as today, load managed images separately, and call:

```rust
supervisor.start_with_engine_input(
    work_id,
    &user_prompt,
    resource_ids,
    EngineInput { message: engine_prompt, images },
).await
```

This is the hard boundary: `project_files::build_engine_prompt` receives only `referenced_files`; uploaded image bytes never enter `<referenced_files>` or prompt text.

Update production assembly in `src-tauri/src/lib.rs` to use `WorkService::with_supervisor_and_resources(repository, supervisor, Arc::clone(&resource_service))`.

- [ ] **Step 6: Carry IDs/input through `EngineSupervisor`**

Replace `start_with_engine_prompt` with:

```rust
pub async fn start_with_engine_input(
    &self,
    work_id: &str,
    user_prompt: &str,
    resource_ids: Vec<String>,
    engine_input: EngineInput,
) -> Result<StartWorkOutput, AppError>
```

`start()` calls it with an empty ID vector and empty images. `run_lifecycle` receives both new values, calls:

```rust
repository
    .begin_run(&work_id, &user_prompt, &resource_ids, engine.kind(), &model_label)
    .await
```

and starts the adapter with `engine.start(context, engine_input, event_sender)`. Preserve all existing timeout, abort ownership, event-consumer, and persisted-instruction behavior.

- [ ] **Step 7: Run backend lifecycle suites**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test resource_lifecycle
cargo test --manifest-path src-tauri/Cargo.toml --test work_lifecycle
cargo test --manifest-path src-tauri/Cargo.toml engine::fake::tests
```

Expected: draft adoption and message links pass, attachment-only runs pass, empty runs fail, and all prior supervisor race/abort tests remain green.

- [ ] **Step 8: Commit authoritative Run integration**

```powershell
git add src-tauri/src/work/repository.rs src-tauri/src/work/service.rs src-tauri/src/engine/mod.rs src-tauri/src/engine/fake.rs src-tauri/src/engine/supervisor.rs src-tauri/src/resource/service.rs src-tauri/src/lib.rs src-tauri/tests/resource_lifecycle.rs src-tauri/tests/work_lifecycle.rs
git commit -m "feat: attach managed images to work runs"
```

### Task 6: Send uploaded images through Pi's native RPC protocol

**Files:**
- Modify: `src-tauri/src/engine/pi/mod.rs`
- Modify: `src-tauri/tests/pi_engine.rs`

- [ ] **Step 1: Write a failing Pi RPC payload test**

Add this test to `src-tauri/tests/pi_engine.rs`:

```rust
#[test]
fn prompt_command_uses_pi_native_images_without_embedding_bytes_in_message() {
    use piwork_lib::engine::{EngineImage, EngineInput};
    use piwork_lib::engine::pi::prompt_command;

    let command = prompt_command(
        "request-1",
        &EngineInput {
            message: "Compare the screenshots".into(),
            images: vec![EngineImage {
                media_type: "image/png".into(),
                data: vec![0, 1, 2, 255],
            }],
        },
    );

    assert_eq!(command, json!({
        "id": "request-1",
        "type": "prompt",
        "message": "Compare the screenshots",
        "images": [{
            "type": "image",
            "mimeType": "image/png",
            "data": "AAEC/w=="
        }]
    }));
    assert!(!command["message"].as_str().unwrap().contains("AAEC/w=="));
}
```

- [ ] **Step 2: Run the test and verify the builder is absent**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test pi_engine prompt_command_uses_pi_native_images_without_embedding_bytes_in_message
```

Expected: compilation fails because `prompt_command` does not exist and the Pi adapter still accepts a `String`.

- [ ] **Step 3: Implement one testable RPC command builder**

Add this public function to `src-tauri/src/engine/pi/mod.rs`:

```rust
pub fn prompt_command(request_id: &str, input: &EngineInput) -> Value {
    use base64::{Engine as _, engine::general_purpose::STANDARD};

    let images = input.images.iter().map(|image| json!({
        "type": "image",
        "mimeType": image.media_type,
        "data": STANDARD.encode(&image.data),
    })).collect::<Vec<_>>();

    json!({
        "id": request_id,
        "type": "prompt",
        "message": input.message,
        "images": images,
    })
}
```

Change `PiEngineAdapter::start` to accept `EngineInput` and replace the inline JSON with:

```rust
write_rpc(&mut stdin, &prompt_command(&request_id, &input)).await?;
```

Keep the `images` property as an empty array for text-only Runs so the protocol shape is deterministic. Do not write encoded image data, hashes, or object keys to logs or startup diagnostics.

- [ ] **Step 4: Run Pi and engine suites**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test pi_engine
cargo test --manifest-path src-tauri/Cargo.toml engine::pi::tests
cargo test --manifest-path src-tauri/Cargo.toml --test work_lifecycle
```

Expected: the native payload assertion passes and all existing Pi translation/lifecycle tests remain green.

- [ ] **Step 5: Commit Pi image transport**

```powershell
git add src-tauri/src/engine/pi/mod.rs src-tauri/tests/pi_engine.rs
git commit -m "feat: send native image inputs to pi rpc"
```

### Task 7: Add the frontend resource client and authoritative store state

**Files:**
- Modify: `src/app/tauriClient.ts`
- Modify: `src/test/mockTauriClient.ts`
- Modify: `src/features/works/workStore.ts`
- Modify: `src/features/works/workStore.test.ts`
- Modify: `src/domain/appError.ts`

- [ ] **Step 1: Write failing client/store tests**

Add these tests to `src/features/works/workStore.test.ts`:

```ts
it("imports Work resources and keeps failed files isolated", async () => {
  const client = createMockTauriClient();
  client.seed(workDetail());
  client.importResources.mockResolvedValueOnce([
    resource({ id: "resource-ready", originalName: "ready.png" }),
    resource({
      id: "resource-failed",
      originalName: "broken.png",
      status: "failed",
      failureCode: "unsupported_image",
    }),
  ]);
  const store = createWorkStore(client);

  const imported = await store.getState().importResources({
    sourcePaths: ["C:/private/ready.png", "C:/private/broken.png"],
    workId: "work-1",
    draftId: null,
  });

  expect(imported).toHaveLength(2);
  expect(store.getState().resources["work-1"]).toEqual(imported);
  expect(store.getState().error).toBeNull();
});

it("copies resource ids when an instruction is queued", () => {
  const store = createWorkStore(createMockTauriClient());
  const resourceIds = ["resource-1"];

  store.getState().queueInstruction("work-1", "Compare", [], resourceIds);
  resourceIds.push("resource-2");

  expect(store.getState().queuedInstructions["work-1"]).toEqual([{
    prompt: "Compare",
    referencedFiles: [],
    resourceIds: ["resource-1"],
  }]);
});

it("hydrates Work resources beside the Work detail", async () => {
  const client = createMockTauriClient();
  client.seed(workDetail());
  client.listWorkResources.mockResolvedValueOnce([
    resource({ id: "resource-1", originalName: "chart.png" }),
  ]);
  const store = createWorkStore(client);

  await store.getState().hydrate();

  expect(client.listWorkResources).toHaveBeenCalledWith("work-1");
  expect(store.getState().resources["work-1"]?.[0]?.id).toBe("resource-1");
});
```

The local `resource` test helper must construct a complete `ResourceSummary` with `mediaType: "image/png"`, `size: 68`, `origin: "user_upload"`, `status: "ready"`, `failureCode: null`, and a fixed ISO timestamp.

- [ ] **Step 2: Run tests and verify client/store APIs are missing**

Run:

```powershell
pnpm test -- src/features/works/workStore.test.ts
```

Expected: TypeScript compilation fails because resource methods/state and `resourceIds` are absent.

- [ ] **Step 3: Extend `PiWorkClient` and Tauri invocation payloads**

Import `ImportResourcesInput`, `ResourceSummary`, and `ResourceThumbnail` from bindings. Add these methods to `PiWorkClient`:

```ts
importResources(input: ImportResourcesInput): Promise<ResourceSummary[]>;
listWorkResources(workId: string): Promise<ResourceSummary[]>;
getResourceThumbnail(resourceId: string): Promise<ResourceThumbnail>;
detachDraftResource(draftId: string, resourceId: string): Promise<void>;
```

Change `startWork` to:

```ts
startWork(
  workId: string,
  prompt: string,
  referencedFiles?: string[],
  resourceIds?: string[],
): Promise<StartWorkOutput>;
```

Implement the transport exactly as:

```ts
importResources: (input) =>
  invoke<ResourceSummary[]>("import_resources", { input }),
listWorkResources: (workId) =>
  invoke<ResourceSummary[]>("list_work_resources", { workId }),
getResourceThumbnail: (resourceId) =>
  invoke<ResourceThumbnail>("get_resource_thumbnail", { resourceId }),
detachDraftResource: (draftId, resourceId) =>
  invoke<void>("detach_draft_resource", { draftId, resourceId }),
startWork: (workId, prompt, referencedFiles = [], resourceIds = []) =>
  invoke<StartWorkOutput>("start_work", {
    workId,
    input: { prompt, referencedFiles, resourceIds },
  }),
```

- [ ] **Step 4: Extend store state and preserve IDs across queue/start recovery**

Add these fields/actions to `WorkState`:

```ts
resources: Record<string, ResourceSummary[]>;
importResources(input: ImportResourcesInput): Promise<ResourceSummary[]>;
startWork(
  workId: string,
  prompt: string,
  referencedFiles?: string[],
  resourceIds?: string[],
): Promise<StartWorkOutput>;
queueInstruction(
  workId: string,
  prompt: string,
  referencedFiles?: string[],
  resourceIds?: string[],
): void;
```

Change `QueuedInstruction` to:

```ts
export type QueuedInstruction = {
  prompt: string;
  referencedFiles: string[];
  resourceIds: string[];
};
```

Initialize `resources: {}`. In `hydrate` and `selectWork`, request `client.getWork(workId)` and `client.listWorkResources(workId)` with `Promise.all`, apply the detail only if the existing intent/request guards still match, then store the resource list under the Work ID. After `createWork`, fetch `listWorkResources(detail.summary.id)` so an adopted draft appears immediately.

Implement `importResources` with the existing operation ownership helpers. Merge returned summaries by resource ID when `input.workId` is non-null; draft imports are returned to the composer without being placed under a fake Work key. Call `client.startWork(workId, prompt, referencedFiles, resourceIds)`. During start failure recovery, treat a newly persisted user message with matching resource IDs as persisted exactly as text messages are treated today. Queue only when trimmed text or at least one resource ID exists, and copy both arrays.

- [ ] **Step 5: Expand the mock client without storing source paths**

Add mock functions for all four resource methods. Maintain `resourcesByWork`, `resourcesByDraft`, and `thumbnailByResource` maps. `importResources` derives `originalName` from the final path segment, creates ready summaries, and stores no source path. `createWork` moves resources from `input.resourceDraftId` to the new Work. `startWork` stores the passed IDs in `userMessage.resourceIds`. Existing message fixtures must use `resourceIds: []`.

Add explicit `Mock<PiWorkClient["importResources"]>`, `Mock<PiWorkClient["listWorkResources"]>`, `Mock<PiWorkClient["getResourceThumbnail"]>`, and `Mock<PiWorkClient["detachDraftResource"]>` fields to `MockTauriClient` so individual UI tests can override them. Also add `seedResource(workId: string, resource: ResourceSummary): void` for UI fixtures.

Update every TypeScript `CreateWorkInput` literal outside `NewWorkStart` with `resourceDraftId: null`, and update every `MessageSummary` fixture with `resourceIds: []`. This includes `workStore.test.ts`, `useWorkEvents.test.tsx`, `WorkSurface.test.tsx`, and `mockTauriClient.ts`.

- [ ] **Step 6: Map resource errors to product copy**

In `src/domain/appError.ts`, map `resource_import` to `errors.resourceImport`, `resource_storage` to `errors.resourceStorage`, and `resource_not_found` to `errors.resourceNotFound`. Do not render backend `reason` values as user-facing prose.

- [ ] **Step 7: Run store and type checks**

Run:

```powershell
pnpm test -- src/features/works/workStore.test.ts
pnpm typecheck
```

Expected: resource import/hydration/queue tests pass and every generated DTO use compiles.

- [ ] **Step 8: Commit frontend state/transport**

```powershell
git add src/app/tauriClient.ts src/test/mockTauriClient.ts src/features/works/workStore.ts src/features/works/workStore.test.ts src/domain/appError.ts
git commit -m "feat: add attachment client and work state"
```

### Task 8: Build the native picker and reusable compact attachment controls

**Files:**
- Create: `src/app/attachmentPicker.ts`
- Create: `src/features/workspace/AttachmentButton.tsx`
- Create: `src/features/workspace/AttachmentDraftList.tsx`
- Create: `src/features/workspace/AttachmentChips.tsx`
- Modify: `src/styles/workspace.css`

- [ ] **Step 1: Write failing component tests through `WorkSurface.test.tsx`**

Add a test-only `pickAttachments` mock returning `C:/private/chart.png` and `C:/private/photo.jpg`. Add this test:

```tsx
it("uploads multiple images through an injected native picker without showing full paths", async () => {
  const client = createMockTauriClient();
  client.seed(workDetail());
  const pickAttachments = vi.fn(async () => [
    "C:/private/chart.png",
    "C:/private/photo.jpg",
  ]);
  render(<WorkSurface client={client} pickAttachments={pickAttachments} />);

  await screen.findByText("Work 1");
  await userEvent.click(screen.getByRole("button", { name: "添加附件" }));
  await userEvent.click(screen.getByRole("button", { name: "上传图片" }));

  expect(pickAttachments).toHaveBeenCalledOnce();
  expect(await screen.findByText("chart.png")).toBeInTheDocument();
  expect(screen.getByText("photo.jpg")).toBeInTheDocument();
  expect(screen.queryByText(/C:\/private/)).not.toBeInTheDocument();
});
```

Add a second test where one returned `ResourceSummary` has `status: "failed"`; assert its chip has an alert label while the ready chip remains selectable.

- [ ] **Step 2: Run the UI test and verify picker/components are absent**

Run:

```powershell
pnpm test -- src/features/workspace/WorkSurface.test.tsx
```

Expected: compilation fails because `pickAttachments` and the attachment controls do not exist.

- [ ] **Step 3: Implement the injectable native picker**

Create `src/app/attachmentPicker.ts`:

```ts
import { open } from "@tauri-apps/plugin-dialog";

export type PickAttachments = () => Promise<string[]>;

export const pickAttachments: PickAttachments = async () => {
  const selected = await open({
    directory: false,
    multiple: true,
    filters: [{
      name: "Images",
      extensions: ["png", "jpg", "jpeg", "gif", "webp"],
    }],
  });
  if (!selected) return [];
  return Array.isArray(selected) ? selected : [selected];
};
```

- [ ] **Step 4: Implement the compact controls with explicit interfaces**

`AttachmentButton` must use this prop contract:

```ts
type AttachmentButtonProps = {
  available: ResourceSummary[];
  disabled?: boolean;
  draftId: string | null;
  pickAttachments: PickAttachments;
  selectedIds: string[];
  workId: string | null;
  onImported(resources: ResourceSummary[]): void;
  onSelectedIdsChange(resourceIds: string[]): void;
};
```

Clicking its compact `Plus` icon opens a popover. The popover lists ready resources already associated with the Work/draft as `aria-pressed` toggle buttons and ends with an “Upload images” button. That button calls the injected picker, then `useWorkStore(state => state.importResources)` with exactly one of `workId` or `draftId`. It calls `onImported` with every returned summary and automatically selects only newly ready IDs. Disable repeat activation while import is pending. Close on Escape and outside pointer-down, and restore focus to the plus button.

`AttachmentDraftList` must render selected ready resources plus failed imports. It receives `resources`, `selectedIds`, and `onRemove(resource)`. Each item shows a 32×32 lazy thumbnail, basename, a localized failure marker when needed, and a remove button. It must never accept or render a local path prop.

`AttachmentChips` is read-only and receives `resources: ResourceSummary[]`; it renders the same basename/status language without remove controls.

For lazy thumbnails, read `client.getResourceThumbnail(resource.id)` from `useWorkStoreContext`, form `data:${mediaType};base64,${dataBase64}`, cancel state updates after unmount, and use an image icon while loading or after failure.

- [ ] **Step 5: Add compact styles**

Add CSS classes for `.attachment-button`, `.attachment-popover`, `.attachment-drafts`, `.attachment-chip`, `.attachment-chip__thumbnail`, and `.attachment-chips`. Use 28–32 px controls, 8 px gaps, one-line ellipsis names, a maximum 260 px popover width, and the existing token colors. Failed chips use the existing danger color and do not disappear. Ensure focus-visible rings match existing buttons.

- [ ] **Step 6: Run the focused UI and accessibility checks**

Run:

```powershell
pnpm test -- src/features/workspace/WorkSurface.test.tsx
pnpm typecheck
```

Expected: multiple files import, private directories never render, one failed resource does not hide ready resources, and the popover controls have accessible names/states.

- [ ] **Step 7: Commit reusable attachment controls**

```powershell
git add src/app/attachmentPicker.ts src/features/workspace/AttachmentButton.tsx src/features/workspace/AttachmentDraftList.tsx src/features/workspace/AttachmentChips.tsx src/styles/workspace.css src/features/workspace/WorkSurface.test.tsx
git commit -m "feat: add compact image attachment controls"
```

### Task 9: Integrate attachments into the existing-Work composer and queue

**Files:**
- Modify: `src/features/workspace/WorkComposer.tsx`
- Modify: `src/features/workspace/WorkSurface.tsx`
- Modify: `src/features/workspace/WorkSurface.test.tsx`
- Modify: `src/features/works/workStore.test.ts`

- [ ] **Step 1: Write failing send, attachment-only, reuse, and queue tests**

Add these UI assertions to `src/features/workspace/WorkSurface.test.tsx`:

```tsx
it("sends selected image ids separately from text and project references", async () => {
  const client = createMockTauriClient();
  client.seed(workDetail());
  render(<WorkSurface
    client={client}
    pickAttachments={async () => ["C:/private/chart.png"]}
  />);
  await screen.findByText("Work 1");
  await userEvent.click(screen.getByRole("button", { name: "添加附件" }));
  await userEvent.click(screen.getByRole("button", { name: "上传图片" }));
  await screen.findByText("chart.png");
  await userEvent.type(screen.getByLabelText("给 PiWork 指令"), "解释图表");
  await userEvent.click(screen.getByRole("button", { name: "发送" }));

  expect(client.startWork).toHaveBeenLastCalledWith(
    "work-1",
    "解释图表",
    [],
    [expect.stringMatching(/^resource-/)],
  );
});

it("allows an attachment-only message and can reuse a Work attachment", async () => {
  const client = createMockTauriClient();
  client.seed(workDetail());
  client.seedResource("work-1", resource({ id: "resource-existing", originalName: "saved.png" }));
  render(<WorkSurface client={client} />);
  await screen.findByText("Work 1");
  await userEvent.click(screen.getByRole("button", { name: "添加附件" }));
  await userEvent.click(screen.getByRole("button", { name: "saved.png" }));

  const send = screen.getByRole("button", { name: "发送" });
  expect(send).toBeEnabled();
  await userEvent.click(send);
  expect(client.startWork).toHaveBeenLastCalledWith(
    "work-1", "", [], ["resource-existing"],
  );
});
```

Extend the existing running-Work queue test to upload an image, queue it, mutate the component selection afterward, and assert the queued entry retains the original ID only.

- [ ] **Step 2: Run focused tests and verify the composer rejects image-only send**

Run:

```powershell
pnpm test -- src/features/workspace/WorkSurface.test.tsx -t "attachment-only|selected image ids|running"
```

Expected: tests fail because `WorkComposer` only enables and submits trimmed text.

- [ ] **Step 3: Wire resource state and picker dependency through `WorkSurface`**

Extend the exported props exactly as:

```ts
export function WorkSurface({
  client,
  modelLabel = "Pi",
  pickProjectDirectory = openProjectDirectory,
  pickAttachments = openAttachments,
}: {
  client?: PiWorkClient;
  modelLabel?: string;
  pickProjectDirectory?: PickProjectDirectory;
  pickAttachments?: PickAttachments;
})
```

Read `resources[selectedWork.id] ?? []` in `SurfaceContent`, pass it and `pickAttachments` to `WorkComposer`, and later pass the same array to timeline/inspector. Do not put source paths into Surface state.

- [ ] **Step 4: Update `WorkComposer` selection/send behavior**

Add local `attachmentResults: ResourceSummary[]` and `selectedResourceIds: string[]`. Merge imported results by ID. The resources presented to `AttachmentButton` are the union of Work resources and current results, deduplicated by ID.

Render `AttachmentButton` at the left of `.work-composer__actions` and `AttachmentDraftList` above the action row. Removing an existing-Work attachment changes only the current selection; the durable Work resource remains available in the plus popover.

Define sendability as:

```ts
const readyResourceIds = selectedResourceIds.filter((id) =>
  availableResources.some((resource) =>
    resource.id === id && resource.status === "ready"),
);
const canSubmit = Boolean(prompt.trim()) || readyResourceIds.length > 0;
```

Queue with `queueInstruction(work.id, instruction, referencedFiles, readyResourceIds)` and start with `startWork(work.id, instruction, referencedFiles, readyResourceIds)`. Clear prompt, references, current results, and selected IDs only after queue acceptance, successful start, or `didPersistStartInstruction(error)`. Retain all draft state on a non-persisted start failure.

- [ ] **Step 5: Run WorkSurface/store regression tests**

Run:

```powershell
pnpm test -- src/features/workspace/WorkSurface.test.tsx src/features/works/workStore.test.ts
```

Expected: text+image, image-only, reuse, queue copying, error retention, and existing `@` reference tests all pass.

- [ ] **Step 6: Commit existing-Work integration**

```powershell
git add src/features/workspace/WorkComposer.tsx src/features/workspace/WorkSurface.tsx src/features/workspace/WorkSurface.test.tsx src/features/works/workStore.test.ts
git commit -m "feat: attach images from existing work composer"
```

### Task 10: Integrate stable attachment drafts into new-Work creation

**Files:**
- Modify: `src/features/workspace/NewWorkStart.tsx`
- Modify: `src/features/workspace/WorkSurface.tsx`
- Modify: `src/features/workspace/WorkSurface.test.tsx`

- [ ] **Step 1: Write failing new-Work draft tests**

Add these tests to `src/features/workspace/WorkSurface.test.tsx`:

```tsx
it("adopts new-Work uploads and sends them on the first Run", async () => {
  const client = createMockTauriClient();
  render(<WorkSurface
    client={client}
    pickProjectDirectory={async () => "D:/workspace"}
    pickAttachments={async () => ["C:/private/brief.png"]}
  />);
  await userEvent.click(screen.getByRole("button", { name: "选择项目" }));
  await userEvent.click(screen.getByRole("button", { name: "选择其他文件夹…" }));
  await userEvent.click(screen.getByRole("button", { name: "添加附件" }));
  await userEvent.click(screen.getByRole("button", { name: "上传图片" }));
  await screen.findByText("brief.png");
  await userEvent.type(screen.getByLabelText("首个任务"), "总结图片");
  await userEvent.click(screen.getByRole("button", { name: "开始 Work" }));

  expect(client.createWork).toHaveBeenCalledWith(expect.objectContaining({
    resourceDraftId: expect.stringMatching(/^[0-9a-f-]{36}$/),
  }));
  expect(client.startWork).toHaveBeenCalledWith(
    "work-1", "总结图片", [], [expect.stringMatching(/^resource-/)],
  );
  expect(await client.listWorkResources("work-1")).toHaveLength(1);
});

it("allows a new Work to start from a ready attachment without text", async () => {
  const client = createMockTauriClient();
  render(<WorkSurface
    client={client}
    pickProjectDirectory={async () => "D:/workspace"}
    pickAttachments={async () => ["C:/private/brief.png"]}
  />);
  await userEvent.click(screen.getByRole("button", { name: "选择项目" }));
  await userEvent.click(screen.getByRole("button", { name: "选择其他文件夹…" }));
  await userEvent.click(screen.getByRole("button", { name: "添加附件" }));
  await userEvent.click(screen.getByRole("button", { name: "上传图片" }));

  const start = await screen.findByRole("button", { name: "开始 Work" });
  expect(start).toBeEnabled();
  await userEvent.click(start);
  expect(client.createWork).toHaveBeenCalledWith(expect.objectContaining({
    title: "brief.png",
    goal: "Review brief.png",
  }));
});
```

Add a removal test that uploads a draft image, clicks its remove button, asserts `detachDraftResource(draftId, resourceId)` is called, and verifies Start is disabled when text is also empty.

- [ ] **Step 2: Run the new-Work tests and verify no draft ID is supplied**

Run:

```powershell
pnpm test -- src/features/workspace/WorkSurface.test.tsx -t "new Work|new-Work|draft"
```

Expected: tests fail because `NewWorkStart` has no attachment draft or image-only fallback title/goal.

- [ ] **Step 3: Add one stable draft ID and draft resource state**

In `NewWorkStart`, create the ID once per mounted new-Work composer:

```ts
const draftIdRef = useRef<string>(crypto.randomUUID());
const draftId = draftIdRef.current;
const [attachmentResults, setAttachmentResults] = useState<ResourceSummary[]>([]);
const [selectedResourceIds, setSelectedResourceIds] = useState<string[]>([]);
```

Render the same `AttachmentButton` and `AttachmentDraftList`, passing `workId={null}` and `draftId={draftId}`. On remove, optimistically remove the resource from local selection/results, call `client.detachDraftResource(draftId, resource.id)`, and restore the chip with a product error if detachment fails. Access the injected client through `useWorkStoreContext`; do not call the global Tauri client.

- [ ] **Step 4: Adopt the draft and support image-only creation**

Derive the authoritative submission values as:

```ts
const readyResources = attachmentResults.filter((resource) =>
  resource.status === "ready" && selectedResourceIds.includes(resource.id),
);
const instruction = prompt.trim();
const firstAttachment = readyResources[0];
if ((!instruction && !firstAttachment) || !rootPath || submittingRef.current) return;
const title = instruction ? workTitle(instruction) : firstAttachment.originalName.slice(0, 40);
const goal = instruction || `Review ${firstAttachment.originalName}`;
```

Create with:

```ts
await createWork({
  title,
  goal,
  rootPath,
  permissionMode: "balanced",
  resourceDraftId: draftId,
});
```

Then call `startWork(detail.summary.id, instruction, referencedFiles, readyResources.map(({ id }) => id))`. The Start button uses the same sendability rule as the existing composer. Preserve the entire draft on create failure. After a persisted start failure, leave the user in the adopted Work and do not attempt to detach its resources as a draft.

- [ ] **Step 5: Run new-Work and project-picker regressions**

Run:

```powershell
pnpm test -- src/features/workspace/WorkSurface.test.tsx
```

Expected: draft adoption, image-only creation, detachment, create-failure retention, project switching, outside dismissal, and `@` mention tests pass together.

- [ ] **Step 6: Commit new-Work draft integration**

```powershell
git add src/features/workspace/NewWorkStart.tsx src/features/workspace/WorkSurface.tsx src/features/workspace/WorkSurface.test.tsx
git commit -m "feat: adopt image drafts into new works"
```

### Task 11: Render message attachments and the Work attachment library

**Files:**
- Modify: `src/features/workspace/WorkTimeline.tsx`
- Modify: `src/features/workspace/WorkTimeline.test.tsx`
- Modify: `src/features/workspace/WorkInspector.tsx`
- Modify: `src/features/workspace/WorkSurface.tsx`
- Modify: `src/features/workspace/WorkSurface.test.tsx`
- Modify: `src/styles/workspace.css`

- [ ] **Step 1: Write failing timeline and inspector tests**

Add this test to `src/features/workspace/WorkTimeline.test.tsx`:

```tsx
it("renders message attachments under their authoritative user message", () => {
  render(<WorkTimeline
    timeline={[message({
      id: "message-1",
      content: "",
      resourceIds: ["resource-1"],
    })]}
    resources={[resource({
      id: "resource-1",
      originalName: "diagram.png",
    })]}
  />);

  expect(screen.getByText("diagram.png")).toBeInTheDocument();
  expect(screen.queryByText("C:/private/diagram.png")).not.toBeInTheDocument();
  expect(screen.getByTestId("message:message-1"))
    .toContainElement(screen.getByText("diagram.png"));
});
```

Keep the existing scroll-follow test and extend its rerendered message with `resourceIds: ["resource-1"]`; assert the final `scrollTop` still equals `scrollHeight` after the chip appears.

Add this test to `src/features/workspace/WorkSurface.test.tsx`:

```tsx
it("lists durable Work attachments in a dedicated inspector tab", async () => {
  const client = createMockTauriClient();
  client.seed(workDetail());
  client.seedResource("work-1", resource({
    id: "resource-1",
    originalName: "retained.png",
  }));
  render(<WorkSurface client={client} />);

  await screen.findByText("Work 1");
  await userEvent.click(screen.getByRole("button", { name: "打开检查器" }));
  await userEvent.click(screen.getByRole("tab", { name: "附件" }));

  expect(screen.getByText("retained.png")).toBeInTheDocument();
  expect(screen.getByText("1 个附件")).toBeInTheDocument();
});
```

- [ ] **Step 2: Run the focused tests and verify resource props/tab are absent**

Run:

```powershell
pnpm test -- src/features/workspace/WorkTimeline.test.tsx src/features/workspace/WorkSurface.test.tsx -t "attachment|附件"
```

Expected: TypeScript or assertions fail because timeline and inspector do not consume resource summaries.

- [ ] **Step 3: Bind message IDs to resource summaries in the timeline**

Change `WorkTimeline` props to:

```ts
{
  timeline: TimelineItem[];
  resources: ResourceSummary[];
  error?: AppError | null;
  onOpenDiagnostics?(): void;
}
```

Pass a `Map<string, ResourceSummary>` into each Run. For every user `MessageSummary`, resolve `message.resourceIds` in stored order and render `AttachmentChips` inside the same `.timeline-user` article after its text. Omit the `<p>` when `message.content` is empty. Set the article's `data-testid` value to the `message:${message.id}` template result. Ignore a missing summary ID instead of rendering an ID or path.

Keep `useLayoutEffect` dependent on `[timeline, resources, error]` so late thumbnail/chip state does not leave the view above the latest message.

- [ ] **Step 4: Add a dedicated inspector Attachments tab**

Change the tab tuple to:

```ts
const tabs = ["preview", "attachments", "changes", "validation", "logs"] as const;
```

Add `resources: ResourceSummary[]` to `WorkInspector` props. When `active === "attachments"`, render a count and `AttachmentChips`; when empty, render `inspector.noAttachments`. Attachments are Work-scoped, so do not filter them by current/all Run scope. The scope switch remains applicable to preview/validation/logs.

In `WorkSurface`, pass the selected Work resource list to both `WorkTimeline` and `WorkInspector`.

- [ ] **Step 5: Finish compact timeline/inspector styles**

Keep timeline attachment chips within the user bubble width and allow multiple chips to wrap. Inspector attachments use a two-column grid above 560 px and one column below it. Thumbnails remain 32 px; filenames ellipsize and expose the full original filename only through the `title` attribute. Do not show byte counts, timestamps, local paths, blob hashes, processing durations, or storage backend labels in this first UI.

- [ ] **Step 6: Run timeline, inspector, and scroll regressions**

Run:

```powershell
pnpm test -- src/features/workspace/WorkTimeline.test.tsx src/features/workspace/WorkSurface.test.tsx
```

Expected: authoritative message chips, attachment-only messages, Work library, keyboard tabs, narrow inspector behavior, and scroll-follow tests all pass.

- [ ] **Step 7: Commit attachment presentation**

```powershell
git add src/features/workspace/WorkTimeline.tsx src/features/workspace/WorkTimeline.test.tsx src/features/workspace/WorkInspector.tsx src/features/workspace/WorkSurface.tsx src/features/workspace/WorkSurface.test.tsx src/styles/workspace.css
git commit -m "feat: show work and message attachments"
```

### Task 12: Localize, validate limits, and run the complete release gate

**Files:**
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/zh-CN.json`
- Modify: `src/domain/appError.test.ts`
- Modify: `src-tauri/tests/resource_lifecycle.rs`
- Modify: `src/features/workspace/WorkSurface.test.tsx`

- [ ] **Step 1: Add failing copy, limit, and privacy tests**

Add backend tests that create 9 ready images and assert `engine_images` returns `resource_import` reason `too_many_images`; create resources whose combined size exceeds 24 MiB and assert reason `image_budget_exceeded`; request a resource linked to another Work and assert reason `resource_not_ready_or_unlinked`.

Add a frontend privacy test that imports from `C:/Users/Alice/Private/medical.png`, opens the composer, timeline, and attachment inspector, then asserts `screen.queryByText(/Alice|Private|C:\\Users/)` is null while `medical.png` appears.

Extend `src/domain/appError.test.ts` with:

```ts
it("maps resource failures without exposing backend diagnostics", () => {
  const error = normalizeAppError({
    code: "resource_import",
    message: "resource import failed",
    details: { reason: "unsupported_image", sourcePath: "C:/private/file.png" },
  });
  expect(appErrorMessageKey(error)).toBe("errors.resourceImport");
  expect(appErrorMessageValues(error)).toBeUndefined();
});
```

- [ ] **Step 2: Run the focused tests and verify copy/limits are incomplete**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml --test resource_lifecycle
pnpm test -- src/domain/appError.test.ts src/features/workspace/WorkSurface.test.tsx -t "resource|privacy|路径"
```

Expected: at least one assertion fails until all limits, error mapping, and localized labels are present.

- [ ] **Step 3: Add complete bilingual product copy**

Add the following keys in both locale files, using these Chinese values and equivalent concise English values:

```json
{
  "attachments": {
    "add": "添加附件",
    "uploadImages": "上传图片",
    "available": "Work 附件",
    "remove": "移除附件 {{name}}",
    "failed": "无法使用",
    "count": "{{count}} 个附件",
    "empty": "还没有附件",
    "importing": "正在导入附件"
  },
  "errors": {
    "resourceImport": "部分附件无法导入，请移除后重试。",
    "resourceStorage": "本地附件存储暂时不可用，请重试。",
    "resourceNotFound": "附件已失效，请重新上传。"
  }
}
```

Merge these into the existing top-level objects rather than adding a second `errors` object. Add `inspector.attachments` as “附件” and `inspector.noAttachments` as “还没有附件”. Use plural-aware English strings where i18next requires `_one`/`_other` keys.

- [ ] **Step 4: Enforce every fixed backend limit and safe failure code**

Verify the service applies the constants in the “Fixed contracts and limits” section before reading entire files into memory. Every failure exposed to the UI must be one of:

```text
unsupported_image
image_too_large
image_decode_failed
thumbnail_failed
import_interrupted
too_many_images
image_budget_exceeded
resource_not_ready_or_unlinked
```

Raw decoder errors remain available only to Rust diagnostics; they are not persisted in `failure_code` and not serialized through `AppError`.

- [ ] **Step 5: Format and run the complete backend gate**

Run:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: formatting is clean, Clippy reports no warnings, and all unit/integration tests pass including storage, resource, Work lifecycle, Pi, and model configuration suites.

- [ ] **Step 6: Run the complete frontend gate**

Run:

```powershell
pnpm typecheck
pnpm test
pnpm build
```

Expected: TypeScript, all Vitest suites, and the production Vite build pass.

- [ ] **Step 7: Perform a desktop smoke test**

Run:

```powershell
pnpm tauri dev
```

Verify in the desktop window:

1. New Work: choose a project, upload two images, remove one, start, and confirm the retained image appears on the first message and under Inspector → Attachments.
2. Existing Work: upload an image, send it without text, wait for Pi to describe it, then select the same Work attachment from the plus popover for another Run.
3. Failure isolation: select one valid image and one renamed non-image; the invalid chip shows a safe failure while the valid image can still be sent.
4. Privacy: no composer, timeline, inspector, or diagnostic product message shows the source directory, SHA-256 hash, blob key, or base64 payload.
5. Restart: close and reopen PiWork; the Work attachment list, message association, and thumbnail are restored.

Expected: all five flows behave as described and Pi receives image input without a `<referenced_files>` copy of the uploaded bytes.

- [ ] **Step 8: Commit localization and release verification changes**

```powershell
git add src/i18n/locales/en.json src/i18n/locales/zh-CN.json src/domain/appError.test.ts src-tauri/tests/resource_lifecycle.rs src/features/workspace/WorkSurface.test.tsx
git commit -m "test: verify local image attachment slice"
```

## Completion criteria and next plan

This slice is complete only when an uploaded image survives source deletion and application restart, can be associated with a new or existing Work, is recorded on the authoritative user message, is reusable from the Work attachment list, and reaches Pi through native RPC images under the fixed count/byte budgets.

The next plan is `2026-07-31-document-runtime-pack.md`. It will add PDF, DOCX, XLS/XLSX/XLSM/XLSB, CSV, text extraction, scanned-PDF offline OCR, canonical derivatives, and isolated processing on top of `ManagedResource`, `ResourceBlob`, `BlobReplica`, and `ResourceService`; it will not replace the contracts in this plan.
