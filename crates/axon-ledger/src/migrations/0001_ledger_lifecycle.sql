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
  FOREIGN KEY (source_id) REFERENCES sources(source_id) ON DELETE CASCADE
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
