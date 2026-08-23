-- Support amortized TTL maintenance without scanning the cache table.
CREATE INDEX embedding_vector_cache_created_at_idx
    ON embedding_vector_cache (created_at, cache_key);

-- Exact O(1) cardinality avoids COUNT/OFFSET scans during bounded LRU passes.
CREATE TABLE embedding_vector_cache_state (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    entry_count INTEGER NOT NULL CHECK (entry_count >= 0)
);

INSERT INTO embedding_vector_cache_state (singleton, entry_count)
SELECT 1, COUNT(*) FROM embedding_vector_cache;

CREATE TRIGGER embedding_vector_cache_count_insert
AFTER INSERT ON embedding_vector_cache
BEGIN
    UPDATE embedding_vector_cache_state
    SET entry_count = entry_count + 1
    WHERE singleton = 1;
END;

CREATE TRIGGER embedding_vector_cache_count_delete
AFTER DELETE ON embedding_vector_cache
BEGIN
    UPDATE embedding_vector_cache_state
    SET entry_count = entry_count - 1
    WHERE singleton = 1;
END;
