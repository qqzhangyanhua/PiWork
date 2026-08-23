CREATE TABLE capability_executions (
    id TEXT PRIMARY KEY NOT NULL,
    decision_id TEXT NOT NULL UNIQUE REFERENCES capability_decisions(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (status IN ('started', 'succeeded', 'failed')),
    started_at TEXT NOT NULL,
    completed_at TEXT,
    CHECK (
        (status = 'started' AND completed_at IS NULL)
        OR (status != 'started' AND completed_at IS NOT NULL)
    )
);

CREATE INDEX idx_capability_executions_unconfirmed
ON capability_executions(status, started_at);
