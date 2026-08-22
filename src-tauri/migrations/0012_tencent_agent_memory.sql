CREATE TABLE memory_connection_settings (
    singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    endpoint TEXT NOT NULL,
    service_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    request_timeout_ms INTEGER NOT NULL CHECK (request_timeout_ms BETWEEN 100 AND 30000),
    recall_timeout_ms INTEGER NOT NULL CHECK (recall_timeout_ms BETWEEN 100 AND 10000),
    max_recall_items INTEGER NOT NULL CHECK (max_recall_items BETWEEN 1 AND 50),
    max_recall_chars INTEGER NOT NULL CHECK (max_recall_chars BETWEEN 256 AND 32000),
    capture_enabled INTEGER NOT NULL CHECK (capture_enabled IN (0, 1)),
    recall_enabled INTEGER NOT NULL CHECK (recall_enabled IN (0, 1)),
    updated_at TEXT NOT NULL
);

INSERT INTO memory_connection_settings (
    singleton_id, enabled, endpoint, service_id, user_id,
    request_timeout_ms, recall_timeout_ms, max_recall_items, max_recall_chars,
    capture_enabled, recall_enabled, updated_at
) VALUES (1, 0, '', '', 'codo-local-user', 5000, 1500, 8, 6000, 1, 1, CURRENT_TIMESTAMP);

CREATE TABLE memory_workspace_bindings (
    root_path TEXT PRIMARY KEY,
    team_id TEXT NOT NULL,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    capture_enabled INTEGER NOT NULL CHECK (capture_enabled IN (0, 1)),
    recall_enabled INTEGER NOT NULL CHECK (recall_enabled IN (0, 1)),
    updated_at TEXT NOT NULL
);

CREATE TABLE memory_capture_outbox (
    id TEXT PRIMARY KEY,
    root_path TEXT NOT NULL,
    work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    assignment_id TEXT NOT NULL REFERENCES assignments(id) ON DELETE CASCADE,
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    user_message TEXT NOT NULL,
    assistant_message TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'sent')),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    next_attempt_at TEXT,
    last_error_code TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    sent_at TEXT,
    UNIQUE (run_id)
);

CREATE INDEX idx_memory_capture_outbox_pending
    ON memory_capture_outbox(status, next_attempt_at, created_at)
    WHERE status = 'pending';
