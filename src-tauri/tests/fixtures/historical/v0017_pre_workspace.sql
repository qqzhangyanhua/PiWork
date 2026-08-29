INSERT INTO works (
    id, title, goal, root_path, permission_mode, status, created_at, updated_at
) VALUES
    (
        'work-alpha-slash',
        'Alpha slash',
        'Keep the alpha Workspace identity',
        'C:/Projects/Alpha',
        'balanced',
        'completed',
        '2026-01-01T00:00:00Z',
        '2026-01-02T00:00:00Z'
    ),
    (
        'work-alpha-backslash',
        'Alpha backslash',
        'Share the alpha Workspace identity',
        'C:\Projects\Alpha',
        'balanced',
        'idle',
        '2026-01-01T01:00:00Z',
        '2026-01-02T01:00:00Z'
    ),
    (
        'work-beta',
        'Beta',
        'Stay on a different Workspace',
        'D:/Projects/Beta',
        'ask_every_step',
        'draft',
        '2026-01-01T02:00:00Z',
        '2026-01-02T02:00:00Z'
    );

INSERT INTO work_agents (
    work_id, agent_instance_id, role_kind, status, permission_policy, joined_at, updated_at
) VALUES
    (
        'work-alpha-slash',
        'agent-instance:piwork-lead',
        'lead',
        'joined',
        'inherit_work',
        '2026-01-01T00:00:00Z',
        '2026-01-01T00:00:00Z'
    ),
    (
        'work-alpha-slash',
        'agent-instance:piwork-engineer',
        'engineer',
        'joined',
        'inherit_work',
        '2026-01-01T00:00:00Z',
        '2026-01-01T00:00:00Z'
    ),
    (
        'work-alpha-backslash',
        'agent-instance:piwork-lead',
        'lead',
        'joined',
        'inherit_work',
        '2026-01-01T01:00:00Z',
        '2026-01-01T01:00:00Z'
    ),
    (
        'work-beta',
        'agent-instance:piwork-lead',
        'lead',
        'joined',
        'inherit_work',
        '2026-01-01T02:00:00Z',
        '2026-01-01T02:00:00Z'
    );

INSERT INTO work_leads (work_id, agent_instance_id, created_at) VALUES
    ('work-alpha-slash', 'agent-instance:piwork-lead', '2026-01-01T00:00:00Z'),
    ('work-alpha-backslash', 'agent-instance:piwork-lead', '2026-01-01T01:00:00Z'),
    ('work-beta', 'agent-instance:piwork-lead', '2026-01-01T02:00:00Z');

INSERT INTO assignments (
    id, work_id, parent_assignment_id, assigned_agent_id, kind, side_effect, title,
    instruction, context_manifest_json, expected_result_schema_json,
    acceptance_criteria_json, permission_scope_json, priority, status,
    attempt_count, max_attempts, created_at, claimed_at, started_at, completed_at,
    updated_at
) VALUES
    (
        'assignment-lead-alpha',
        'work-alpha-slash',
        NULL,
        'agent-instance:piwork-lead',
        'lead',
        'unknown',
        'Lead alpha',
        'Coordinate the alpha delivery',
        '{}',
        '{}',
        '[]',
        '{}',
        10,
        'completed',
        1,
        3,
        '2026-01-01T00:05:00Z',
        '2026-01-01T00:06:00Z',
        '2026-01-01T00:07:00Z',
        '2026-01-01T00:20:00Z',
        '2026-01-01T00:20:00Z'
    ),
    (
        'assignment-member-alpha',
        'work-alpha-slash',
        'assignment-lead-alpha',
        'agent-instance:piwork-engineer',
        'member',
        'idempotent_write',
        'Write alpha notes',
        'Draft the alpha notes',
        '{}',
        '{}',
        '[]',
        '{}',
        20,
        'completed',
        1,
        3,
        '2026-01-01T00:08:00Z',
        '2026-01-01T00:09:00Z',
        '2026-01-01T00:10:00Z',
        '2026-01-01T00:15:00Z',
        '2026-01-01T00:15:00Z'
    );

INSERT INTO runs (
    id, work_id, engine_kind, model_label, status, created_at, updated_at,
    started_at, completed_at, assignment_id, agent_instance_id, attempt_number
) VALUES (
    'run-alpha',
    'work-alpha-slash',
    'pi',
    'test-model',
    'completed',
    '2026-01-01T00:07:00Z',
    '2026-01-01T00:20:00Z',
    '2026-01-01T00:07:00Z',
    '2026-01-01T00:20:00Z',
    'assignment-lead-alpha',
    'agent-instance:piwork-lead',
    1
);

INSERT INTO messages (
    id, work_id, run_id, role, content, created_at, assignment_id
) VALUES (
    'message-alpha',
    'work-alpha-slash',
    'run-alpha',
    'user',
    'Ship the alpha report',
    '2026-01-01T00:07:30Z',
    'assignment-lead-alpha'
);

