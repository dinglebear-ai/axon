-- Composite indexes for the recall/review/import query patterns that filter
-- on more than one column at once (status+scope, type+status, updated_at
-- ordering, reinforcement history by memory+time). The single-column indexes
-- from 0001 don't help a compound WHERE/ORDER BY as well as a matching
-- composite index does.

CREATE INDEX IF NOT EXISTS idx_memory_records_status_scope
    ON memory_records(status, scope_kind, scope_value);
CREATE INDEX IF NOT EXISTS idx_memory_records_type_status
    ON memory_records(memory_type, status);
CREATE INDEX IF NOT EXISTS idx_memory_records_updated
    ON memory_records(updated_at);
CREATE INDEX IF NOT EXISTS idx_memory_reinforcement_memory_time
    ON memory_reinforcement(memory_id, created_at);
