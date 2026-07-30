-- Durable source-publication watermark. The source lease serializes the
-- finalizer; this row records the last fully committed epoch so a replacement
-- worker can distinguish an unfinished vector write from a published epoch.
CREATE TABLE IF NOT EXISTS source_publication_state (
  source_id TEXT PRIMARY KEY NOT NULL,
  committed_epoch INTEGER NOT NULL DEFAULT 0,
  previous_epoch INTEGER,
  finalizer_lease_id TEXT,
  finalizer_owner_id TEXT,
  finalizer_expires_at TEXT,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (source_id) REFERENCES sources(source_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_source_publication_state_finalizer_expiry
  ON source_publication_state(finalizer_expires_at);
