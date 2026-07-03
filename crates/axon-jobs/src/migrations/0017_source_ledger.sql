CREATE TABLE IF NOT EXISTS axon_source_sources (
  source_id TEXT PRIMARY KEY,
  source_kind TEXT NOT NULL CHECK (source_kind IN ('local_code', 'crawl', 'git', 'feed', 'session', 'media')),
  collection TEXT NOT NULL,
  index_version INTEGER NOT NULL CHECK (index_version > 0),
  committed_generation INTEGER NOT NULL DEFAULT 0 CHECK (committed_generation >= 0),
  max_generation INTEGER NOT NULL DEFAULT 0 CHECK (max_generation >= 0),
  lease_owner TEXT,
  lease_expires_at_ms INTEGER NOT NULL DEFAULT 0 CHECK (lease_expires_at_ms >= 0),
  backoff_until_ms INTEGER NOT NULL DEFAULT 0 CHECK (backoff_until_ms >= 0),
  backoff_dependency TEXT,
  last_error TEXT,
  last_checked_at_ms INTEGER NOT NULL DEFAULT 0 CHECK (last_checked_at_ms >= 0),
  last_success_at_ms INTEGER NOT NULL DEFAULT 0 CHECK (last_success_at_ms >= 0),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
  CHECK (max_generation >= committed_generation)
);

CREATE TABLE IF NOT EXISTS axon_source_manifest_items (
  source_id TEXT NOT NULL,
  item_key TEXT NOT NULL,
  content_hash TEXT NOT NULL,
  size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
  indexed_generation INTEGER NOT NULL CHECK (indexed_generation > 0),
  pending INTEGER NOT NULL DEFAULT 0 CHECK (pending IN (0, 1)),
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
  PRIMARY KEY (source_id, indexed_generation, item_key),
  FOREIGN KEY (source_id) REFERENCES axon_source_sources(source_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_axon_source_manifest_committed
  ON axon_source_manifest_items(source_id, pending, item_key);

CREATE TABLE IF NOT EXISTS axon_source_cleanup_debt (
  source_id TEXT NOT NULL,
  generation INTEGER NOT NULL CHECK (generation > 0),
  item_key TEXT NOT NULL,
  selector_json TEXT NOT NULL,
  retry_count INTEGER NOT NULL DEFAULT 0 CHECK (retry_count >= 0),
  last_error TEXT,
  updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
  PRIMARY KEY (source_id, generation, item_key),
  FOREIGN KEY (source_id) REFERENCES axon_source_sources(source_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_axon_source_sources_kind ON axon_source_sources(source_kind);
CREATE INDEX IF NOT EXISTS idx_axon_source_sources_backoff ON axon_source_sources(backoff_until_ms);
CREATE INDEX IF NOT EXISTS idx_axon_source_cleanup_source ON axon_source_cleanup_debt(source_id);

-- ---------------------------------------------------------------------------
-- The seven unified-ledger contract tables (`sources`, `source_generations`,
-- `source_manifests`, `source_items`, `document_status`, `cleanup_debt`,
-- `leases`) were previously duplicated here as a byte-for-byte copy of
-- crates/axon-ledger/src/migrations/0001_ledger_lifecycle.sql, because no runner
-- executed axon-ledger's own migration against the shared pool.
--
-- That split-brain is now eliminated: the composed cross-crate migration runner
-- (crates/axon-jobs/src/migrations.rs) applies axon-ledger's migration FIRST
-- against the SAME pool, so `axon-ledger` is the SOLE creator of the contract
-- tables (schema-contract.md). `jobs.source_id` (migration 0018) still FKs
-- `sources(source_id)` because the ledger set runs before the jobs set.
--
-- The legacy `axon_source_*` tables above are intentionally KEPT — they still
-- back axon-source-ledger (crawl_sync/embed) during the cutover and are retired
-- separately (bead 5mlrh), not here.
-- ---------------------------------------------------------------------------
