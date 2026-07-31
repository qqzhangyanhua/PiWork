CREATE TABLE resource_derivatives (
    resource_id TEXT NOT NULL REFERENCES managed_resources(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('canonical_markdown')),
    state TEXT NOT NULL CHECK (state IN ('processing', 'ready', 'failed')),
    cache_key TEXT,
    extractor TEXT NOT NULL,
    extractor_version TEXT NOT NULL,
    content_sha256 TEXT,
    content_chars INTEGER CHECK (content_chars IS NULL OR content_chars >= 0),
    used_ocr INTEGER NOT NULL DEFAULT 0 CHECK (used_ocr IN (0, 1)),
    failure_code TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (resource_id, kind),
    CHECK ((state = 'ready' AND cache_key IS NOT NULL AND content_sha256 IS NOT NULL
            AND content_chars IS NOT NULL AND failure_code IS NULL)
        OR state <> 'ready'),
    CHECK ((state = 'failed' AND failure_code IS NOT NULL) OR state <> 'failed')
);

CREATE INDEX idx_resource_derivatives_state
ON resource_derivatives(state, updated_at);
