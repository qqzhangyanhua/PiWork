DROP TRIGGER works_workspace_autofill;

CREATE TRIGGER works_workspace_autofill
AFTER INSERT ON works
WHEN NEW.workspace_id IS NULL
BEGIN
    INSERT OR IGNORE INTO workspaces (
        id, canonical_root_path, path_identity, default_permission_mode,
        lifecycle_status, created_at, updated_at
    ) VALUES (
        'workspace:auto:' || lower(hex(randomblob(16))),
        CASE WHEN trim(NEW.root_path) = '' THEN '.' ELSE NEW.root_path END,
        lower(replace(CASE WHEN trim(NEW.root_path) = '' THEN '.' ELSE NEW.root_path END, char(92), '/')),
        NEW.permission_mode,
        'active',
        NEW.created_at,
        NEW.updated_at
    );
    UPDATE works
    SET workspace_id = (
        SELECT id FROM workspaces
        WHERE path_identity = lower(replace(
            CASE WHEN trim(NEW.root_path) = '' THEN '.' ELSE NEW.root_path END,
            char(92), '/'
        ))
    )
    WHERE id = NEW.id;
END;
