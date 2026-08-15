CREATE TABLE assignments (
    id TEXT PRIMARY KEY NOT NULL,
    work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    parent_assignment_id TEXT,
    created_by_agent_id TEXT,
    assigned_agent_id TEXT NOT NULL,
    capability_pack_id TEXT REFERENCES capability_packs(id) ON DELETE RESTRICT,
    kind TEXT NOT NULL CHECK (kind IN ('lead', 'member')),
    side_effect TEXT NOT NULL CHECK (
        side_effect IN ('read_only', 'idempotent_write', 'non_idempotent_write', 'unknown')
    ),
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    instruction TEXT NOT NULL CHECK (length(trim(instruction)) > 0),
    context_manifest_json TEXT NOT NULL CHECK (
        json_valid(context_manifest_json)
        AND CASE WHEN json_valid(context_manifest_json)
            THEN json_type(context_manifest_json) = 'object' ELSE 0 END
    ),
    expected_result_schema_json TEXT NOT NULL CHECK (
        json_valid(expected_result_schema_json)
        AND CASE WHEN json_valid(expected_result_schema_json)
            THEN json_type(expected_result_schema_json) = 'object' ELSE 0 END
    ),
    acceptance_criteria_json TEXT NOT NULL CHECK (
        json_valid(acceptance_criteria_json)
        AND CASE WHEN json_valid(acceptance_criteria_json)
            THEN json_type(acceptance_criteria_json) = 'array' ELSE 0 END
    ),
    permission_scope_json TEXT NOT NULL CHECK (
        json_valid(permission_scope_json)
        AND CASE WHEN json_valid(permission_scope_json)
            THEN json_type(permission_scope_json) = 'object' ELSE 0 END
    ),
    priority INTEGER NOT NULL CHECK (priority BETWEEN 0 AND 4294967295),
    status TEXT NOT NULL CHECK (
        status IN (
            'queued', 'claimed', 'running', 'waiting', 'completed', 'failed',
            'cancelled', 'interrupted', 'dead_letter',
            'recovery_confirmation_required'
        )
    ),
    attempt_count INTEGER NOT NULL CHECK (
        attempt_count BETWEEN 0 AND 4294967295
    ),
    max_attempts INTEGER NOT NULL CHECK (
        max_attempts BETWEEN 1 AND 4294967295
        AND attempt_count <= max_attempts
    ),
    not_before TEXT,
    result_summary TEXT,
    last_error TEXT,
    next_attempt_at TEXT,
    recovery_reason TEXT,
    runtime_owner_id TEXT,
    created_at TEXT NOT NULL,
    claimed_at TEXT,
    started_at TEXT,
    completed_at TEXT,
    updated_at TEXT NOT NULL,
    CHECK (parent_assignment_id IS NULL OR parent_assignment_id <> id),
    CHECK (updated_at >= created_at),
    CHECK (not_before IS NULL OR not_before >= created_at),
    CHECK (claimed_at IS NULL OR claimed_at >= created_at),
    CHECK (started_at IS NULL OR started_at >= COALESCE(claimed_at, created_at)),
    CHECK (
        completed_at IS NULL
        OR completed_at >= COALESCE(started_at, claimed_at, created_at)
    ),
    CHECK (next_attempt_at IS NULL OR next_attempt_at >= created_at),
    UNIQUE (work_id, id),
    UNIQUE (work_id, id, assigned_agent_id),
    FOREIGN KEY (work_id, parent_assignment_id)
        REFERENCES assignments(work_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (work_id, created_by_agent_id)
        REFERENCES work_agents(work_id, agent_instance_id) ON DELETE RESTRICT,
    FOREIGN KEY (work_id, assigned_agent_id)
        REFERENCES work_agents(work_id, agent_instance_id) ON DELETE RESTRICT
);

CREATE UNIQUE INDEX idx_assignments_one_inflight_per_work
ON assignments(work_id)
WHERE status IN ('claimed', 'running');

CREATE INDEX idx_assignments_schedulable
ON assignments(status, not_before, created_at, id);

CREATE INDEX idx_assignments_assigned_agent
ON assignments(work_id, assigned_agent_id, status);

INSERT INTO assignments (
    id, work_id, assigned_agent_id, kind, side_effect, title, instruction,
    context_manifest_json, expected_result_schema_json, acceptance_criteria_json,
    permission_scope_json, priority, status, attempt_count, max_attempts,
    created_at, completed_at, updated_at
)
SELECT
    legacy.id,
    legacy.work_id,
    work_leads.agent_instance_id,
    'lead',
    'unknown',
    'Legacy assignment',
    'Preserved from the pre-assignment event journal during schema migration.',
    '{"legacy":true}',
    '{}',
    '[]',
    '{"mode":"inherit_work","legacy":true}',
    0,
    'interrupted',
    0,
    1,
    legacy.created_at,
    legacy.updated_at,
    legacy.updated_at
FROM (
    SELECT
        assignment_id AS id,
        MIN(work_id) AS work_id,
        MIN(occurred_at) AS created_at,
        MAX(occurred_at) AS updated_at
    FROM events
    WHERE assignment_id IS NOT NULL
    GROUP BY assignment_id
) AS legacy
INNER JOIN work_leads ON work_leads.work_id = legacy.work_id;

CREATE TABLE assignment_dependencies (
    assignment_id TEXT NOT NULL REFERENCES assignments(id) ON DELETE CASCADE,
    depends_on_assignment_id TEXT NOT NULL REFERENCES assignments(id) ON DELETE CASCADE,
    PRIMARY KEY (assignment_id, depends_on_assignment_id),
    CHECK (assignment_id <> depends_on_assignment_id)
);

CREATE INDEX idx_assignment_dependencies_reverse
ON assignment_dependencies(depends_on_assignment_id, assignment_id);

CREATE TABLE agent_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    agent_instance_id TEXT NOT NULL,
    engine_kind TEXT NOT NULL CHECK (length(trim(engine_kind)) > 0),
    engine_reference TEXT NOT NULL CHECK (length(engine_reference) > 0),
    generation INTEGER NOT NULL CHECK (generation BETWEEN 1 AND 4294967295),
    current_assignment_id TEXT,
    last_successful_turn_id TEXT,
    status TEXT NOT NULL CHECK (status IN ('ready', 'running', 'invalidated')),
    rotation_reason TEXT,
    runtime_owner_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    invalidated_at TEXT,
    CHECK (updated_at >= created_at),
    CHECK (invalidated_at IS NULL OR invalidated_at >= created_at),
    CHECK (
        (status = 'invalidated' AND invalidated_at IS NOT NULL)
        OR (status <> 'invalidated' AND invalidated_at IS NULL)
    ),
    UNIQUE (agent_instance_id, work_id, engine_kind, generation),
    FOREIGN KEY (work_id, agent_instance_id)
        REFERENCES work_agents(work_id, agent_instance_id) ON DELETE RESTRICT,
    FOREIGN KEY (work_id, current_assignment_id, agent_instance_id)
        REFERENCES assignments(work_id, id, assigned_agent_id) ON DELETE RESTRICT
);

