-- Contract the logical invariant without rebuilding the highly-referenced
-- works table. The expand compatibility trigger fills omitted values; these
-- guards prevent committed NULL or dangling identities.
UPDATE works
SET workspace_id = (
    SELECT workspaces.id
    FROM workspaces
    WHERE workspaces.path_identity = lower(replace(works.root_path, char(92), '/'))
)
WHERE workspace_id IS NULL;

CREATE TRIGGER works_workspace_required_on_update
BEFORE UPDATE OF workspace_id ON works
WHEN NEW.workspace_id IS NULL
BEGIN
    SELECT RAISE(ABORT, 'works.workspace_id is required');
END;

CREATE TRIGGER workspaces_identity_immutable
BEFORE UPDATE OF path_identity ON workspaces
WHEN NEW.path_identity <> OLD.path_identity
BEGIN
    SELECT RAISE(ABORT, 'workspace path identity is immutable');
END;

CREATE UNIQUE INDEX idx_workspaces_canonical_identity
ON workspaces(path_identity);
