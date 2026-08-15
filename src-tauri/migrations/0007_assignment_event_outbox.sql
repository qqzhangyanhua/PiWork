CREATE TABLE assignment_event_outbox (
    ordinal INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL UNIQUE REFERENCES events(id) ON DELETE CASCADE,
    assignment_id TEXT NOT NULL REFERENCES assignments(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (
        status IN ('pending', 'delivering', 'delivered')
    ),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    last_attempt_at TEXT,
    last_error TEXT CHECK (last_error IS NULL OR length(last_error) <= 512),
    lease_token TEXT,
    lease_expires_at TEXT,
    delivered_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (updated_at >= created_at),
    CHECK (
        (status = 'pending' AND lease_token IS NULL AND lease_expires_at IS NULL AND delivered_at IS NULL)
        OR (
            status = 'delivering'
            AND lease_token IS NOT NULL
            AND lease_expires_at IS NOT NULL
            AND delivered_at IS NULL
        )
        OR (
            status = 'delivered'
            AND lease_token IS NULL
            AND lease_expires_at IS NULL
            AND delivered_at IS NOT NULL
        )
    )
);

CREATE INDEX idx_assignment_event_outbox_pending
ON assignment_event_outbox(status, ordinal)
WHERE status <> 'delivered';
