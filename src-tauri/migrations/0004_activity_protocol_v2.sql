ALTER TABLE events ADD COLUMN turn_id TEXT;
ALTER TABLE events ADD COLUMN session_id TEXT;
ALTER TABLE events ADD COLUMN agent_id TEXT;
ALTER TABLE events ADD COLUMN assignment_id TEXT;
ALTER TABLE events ADD COLUMN causation_id TEXT;
ALTER TABLE events ADD COLUMN correlation_id TEXT;

CREATE INDEX idx_events_work_turn_sequence
    ON events(work_id, turn_id, sequence);
CREATE INDEX idx_events_assignment_sequence
    ON events(assignment_id, sequence)
    WHERE assignment_id IS NOT NULL;
