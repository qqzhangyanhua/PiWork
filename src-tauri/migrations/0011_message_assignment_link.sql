-- User messages are accepted before the Scheduler creates a Run. Preserve the
-- Assignment identity so they remain visible and can be attached atomically to
-- the first attempt.
ALTER TABLE messages ADD COLUMN assignment_id TEXT REFERENCES assignments(id) ON DELETE CASCADE;

CREATE INDEX idx_messages_assignment_id ON messages(assignment_id);

-- Repair messages created by the initial Assignment runtime. A Lead
-- Assignment and its user message are written back-to-back, so nearest creation
-- time is the only durable identity available in pre-v11 databases.
UPDATE messages
SET assignment_id = (
    SELECT MIN(candidate.id)
    FROM assignments candidate
    WHERE candidate.work_id = messages.work_id
      AND candidate.kind = 'lead'
      AND ABS(julianday(candidate.created_at) - julianday(messages.created_at)) = (
          SELECT MIN(ABS(julianday(comparison.created_at) - julianday(messages.created_at)))
          FROM assignments comparison
          WHERE comparison.work_id = messages.work_id
            AND comparison.kind = 'lead'
      )
)
WHERE messages.run_id IS NULL;

UPDATE messages
SET run_id = (
    SELECT runs.id
    FROM runs
    WHERE runs.assignment_id = messages.assignment_id
    ORDER BY runs.attempt_number, runs.created_at, runs.id
    LIMIT 1
)
WHERE messages.run_id IS NULL
  AND messages.assignment_id IS NOT NULL;
