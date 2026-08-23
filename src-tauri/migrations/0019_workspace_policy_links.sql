CREATE TABLE workspace_memory_bindings (
    workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
    task_id TEXT NOT NULL UNIQUE,
    source_root_path TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

INSERT INTO workspace_memory_bindings (
    workspace_id, task_id, source_root_path, created_at, updated_at
)
SELECT
    workspaces.id,
    MIN(memory_workspace_bindings.task_id),
    MIN(memory_workspace_bindings.root_path),
    MIN(memory_workspace_bindings.updated_at),
    MAX(memory_workspace_bindings.updated_at)
FROM memory_workspace_bindings
INNER JOIN workspaces
    ON workspaces.path_identity = lower(replace(memory_workspace_bindings.root_path, char(92), '/'))
WHERE memory_workspace_bindings.task_id <> ''
GROUP BY workspaces.id;

CREATE TABLE extension_workspace_policies (
    package_id TEXT NOT NULL REFERENCES extension_packages(package_id) ON DELETE CASCADE,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    tool_allowlist_json TEXT NOT NULL CHECK (json_valid(tool_allowlist_json)),
    has_conflict INTEGER NOT NULL CHECK (has_conflict IN (0, 1)),
    updated_at TEXT NOT NULL,
    PRIMARY KEY (package_id, workspace_id)
);

INSERT INTO extension_workspace_policies (
    package_id, workspace_id, enabled, tool_allowlist_json, has_conflict, updated_at
)
SELECT
    policies.package_id,
    works.workspace_id,
    MIN(policies.enabled),
    CASE WHEN MIN(policies.tool_allowlist_json) = MAX(policies.tool_allowlist_json)
        THEN MIN(policies.tool_allowlist_json) ELSE '[]' END,
    CASE WHEN MIN(policies.enabled) = MAX(policies.enabled)
              AND MIN(policies.tool_allowlist_json) = MAX(policies.tool_allowlist_json)
        THEN 0 ELSE 1 END,
    MAX(policies.updated_at)
FROM extension_work_policies AS policies
INNER JOIN works ON works.id = policies.work_id
GROUP BY policies.package_id, works.workspace_id;

CREATE TABLE connector_workspace_grants (
    connection_id TEXT NOT NULL REFERENCES connector_connections(id) ON DELETE CASCADE,
    workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    permissions_json TEXT NOT NULL CHECK (json_valid(permissions_json)),
    has_conflict INTEGER NOT NULL CHECK (has_conflict IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (connection_id, workspace_id)
);

INSERT INTO connector_workspace_grants (
    connection_id, workspace_id, permissions_json, has_conflict, created_at, updated_at
)
SELECT
    grants.connection_id,
    works.workspace_id,
    CASE WHEN MIN(grants.permissions_json) = MAX(grants.permissions_json)
        THEN MIN(grants.permissions_json) ELSE '{}' END,
    CASE WHEN MIN(grants.permissions_json) = MAX(grants.permissions_json) THEN 0 ELSE 1 END,
    MIN(grants.created_at),
    MAX(grants.updated_at)
FROM connector_work_grants AS grants
INNER JOIN works ON works.id = grants.work_id
GROUP BY grants.connection_id, works.workspace_id;
