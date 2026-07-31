CREATE TABLE spaces (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('personal', 'team')),
    created_at TEXT NOT NULL
);

INSERT INTO spaces (id, kind, created_at)
VALUES ('local-personal', 'personal', '2026-07-31T00:00:00Z');

ALTER TABLE works
ADD COLUMN space_id TEXT NOT NULL DEFAULT 'local-personal';

CREATE TRIGGER works_space_insert
BEFORE INSERT ON works
WHEN NOT EXISTS (SELECT 1 FROM spaces WHERE id = NEW.space_id)
BEGIN
    SELECT RAISE(ABORT, 'work space not found');
END;

CREATE TRIGGER works_space_update
BEFORE UPDATE OF space_id ON works
WHEN NOT EXISTS (SELECT 1 FROM spaces WHERE id = NEW.space_id)
BEGIN
    SELECT RAISE(ABORT, 'work space not found');
END;

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
