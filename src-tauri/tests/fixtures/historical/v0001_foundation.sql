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

INSERT INTO runs (
    id, work_id, engine_kind, model_label, status, created_at, updated_at,
    started_at, completed_at
) VALUES
    (
        'run-alpha',
        'work-alpha-slash',
        'pi',
        'test-model',
        'completed',
        '2026-01-01T00:10:00Z',
        '2026-01-01T00:20:00Z',
        '2026-01-01T00:10:00Z',
        '2026-01-01T00:20:00Z'
    ),
    (
        'run-beta',
        'work-beta',
        'pi',
        'test-model',
        'queued',
        '2026-01-01T02:10:00Z',
        '2026-01-01T02:10:00Z',
        NULL,
        NULL
    );

INSERT INTO messages (
    id, work_id, run_id, role, content, created_at
) VALUES (
    'message-alpha',
    'work-alpha-slash',
    'run-alpha',
    'user',
    'Ship the alpha report',
    '2026-01-01T00:11:00Z'
);

INSERT INTO events (
    id, work_id, run_id, sequence, version, occurred_at, payload
) VALUES (
    'event-alpha',
    'work-alpha-slash',
    'run-alpha',
    1,
    1,
    '2026-01-01T00:20:00Z',
    '{"type":"agent_end"}'
);

INSERT INTO settings (key, value, updated_at)
VALUES ('theme', '{"mode":"system"}', '2026-01-01T00:00:00Z');
