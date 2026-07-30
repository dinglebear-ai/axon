-- Durable publication watermark and finalizer lease for source-derived graph
-- mutations. Graph ownership is independent of the ledger schema, so source
-- identity remains an opaque stable string rather than a cross-crate FK.
CREATE TABLE IF NOT EXISTS graph_publication_state (
    source_id            TEXT PRIMARY KEY NOT NULL,
    committed_epoch      INTEGER NOT NULL DEFAULT 0,
    previous_epoch       INTEGER,
    finalizer_lease_id   TEXT,
    finalizer_owner_id   TEXT,
    finalizer_expires_at TEXT,
    updated_at           TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_graph_publication_state_finalizer_expiry
    ON graph_publication_state (finalizer_expires_at);
