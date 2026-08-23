-- Content-addressed dense embedding cache shared by source generations and
-- short-lived CLI processes. Cache keys contain the provider authority,
-- provider/model identity, instruction, content kind, and input text digest;
-- raw source text is never persisted here.
CREATE TABLE embedding_vector_cache (
    cache_key TEXT PRIMARY KEY NOT NULL CHECK (length(cache_key) = 71),
    provider_id TEXT NOT NULL CHECK (length(provider_id) > 0),
    model TEXT NOT NULL CHECK (length(model) > 0),
    dimensions INTEGER NOT NULL CHECK (dimensions > 0),
    vector BLOB NOT NULL CHECK (length(vector) > 0),
    created_at INTEGER NOT NULL,
    last_used_at INTEGER NOT NULL,
    hit_count INTEGER NOT NULL DEFAULT 0 CHECK (hit_count >= 0)
);

CREATE INDEX embedding_vector_cache_lru_idx
    ON embedding_vector_cache (last_used_at, cache_key);
