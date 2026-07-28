PRAGMA foreign_keys = ON;

CREATE TABLE works (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    goal TEXT NOT NULL,
    root_path TEXT NOT NULL,
    permission_mode TEXT NOT NULL,
    status TEXT NOT NULL CHECK (
        status IN (
            'draft', 'queued', 'running', 'waiting', 'idle', 'completed',
            'failed', 'stopped', 'interrupted', 'archived'
        )
    ),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE runs (
    id TEXT PRIMARY KEY NOT NULL,
    work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
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
    UNIQUE (work_id, id)
);

CREATE INDEX idx_runs_work_id ON runs(work_id);

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

CREATE TABLE events (
    id TEXT PRIMARY KEY NOT NULL,
    work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    run_id TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK (sequence >= 0),
    version INTEGER NOT NULL CHECK (version > 0),
    occurred_at TEXT NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    FOREIGN KEY (work_id, run_id) REFERENCES runs(work_id, id) ON DELETE CASCADE,
    UNIQUE (run_id, sequence)
);

CREATE INDEX idx_events_run_sequence ON events(run_id, sequence);

CREATE TABLE settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL CHECK (json_valid(value)),
    updated_at TEXT NOT NULL
);
