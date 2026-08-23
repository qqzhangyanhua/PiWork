CREATE TABLE work_deliveries (
    id TEXT PRIMARY KEY NOT NULL,
    work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    lead_assignment_id TEXT NOT NULL REFERENCES assignments(id) ON DELETE RESTRICT,
    summary TEXT NOT NULL CHECK (length(trim(summary)) > 0),
    limitations_json TEXT NOT NULL CHECK (
        json_valid(limitations_json)
        AND CASE WHEN json_valid(limitations_json)
            THEN json_type(limitations_json) = 'array' ELSE 0 END
    ),
    status TEXT NOT NULL CHECK (status IN ('pending', 'valid', 'invalid', 'superseded')),
    created_at TEXT NOT NULL,
    validated_at TEXT,
    superseded_at TEXT,
    CHECK (validated_at IS NULL OR validated_at >= created_at),
    CHECK (superseded_at IS NULL OR superseded_at >= created_at)
);

CREATE UNIQUE INDEX idx_work_deliveries_one_valid
ON work_deliveries(work_id)
WHERE status = 'valid';

CREATE INDEX idx_work_deliveries_assignment
ON work_deliveries(lead_assignment_id, created_at);

CREATE TABLE delivery_artifacts (
    id TEXT PRIMARY KEY NOT NULL,
    delivery_id TEXT NOT NULL REFERENCES work_deliveries(id) ON DELETE CASCADE,
    workspace_path TEXT NOT NULL,
    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
    admission_status TEXT NOT NULL CHECK (admission_status = 'admitted'),
    created_at TEXT NOT NULL,
    UNIQUE (delivery_id, workspace_path)
);

CREATE TABLE delivery_validations (
    id TEXT PRIMARY KEY NOT NULL,
    delivery_id TEXT NOT NULL REFERENCES work_deliveries(id) ON DELETE CASCADE,
    claim TEXT NOT NULL CHECK (length(trim(claim)) > 0),
    source_event_id TEXT REFERENCES events(id) ON DELETE RESTRICT,
    verification_status TEXT NOT NULL CHECK (
        verification_status IN ('verified', 'unverified', 'failed')
    ),
    created_at TEXT NOT NULL
);
