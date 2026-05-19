CREATE TABLE IF NOT EXISTS axon_watch_defs (
    id               TEXT PRIMARY KEY,
    name             TEXT NOT NULL UNIQUE,
    task_type        TEXT NOT NULL,
    task_payload     TEXT NOT NULL DEFAULT '{}',
    every_seconds    INTEGER NOT NULL,
    enabled          INTEGER NOT NULL DEFAULT 1,
    next_run_at      INTEGER NOT NULL,
    lease_expires_at INTEGER,
    last_run_at      INTEGER,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_watch_defs_due ON axon_watch_defs(next_run_at);

CREATE TABLE IF NOT EXISTS axon_watch_runs (
    id                TEXT PRIMARY KEY,
    watch_id          TEXT NOT NULL,
    status            TEXT NOT NULL,
    dispatched_job_id TEXT,
    error_text        TEXT,
    result_json       TEXT,
    started_at        INTEGER,
    finished_at       INTEGER,
    created_at        INTEGER NOT NULL,
    updated_at        INTEGER NOT NULL,
    FOREIGN KEY (watch_id) REFERENCES axon_watch_defs(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_watch_runs_watch_id ON axon_watch_runs(watch_id, created_at DESC);

CREATE TABLE IF NOT EXISTS axon_watch_run_artifacts (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    watch_run_id TEXT NOT NULL,
    kind         TEXT NOT NULL,
    path         TEXT,
    payload      TEXT,
    created_at   INTEGER NOT NULL,
    FOREIGN KEY (watch_run_id) REFERENCES axon_watch_runs(id) ON DELETE CASCADE
);
