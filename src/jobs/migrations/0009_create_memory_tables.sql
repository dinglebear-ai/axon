-- Add persistent agent memory graph tables.
--
-- Version-skew note: this migration is applied to the shared jobs SQLite DB.
-- Older axon binaries that do not know migration 0009 will fail to open this
-- DB with VersionMissing. Deploy host CLI and container image together.

CREATE TABLE IF NOT EXISTS axon_memory_nodes (
  id           TEXT PRIMARY KEY,
  type         TEXT NOT NULL CHECK (type IN ('decision','fact','preference','task','bug')),
  title        TEXT NOT NULL,
  project      TEXT,
  repo         TEXT,
  file_path    TEXT,
  status       TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','superseded','archived')),
  confidence   REAL NOT NULL DEFAULT 1.0 CHECK (confidence BETWEEN 0.0 AND 1.0),
  source       TEXT NOT NULL DEFAULT 'manual',
  access_count INTEGER NOT NULL DEFAULT 0,
  created_at   INTEGER NOT NULL,
  updated_at   INTEGER NOT NULL,
  last_seen_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS axon_memory_edges (
  id         TEXT PRIMARY KEY,
  source_id  TEXT NOT NULL,
  target_id  TEXT NOT NULL,
  type       TEXT NOT NULL CHECK (type IN ('relates_to','supersedes')),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(source_id) REFERENCES axon_memory_nodes(id) ON DELETE CASCADE,
  FOREIGN KEY(target_id) REFERENCES axon_memory_nodes(id) ON DELETE CASCADE,
  UNIQUE(source_id, target_id, type),
  CHECK (source_id <> target_id)
);

CREATE INDEX IF NOT EXISTS idx_memory_nodes_active
  ON axon_memory_nodes(project, type)
  WHERE status='active';
CREATE INDEX IF NOT EXISTS idx_memory_edges_source ON axon_memory_edges(source_id);
CREATE INDEX IF NOT EXISTS idx_memory_edges_target ON axon_memory_edges(target_id);
