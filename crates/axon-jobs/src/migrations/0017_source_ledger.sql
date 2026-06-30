CREATE TABLE IF NOT EXISTS axon_source_sources (
  source_id TEXT PRIMARY KEY,
  source_kind TEXT NOT NULL,
  collection TEXT NOT NULL,
  index_version INTEGER NOT NULL,
  committed_generation INTEGER NOT NULL DEFAULT 0,
  max_generation INTEGER NOT NULL DEFAULT 0,
  lease_owner TEXT,
  lease_expires_at_ms INTEGER NOT NULL DEFAULT 0,
  backoff_until_ms INTEGER NOT NULL DEFAULT 0,
  backoff_dependency TEXT,
  last_error TEXT,
  last_checked_at_ms INTEGER NOT NULL DEFAULT 0,
  last_success_at_ms INTEGER NOT NULL DEFAULT 0,
  updated_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS axon_source_manifest_items (
  source_id TEXT NOT NULL,
  item_key TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  size_bytes INTEGER NOT NULL,
  indexed_generation INTEGER NOT NULL,
  pending INTEGER NOT NULL DEFAULT 0,
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY (source_id, item_key),
  FOREIGN KEY (source_id) REFERENCES axon_source_sources(source_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS axon_source_cleanup_debt (
  source_id TEXT NOT NULL,
  generation INTEGER NOT NULL,
  item_key TEXT NOT NULL,
  selector_json TEXT NOT NULL,
  retry_count INTEGER NOT NULL DEFAULT 0,
  last_error TEXT,
  updated_at_ms INTEGER NOT NULL,
  PRIMARY KEY (source_id, generation, item_key),
  FOREIGN KEY (source_id) REFERENCES axon_source_sources(source_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_axon_source_sources_kind ON axon_source_sources(source_kind);
CREATE INDEX IF NOT EXISTS idx_axon_source_sources_backoff ON axon_source_sources(backoff_until_ms);
CREATE INDEX IF NOT EXISTS idx_axon_source_cleanup_source ON axon_source_cleanup_debt(source_id);