CREATE INDEX idx_agent_sessions_work_status
ON agent_sessions(work_id, status, updated_at);

CREATE TEMP TABLE migration_0006_runs AS SELECT * FROM runs;
CREATE TEMP TABLE migration_0006_messages AS SELECT * FROM messages;
CREATE TEMP TABLE migration_0006_events AS SELECT * FROM events;
CREATE TEMP TABLE migration_0006_resource_links AS SELECT * FROM resource_links;

DROP TABLE resource_links;
DROP TABLE events;
DROP TABLE messages;
DROP TABLE runs;

CREATE TABLE runs (
    id TEXT PRIMARY KEY NOT NULL,
    work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    engine_kind TEXT NOT NULL,
    engine_session_id TEXT,
    model_label TEXT NOT NULL,
    status TEXT NOT NULL CHECK (
        status IN (
            'queued', 'running', 'waiting', 'completed', 'failed', 'stopped',
            'interrupted'
        )
    ),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    started_at TEXT,
    completed_at TEXT,
    assignment_id TEXT,
    agent_instance_id TEXT,
    attempt_number INTEGER,
    CHECK (
        (assignment_id IS NULL AND agent_instance_id IS NULL AND attempt_number IS NULL)
        OR (
            assignment_id IS NOT NULL
            AND agent_instance_id IS NOT NULL
            AND attempt_number BETWEEN 1 AND 4294967295
        )
    ),
    UNIQUE (work_id, id),
    FOREIGN KEY (work_id, assignment_id, agent_instance_id)
        REFERENCES assignments(work_id, id, assigned_agent_id) ON DELETE RESTRICT
);

