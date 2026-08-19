-- Durable provider identity cache shared by short-lived CLI processes and
-- long-lived server runtimes. Only verified identities are written; fallback
-- identities remain process-local so a transient provider outage cannot poison
-- future reads.
CREATE TABLE provider_identity_cache (
    cache_key TEXT PRIMARY KEY NOT NULL,
    provider_kind TEXT NOT NULL CHECK (provider_kind IN ('embedding')),
    provider_id TEXT NOT NULL,
    model TEXT NOT NULL CHECK (length(model) > 0),
    dimensions INTEGER NOT NULL CHECK (dimensions > 0),
    updated_at INTEGER NOT NULL
);

CREATE INDEX provider_identity_cache_provider_idx
    ON provider_identity_cache (provider_kind, provider_id, updated_at);
