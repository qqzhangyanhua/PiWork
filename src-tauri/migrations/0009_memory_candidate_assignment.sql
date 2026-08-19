-- Track the Assignment that proposed each Memory candidate so a later
-- confirmation/rejection can be journaled against a valid event identity
-- (the events table requires either run_id or assignment_id).

ALTER TABLE memory_candidates ADD COLUMN source_assignment_id TEXT;

CREATE INDEX idx_memory_candidates_assignment
ON memory_candidates(source_assignment_id);
