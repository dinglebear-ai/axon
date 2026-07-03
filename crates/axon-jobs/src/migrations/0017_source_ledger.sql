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
-- Unified source ledger (contract schema, owned by axon-ledger).
--
-- Per docs/pipeline-unification: storage-contract.md + schema-contract.md, the
-- runtime uses ONE SQLite database with ONE migration runner. `sources` and its
-- lifecycle tables are owned by `axon-ledger` but must be co-located with the
-- `jobs` table so `jobs.source_id` can FK to `sources(source_id)` (SQLite FKs
-- are single-file). The runtime `SqliteLedgerStore` binds to this shared pool
-- via `from_pool` (no separate migration). The legacy `axon_source_*` tables
-- above stay only until `embed`/`crawl` are removed (they still back
-- axon-source-ledger for crawl_sync/embed during the cutover).
--
-- NOTE (follow-up): this schema is duplicated from
-- crates/axon-ledger/src/migrations/0001_ledger_lifecycle.sql (which still runs
-- for axon-ledger's standalone in-memory tests). Unify into one owned migration
-- source once the composed cross-crate migration runner exists. Keep the two in
-- sync until then.
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS sources (
  source_id TEXT PRIMARY KEY NOT NULL,
  committed_generation TEXT,
  summary_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_sources_canonical_uri
  ON sources(json_extract(summary_json, '$.canonical_uri'));

CREATE TABLE IF NOT EXISTS source_generations (
  source_id TEXT NOT NULL,
  generation TEXT NOT NULL,
  sequence INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL,
  publish_state TEXT NOT NULL,
  generation_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  published_at TEXT,
  PRIMARY KEY (source_id, generation),
  FOREIGN KEY (source_id) REFERENCES sources(source_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_source_generations_source_status_created
  ON source_generations(source_id, status, created_at);

CREATE UNIQUE INDEX IF NOT EXISTS idx_source_generations_source_sequence
  ON source_generations(source_id, sequence);

CREATE TABLE IF NOT EXISTS source_manifests (
  source_id TEXT NOT NULL,
  generation TEXT NOT NULL,
  manifest_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY (source_id, generation),
  FOREIGN KEY (source_id, generation)
    REFERENCES source_generations(source_id, generation)
    ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS source_items (
  source_id TEXT NOT NULL,
  source_item_key TEXT NOT NULL,
  generation TEXT NOT NULL,
  item_canonical_uri TEXT NOT NULL,
  content_hash TEXT,
  version TEXT,
  mtime TEXT,
  item_json TEXT NOT NULL,
  PRIMARY KEY (source_id, generation, source_item_key),
  FOREIGN KEY (source_id, generation)
    REFERENCES source_manifests(source_id, generation)
    ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_source_items_key_generation
  ON source_items(source_id, source_item_key, generation);

CREATE INDEX IF NOT EXISTS idx_source_items_canonical_uri
  ON source_items(source_id, item_canonical_uri);

CREATE TABLE IF NOT EXISTS document_status (
  document_id TEXT PRIMARY KEY NOT NULL,
  source_id TEXT NOT NULL,
  source_item_key TEXT NOT NULL,
  generation TEXT NOT NULL,
  status TEXT NOT NULL,
  status_json TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (source_id) REFERENCES sources(source_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_document_status_source_generation_item
  ON document_status(source_id, generation, source_item_key);

CREATE TABLE IF NOT EXISTS cleanup_debt (
  debt_id TEXT PRIMARY KEY NOT NULL,
  job_id TEXT NOT NULL,
  source_id TEXT NOT NULL,
  generation TEXT,
  generation_key TEXT NOT NULL,
  kind TEXT NOT NULL,
  selector_hash TEXT NOT NULL,
  status TEXT NOT NULL,
  debt_json TEXT NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  next_retry_at TEXT,
  completed_at TEXT,
  FOREIGN KEY (source_id) REFERENCES sources(source_id) ON DELETE CASCADE,
  UNIQUE (source_id, generation_key, kind, selector_hash)
);

CREATE INDEX IF NOT EXISTS idx_cleanup_debt_status_retry
  ON cleanup_debt(status, next_retry_at);

CREATE TABLE IF NOT EXISTS leases (
  lease_id TEXT PRIMARY KEY NOT NULL,
  lease_key TEXT NOT NULL UNIQUE,
  owner_id TEXT NOT NULL,
  acquired_at TEXT NOT NULL,
  expires_at TEXT NOT NULL,
  heartbeat_at TEXT NOT NULL,
  job_id TEXT,
  lease_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_leases_key_expires
  ON leases(lease_key, expires_at);
