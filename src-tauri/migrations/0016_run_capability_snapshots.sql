CREATE TABLE run_capability_snapshots (
    id TEXT PRIMARY KEY NOT NULL,
    schema_version INTEGER NOT NULL CHECK (schema_version = 1),
    run_id TEXT NOT NULL UNIQUE REFERENCES runs(id) ON DELETE CASCADE,
    work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    assignment_id TEXT NOT NULL REFERENCES assignments(id) ON DELETE RESTRICT,
    agent_instance_id TEXT NOT NULL REFERENCES agent_instances(id) ON DELETE RESTRICT,
    role_kind TEXT NOT NULL CHECK (
        role_kind IN ('lead', 'researcher', 'engineer', 'reviewer')
    ),
    permission_mode TEXT NOT NULL CHECK (
        permission_mode IN ('ask_every_step', 'balanced', 'auto_execute')
    ),
    workspace_root TEXT NOT NULL CHECK (length(trim(workspace_root)) > 0),
    expert_pack_ids_json TEXT NOT NULL CHECK (
        json_valid(expert_pack_ids_json)
        AND CASE WHEN json_valid(expert_pack_ids_json)
            THEN json_type(expert_pack_ids_json) = 'array' ELSE 0 END
    ),
    host_tool_ids_json TEXT NOT NULL CHECK (
        json_valid(host_tool_ids_json)
        AND CASE WHEN json_valid(host_tool_ids_json)
            THEN json_type(host_tool_ids_json) = 'array' ELSE 0 END
    ),
    extension_tool_ids_json TEXT NOT NULL CHECK (
        json_valid(extension_tool_ids_json)
        AND CASE WHEN json_valid(extension_tool_ids_json)
            THEN json_type(extension_tool_ids_json) = 'array' ELSE 0 END
    ),
    created_at TEXT NOT NULL,
    expires_at TEXT,
    revoked_at TEXT,
    CHECK (expires_at IS NULL OR expires_at > created_at),
    CHECK (revoked_at IS NULL OR revoked_at >= created_at),
    UNIQUE (work_id, run_id),
    FOREIGN KEY (work_id, run_id) REFERENCES runs(work_id, id) ON DELETE CASCADE,
    FOREIGN KEY (work_id, assignment_id, agent_instance_id)
        REFERENCES assignments(work_id, id, assigned_agent_id) ON DELETE RESTRICT
);

CREATE INDEX idx_run_capability_snapshots_active
ON run_capability_snapshots(run_id, revoked_at, expires_at);
