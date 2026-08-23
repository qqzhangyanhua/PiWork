CREATE TABLE capability_decisions (
    id TEXT PRIMARY KEY NOT NULL,
    snapshot_id TEXT NOT NULL REFERENCES run_capability_snapshots(id) ON DELETE CASCADE,
    operation_json TEXT NOT NULL CHECK (json_valid(operation_json)),
    decision TEXT NOT NULL CHECK (decision IN ('allow', 'deny', 'ask')),
    denial_reason TEXT,
    approval_request_id TEXT UNIQUE,
    created_at TEXT NOT NULL,
    CHECK (
        (decision = 'deny' AND denial_reason IS NOT NULL AND approval_request_id IS NULL)
        OR (decision = 'ask' AND denial_reason IS NULL AND approval_request_id IS NOT NULL)
        OR (decision = 'allow' AND denial_reason IS NULL AND approval_request_id IS NULL)
    )
);

CREATE INDEX idx_capability_decisions_snapshot_created
ON capability_decisions(snapshot_id, created_at);

CREATE TABLE capability_approval_requests (
    id TEXT PRIMARY KEY NOT NULL,
    decision_id TEXT NOT NULL UNIQUE REFERENCES capability_decisions(id) ON DELETE CASCADE,
    snapshot_id TEXT NOT NULL REFERENCES run_capability_snapshots(id) ON DELETE CASCADE,
    operation_json TEXT NOT NULL CHECK (json_valid(operation_json)),
    status TEXT NOT NULL DEFAULT 'pending' CHECK (
        status IN ('pending', 'approved', 'denied', 'cancelled', 'expired')
    ),
    created_at TEXT NOT NULL,
    resolved_at TEXT,
    CHECK (
        (status = 'pending' AND resolved_at IS NULL)
        OR (status != 'pending' AND resolved_at IS NOT NULL)
    )
);

CREATE INDEX idx_capability_approval_requests_pending
ON capability_approval_requests(snapshot_id, status, created_at);
