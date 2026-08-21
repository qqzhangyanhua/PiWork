-- Remove the presentation-only capability catalog while preserving the
-- executable system capability packs used by the built-in team.
DELETE FROM capability_packs WHERE status = 'catalog_only';

CREATE TABLE extension_packages (
    package_id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    description TEXT NOT NULL,
    publisher TEXT NOT NULL,
    trust_tier TEXT NOT NULL CHECK (trust_tier IN ('builtin', 'verified', 'community')),
    source_kind TEXT NOT NULL CHECK (source_kind IN ('bundled', 'npm')),
    installed_version TEXT,
    latest_version TEXT NOT NULL,
    integrity TEXT,
    entry_path TEXT,
    install_root TEXT,
    lifecycle_status TEXT NOT NULL CHECK (
        lifecycle_status IN ('available', 'installed', 'disabled', 'revoked', 'pending_removal')
    ),
    manifest_json TEXT NOT NULL DEFAULT '{}',
    permissions_json TEXT NOT NULL DEFAULT '{}',
    previous_versions_json TEXT NOT NULL DEFAULT '[]',
    revoked_reason TEXT,
    builtin INTEGER NOT NULL DEFAULT 0 CHECK (builtin IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE extension_agent_grants (
    package_id TEXT NOT NULL REFERENCES extension_packages(package_id) ON DELETE CASCADE,
    agent_instance_id TEXT NOT NULL REFERENCES agent_instances(id) ON DELETE CASCADE,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    tool_allowlist_json TEXT NOT NULL DEFAULT '[]',
    updated_at TEXT NOT NULL,
    PRIMARY KEY (package_id, agent_instance_id)
);

CREATE TABLE extension_work_policies (
    package_id TEXT NOT NULL REFERENCES extension_packages(package_id) ON DELETE CASCADE,
    work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    tool_allowlist_json TEXT NOT NULL DEFAULT '[]',
    updated_at TEXT NOT NULL,
    PRIMARY KEY (package_id, work_id)
);

CREATE TABLE web_access_settings (
    singleton_id INTEGER PRIMARY KEY CHECK (singleton_id = 1),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    url_fetch_enabled INTEGER NOT NULL CHECK (url_fetch_enabled IN (0, 1)),
    default_provider TEXT,
    fallback_provider TEXT,
    updated_at TEXT NOT NULL
);

CREATE TABLE web_search_providers (
    provider_id TEXT PRIMARY KEY,
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    endpoint TEXT,
    settings_json TEXT NOT NULL DEFAULT '{}',
    updated_at TEXT NOT NULL
);

CREATE TABLE connector_connections (
    id TEXT PRIMARY KEY,
    connector_kind TEXT NOT NULL CHECK (connector_kind = 'email'),
    preset TEXT NOT NULL,
    display_name TEXT NOT NULL,
    email_address TEXT NOT NULL,
    username TEXT NOT NULL,
    imap_host TEXT NOT NULL,
    imap_port INTEGER NOT NULL CHECK (imap_port BETWEEN 1 AND 65535),
    smtp_host TEXT NOT NULL,
    smtp_port INTEGER NOT NULL CHECK (smtp_port BETWEEN 1 AND 65535),
    tls_mode TEXT NOT NULL CHECK (tls_mode = 'tls'),
    enabled INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    poll_interval_minutes INTEGER NOT NULL CHECK (poll_interval_minutes IN (1, 2, 5, 15)),
    health_status TEXT NOT NULL CHECK (health_status IN ('untested', 'healthy', 'degraded', 'error')),
    last_error_code TEXT,
    last_checked_at TEXT,
    inbox_uid_validity INTEGER,
    inbox_last_uid INTEGER,
    last_polled_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE connector_work_grants (
    connection_id TEXT NOT NULL REFERENCES connector_connections(id) ON DELETE CASCADE,
    work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    permissions_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (connection_id, work_id)
);

CREATE TABLE email_metadata (
    connection_id TEXT NOT NULL REFERENCES connector_connections(id) ON DELETE CASCADE,
    folder TEXT NOT NULL,
    uid INTEGER NOT NULL,
    message_id TEXT,
    sender_name TEXT,
    sender_address TEXT NOT NULL,
    subject TEXT NOT NULL,
    sent_at TEXT,
    received_at TEXT,
    flags_json TEXT NOT NULL DEFAULT '[]',
    attachment_count INTEGER NOT NULL DEFAULT 0,
    size_bytes INTEGER,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (connection_id, folder, uid)
);

CREATE INDEX idx_email_metadata_received
    ON email_metadata(connection_id, folder, received_at DESC);

CREATE TABLE connector_pending_actions (
    id TEXT PRIMARY KEY,
    connection_id TEXT NOT NULL REFERENCES connector_connections(id) ON DELETE CASCADE,
    work_id TEXT NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    action_type TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    payload_hash TEXT NOT NULL,
    preview_json TEXT NOT NULL,
    idempotency_key TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL CHECK (status IN ('pending', 'approved', 'denied', 'cancelled', 'expired', 'executed', 'failed')),
    expires_at TEXT NOT NULL,
    resolved_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_connector_pending_actions_work
    ON connector_pending_actions(work_id, status, created_at DESC);

CREATE TABLE app_notifications (
    id TEXT PRIMARY KEY,
    category TEXT NOT NULL CHECK (category IN ('mail', 'approval', 'plugin', 'connector')),
    connection_id TEXT REFERENCES connector_connections(id) ON DELETE CASCADE,
    work_id TEXT REFERENCES works(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    summary TEXT NOT NULL,
    action_json TEXT NOT NULL DEFAULT '{}',
    read_at TEXT,
    cleared_at TEXT,
    expires_at TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_app_notifications_inbox
    ON app_notifications(cleared_at, created_at DESC);

CREATE TABLE connector_audit_log (
    id TEXT PRIMARY KEY,
    connection_id TEXT REFERENCES connector_connections(id) ON DELETE SET NULL,
    work_id TEXT REFERENCES works(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    event_kind TEXT NOT NULL,
    outcome TEXT NOT NULL,
    details_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    expires_at TEXT
);

CREATE INDEX idx_connector_audit_expiry ON connector_audit_log(expires_at);

INSERT INTO extension_packages (
    package_id, display_name, description, publisher, trust_tier, source_kind,
    installed_version, latest_version, integrity, entry_path, install_root,
    lifecycle_status, manifest_json, permissions_json, previous_versions_json,
    builtin, created_at, updated_at
) VALUES (
    'pi-web-access',
    'Pi Web Access',
    'Web search and content retrieval for Pi agents.',
    'nicobailon',
    'builtin',
    'bundled',
    '0.24.0',
    '0.24.0',
    'sha512-BVosva1tGDhHveaGpFnc++YS5+pzmWVzJ/5B+1xBavkRjAgyDvMpA1EfVL+GYIviAxXKck9JyRGVzo4ASV7snA==',
    'index.ts',
    NULL,
    'installed',
    '{"tools":["web_search","fetch_content","get_search_content","source_check"],"piVersion":">=0.37.3"}',
    '{"network":true,"filesystem":"runtime-cache","subprocess":true,"data":["search queries","requested URLs"]}',
    '[]',
    1,
    CURRENT_TIMESTAMP,
    CURRENT_TIMESTAMP
);

INSERT INTO web_access_settings (
    singleton_id, enabled, url_fetch_enabled, default_provider, fallback_provider, updated_at
) VALUES (1, 0, 1, NULL, NULL, CURRENT_TIMESTAMP);

INSERT INTO web_search_providers (provider_id, enabled, endpoint, settings_json, updated_at) VALUES
    ('exa', 0, NULL, '{}', CURRENT_TIMESTAMP),
    ('brave', 0, NULL, '{}', CURRENT_TIMESTAMP),
    ('tavily', 0, NULL, '{}', CURRENT_TIMESTAMP),
    ('bocha', 0, NULL, '{}', CURRENT_TIMESTAMP),
    ('jina', 0, NULL, '{}', CURRENT_TIMESTAMP),
    ('firecrawl', 0, NULL, '{}', CURRENT_TIMESTAMP),
    ('openai', 0, NULL, '{}', CURRENT_TIMESTAMP),
    ('searxng', 0, NULL, '{}', CURRENT_TIMESTAMP);
