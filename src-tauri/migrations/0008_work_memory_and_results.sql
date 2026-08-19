-- Work Ledger cache, confirmed Agent memory, and durable Result/Memory-candidate
-- records for the lead/expert collaboration loop. The Work Ledger remains a
-- projection of append-only events (the source sequence cursor enables rebuild);
-- only confirmed Memory candidates may reach agent_memory.

CREATE TABLE agent_memory (
    id TEXT PRIMARY KEY NOT NULL,
    agent_instance_id TEXT NOT NULL REFERENCES agent_instances(id) ON DELETE CASCADE,
    source_work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    source_event_id TEXT,
    author_agent_id TEXT NOT NULL CHECK (length(trim(author_agent_id)) > 0),
    content TEXT NOT NULL CHECK (length(trim(content)) > 0),
    reason TEXT NOT NULL,
    version INTEGER NOT NULL CHECK (version > 0),
    created_at TEXT NOT NULL
);

CREATE INDEX idx_agent_memory_agent_version
ON agent_memory(agent_instance_id, version DESC);

CREATE TABLE work_memory (
    id TEXT PRIMARY KEY NOT NULL,
    work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    revision INTEGER NOT NULL CHECK (revision > 0),
    ledger_json TEXT NOT NULL CHECK (json_valid(ledger_json)),
    source_sequence INTEGER NOT NULL CHECK (source_sequence >= 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (updated_at >= created_at),
    UNIQUE (work_id, revision)
);

CREATE INDEX idx_work_memory_work_revision
ON work_memory(work_id, revision DESC);

CREATE TABLE assignment_results (
    id TEXT PRIMARY KEY NOT NULL,
    assignment_id TEXT NOT NULL REFERENCES assignments(id) ON DELETE CASCADE,
    run_id TEXT,
    event_id TEXT,
    author_agent_id TEXT NOT NULL CHECK (length(trim(author_agent_id)) > 0),
    envelope_json TEXT NOT NULL CHECK (json_valid(envelope_json)),
    schema_version INTEGER NOT NULL CHECK (schema_version > 0),
    repair_attempt INTEGER NOT NULL DEFAULT 0 CHECK (repair_attempt >= 0),
    status TEXT NOT NULL CHECK (status IN ('valid', 'repair_requested', 'rejected')),
    created_at TEXT NOT NULL,
    UNIQUE (assignment_id, repair_attempt)
);

CREATE INDEX idx_assignment_results_assignment
ON assignment_results(assignment_id, repair_attempt DESC);

CREATE TABLE memory_candidates (
    id TEXT PRIMARY KEY NOT NULL,
    source_work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    source_event_id TEXT,
    author_agent_id TEXT NOT NULL CHECK (length(trim(author_agent_id)) > 0),
    content TEXT NOT NULL CHECK (length(trim(content)) > 0),
    reason TEXT NOT NULL,
    version INTEGER NOT NULL CHECK (version > 0),
    status TEXT NOT NULL DEFAULT 'proposed' CHECK (
        status IN ('proposed', 'confirmed', 'rejected')
    ),
    created_at TEXT NOT NULL,
    resolved_at TEXT,
    resolved_by TEXT,
    CHECK (
        (status = 'proposed' AND resolved_at IS NULL AND resolved_by IS NULL)
        OR (
            status IN ('confirmed', 'rejected')
            AND resolved_at IS NOT NULL
            AND resolved_by IS NOT NULL
        )
    )
);

CREATE INDEX idx_memory_candidates_work_status
ON memory_candidates(source_work_id, status);