CREATE INDEX idx_runs_work_id ON runs(work_id);

CREATE UNIQUE INDEX idx_runs_assignment_attempt
ON runs(assignment_id, attempt_number)
WHERE assignment_id IS NOT NULL;

INSERT INTO runs (
    id, work_id, engine_kind, engine_session_id, model_label, status,
    created_at, updated_at, started_at, completed_at,
    assignment_id, agent_instance_id, attempt_number
)
SELECT
    id, work_id, engine_kind, engine_session_id, model_label, status,
    created_at, updated_at, started_at, completed_at,
    NULL, NULL, NULL
FROM migration_0006_runs;

CREATE TABLE messages (
    id TEXT PRIMARY KEY NOT NULL,
    work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    run_id TEXT,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (work_id, run_id) REFERENCES runs(work_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_messages_work_id ON messages(work_id);
CREATE UNIQUE INDEX idx_messages_work_id_id ON messages(work_id, id);

INSERT INTO messages (id, work_id, run_id, role, content, created_at)
SELECT id, work_id, run_id, role, content, created_at
FROM migration_0006_messages;

CREATE TABLE events (
    id TEXT PRIMARY KEY NOT NULL,
    work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    run_id TEXT,
    sequence INTEGER NOT NULL CHECK (sequence BETWEEN 1 AND 4294967295),
    version INTEGER NOT NULL CHECK (version > 0),
    occurred_at TEXT NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    turn_id TEXT,
    session_id TEXT,
    agent_id TEXT,
    assignment_id TEXT,
    causation_id TEXT,
    correlation_id TEXT,
    CHECK (run_id IS NOT NULL OR assignment_id IS NOT NULL),
    FOREIGN KEY (work_id, run_id) REFERENCES runs(work_id, id) ON DELETE CASCADE,
    FOREIGN KEY (work_id, assignment_id)
        REFERENCES assignments(work_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_events_work_turn_sequence
ON events(work_id, turn_id, sequence);

CREATE UNIQUE INDEX idx_events_run_sequence
ON events(run_id, sequence)
WHERE run_id IS NOT NULL;

CREATE UNIQUE INDEX idx_events_assignment_sequence
ON events(assignment_id, sequence)
WHERE run_id IS NULL AND assignment_id IS NOT NULL;

INSERT INTO events (
    id, work_id, run_id, sequence, version, occurred_at, payload,
    turn_id, session_id, agent_id, assignment_id, causation_id, correlation_id
)
SELECT
    id, work_id, run_id, sequence, version, occurred_at, payload,
    turn_id, session_id, agent_id, assignment_id, causation_id, correlation_id
FROM migration_0006_events;

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

INSERT INTO resource_links (
    id, resource_id, work_id, draft_id, message_id, run_id, role, created_at
)
SELECT id, resource_id, work_id, draft_id, message_id, run_id, role, created_at
FROM migration_0006_resource_links;

DROP TABLE migration_0006_resource_links;
DROP TABLE migration_0006_events;
DROP TABLE migration_0006_messages;
DROP TABLE migration_0006_runs;
