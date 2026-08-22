ALTER TABLE memory_connection_settings
ADD COLUMN team_id TEXT NOT NULL DEFAULT '';

-- Preserve an existing Team only when every configured workspace used the same one.
UPDATE memory_connection_settings
SET team_id = COALESCE((
    SELECT MIN(team_id)
    FROM memory_workspace_bindings
    WHERE team_id <> ''
    HAVING COUNT(DISTINCT team_id) = 1
), '')
WHERE singleton_id = 1;

ALTER TABLE memory_workspace_bindings
ADD COLUMN task_id TEXT NOT NULL DEFAULT '';

UPDATE memory_workspace_bindings
SET task_id = lower(hex(randomblob(16)))
WHERE task_id = '';

UPDATE memory_capture_outbox
SET session_id = work_id
WHERE session_id <> work_id;

CREATE UNIQUE INDEX idx_memory_workspace_bindings_task_id
ON memory_workspace_bindings(task_id)
WHERE task_id <> '';
