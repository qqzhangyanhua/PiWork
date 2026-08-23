CREATE TABLE workspaces (
    id TEXT PRIMARY KEY NOT NULL,
    canonical_root_path TEXT NOT NULL CHECK (length(trim(canonical_root_path)) > 0),
    path_identity TEXT NOT NULL UNIQUE CHECK (length(trim(path_identity)) > 0),
    default_permission_mode TEXT NOT NULL CHECK (
        default_permission_mode IN ('ask_every_step', 'balanced', 'auto_execute')
    ),
    lifecycle_status TEXT NOT NULL CHECK (lifecycle_status IN ('active', 'unavailable')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

ALTER TABLE works
ADD COLUMN workspace_id TEXT REFERENCES workspaces(id) ON DELETE RESTRICT;

INSERT INTO workspaces (
    id, canonical_root_path, path_identity, default_permission_mode,
    lifecycle_status, created_at, updated_at
)
SELECT
    'workspace:legacy:' || MIN(id),
    MIN(root_path),
    lower(replace(root_path, char(92), '/')),
    MIN(permission_mode),
    'active',
    MIN(created_at),
    MAX(updated_at)
FROM works
GROUP BY lower(replace(root_path, char(92), '/'));

UPDATE works
SET workspace_id = (
    SELECT workspaces.id
    FROM workspaces
    WHERE workspaces.path_identity = lower(replace(works.root_path, char(92), '/'))
);

CREATE INDEX idx_works_workspace ON works(workspace_id, updated_at DESC);

-- Compatibility during expand/switch: legacy writers may omit workspace_id,
-- but every committed Work is repaired inside the same SQLite statement.
CREATE TRIGGER works_workspace_autofill
AFTER INSERT ON works
WHEN NEW.workspace_id IS NULL
BEGIN
    INSERT OR IGNORE INTO workspaces (
        id, canonical_root_path, path_identity, default_permission_mode,
        lifecycle_status, created_at, updated_at
    ) VALUES (
        'workspace:auto:' || lower(hex(randomblob(16))),
        NEW.root_path,
        lower(replace(NEW.root_path, char(92), '/')),
        NEW.permission_mode,
        'active',
        NEW.created_at,
        NEW.updated_at
    );
    UPDATE works
    SET workspace_id = (
        SELECT id FROM workspaces
        WHERE path_identity = lower(replace(NEW.root_path, char(92), '/'))
    )
    WHERE id = NEW.id;
END;
