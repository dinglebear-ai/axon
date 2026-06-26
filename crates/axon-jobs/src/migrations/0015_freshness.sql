CREATE TABLE IF NOT EXISTS axon_freshness_defs (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  command TEXT NOT NULL,
  target TEXT NOT NULL,
  identity_hash TEXT NOT NULL UNIQUE,
  request_json TEXT NOT NULL,
  config_json TEXT NOT NULL,
  every_seconds INTEGER NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  next_run_at INTEGER NOT NULL,
  lease_expires_at INTEGER,
  last_run_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_axon_freshness_due
  ON axon_freshness_defs(enabled, next_run_at, lease_expires_at);

CREATE INDEX IF NOT EXISTS idx_axon_freshness_target
  ON axon_freshness_defs(command, target);

CREATE TABLE IF NOT EXISTS axon_freshness_runs (
  id TEXT PRIMARY KEY,
  freshness_id TEXT NOT NULL,
  status TEXT NOT NULL,
  dispatched_job_id TEXT,
  error_text TEXT,
  result_json TEXT,
  started_at INTEGER,
  finished_at INTEGER,
  heartbeat_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(freshness_id) REFERENCES axon_freshness_defs(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_axon_freshness_runs_def_created
  ON axon_freshness_runs(freshness_id, created_at DESC);