INSERT INTO events (
    id, work_id, run_id, sequence, version, occurred_at, payload, assignment_id
) VALUES (
    'event-alpha',
    'work-alpha-slash',
    'run-alpha',
    1,
    1,
    '2026-01-01T00:20:00Z',
    '{"type":"agent_end"}',
    'assignment-lead-alpha'
);

INSERT INTO assignment_results (
    id, assignment_id, run_id, event_id, author_agent_id, envelope_json,
    schema_version, repair_attempt, status, created_at
) VALUES (
    'result-lead-alpha',
    'assignment-lead-alpha',
    'run-alpha',
    'event-alpha',
    'agent-instance:piwork-lead',
    '{"summary":"Alpha delivered"}',
    1,
    0,
    'valid',
    '2026-01-01T00:20:00Z'
);

INSERT INTO work_deliveries (
    id, work_id, lead_assignment_id, summary, limitations_json, status,
    created_at, validated_at
) VALUES (
    'delivery-alpha',
    'work-alpha-slash',
    'assignment-lead-alpha',
    'Alpha report',
    '[]',
    'valid',
    '2026-01-01T00:21:00Z',
    '2026-01-01T00:21:00Z'
);

INSERT INTO delivery_artifacts (
    id, delivery_id, workspace_path, size_bytes, admission_status, created_at
) VALUES (
    'artifact-alpha',
    'delivery-alpha',
    'deliverables/report.md',
    12,
    'admitted',
    '2026-01-01T00:21:00Z'
);

INSERT INTO delivery_validations (
    id, delivery_id, claim, source_event_id, verification_status, created_at
) VALUES (
    'validation-alpha',
    'delivery-alpha',
    'The alpha report was delivered',
    'event-alpha',
    'verified',
    '2026-01-01T00:21:00Z'
);

INSERT INTO run_capability_snapshots (
    id, schema_version, run_id, work_id, assignment_id, agent_instance_id,
    role_kind, permission_mode, workspace_root, expert_pack_ids_json,
    host_tool_ids_json, extension_tool_ids_json, created_at
) VALUES (
    'snapshot-alpha',
    1,
    'run-alpha',
    'work-alpha-slash',
    'assignment-lead-alpha',
    'agent-instance:piwork-lead',
    'lead',
    'balanced',
    'C:/Projects/Alpha',
    '[]',
    '[]',
    '[]',
    '2026-01-01T00:07:00Z'
);

INSERT INTO resource_blobs (
    id, plaintext_sha256, size, created_at
) VALUES (
    'blob-alpha',
    'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
    12,
    '2026-01-01T00:00:00Z'
);

INSERT INTO managed_resources (
    id, space_id, blob_id, original_name, media_type, size, origin, status,
    created_at, updated_at
) VALUES (
    'resource-alpha',
    'local-personal',
    'blob-alpha',
    'notes.md',
    'text/markdown',
    12,
    'user_upload',
    'ready',
    '2026-01-01T00:00:00Z',
    '2026-01-01T00:00:00Z'
);

INSERT INTO resource_links (
    id, resource_id, work_id, role, created_at
) VALUES (
    'link-alpha',
    'resource-alpha',
    'work-alpha-slash',
    'attached',
    '2026-01-01T00:00:00Z'
);

INSERT INTO assignment_event_outbox (
    event_id, assignment_id, status, attempt_count, delivered_at, created_at,
    updated_at
) VALUES (
    'event-alpha',
    'assignment-lead-alpha',
    'delivered',
    1,
    '2026-01-01T00:20:00Z',
    '2026-01-01T00:20:00Z',
    '2026-01-01T00:20:00Z'
);

INSERT INTO memory_workspace_bindings (
    root_path, team_id, enabled, capture_enabled, recall_enabled, updated_at,
    task_id
) VALUES
    (
        'C:/Projects/Alpha',
        'team-codo',
        1,
        1,
        1,
        '2026-01-01T00:00:00Z',
        'task-alpha-slash'
    ),
    (
        'C:\Projects\Alpha',
        'team-codo',
        1,
        1,
        1,
        '2026-01-01T00:00:00Z',
        'task-alpha-backslash'
    ),
    (
        'D:/Projects/Beta',
        'team-codo',
        1,
        1,
        1,
        '2026-01-01T00:00:00Z',
        'task-beta'
    );

INSERT INTO extension_packages (
    package_id, display_name, description, publisher, trust_tier, source_kind,
    latest_version, lifecycle_status, builtin, created_at, updated_at
) VALUES (
    'pkg-web-access',
    'Web Access',
    'Fetch URLs',
    'piwork',
    'builtin',
    'bundled',
    '1.0.0',
    'installed',
    1,
    '2026-01-01T00:00:00Z',
    '2026-01-01T00:00:00Z'
);

INSERT INTO extension_work_policies (
    package_id, work_id, enabled, tool_allowlist_json, updated_at
) VALUES
    (
        'pkg-web-access',
        'work-alpha-slash',
        1,
        '["read"]',
        '2026-01-01T00:00:00Z'
    ),
    (
        'pkg-web-access',
        'work-alpha-backslash',
        1,
        '["read"]',
        '2026-01-01T00:00:00Z'
    );
