-- Source-request-backed watch store (WS-B / audit C4-04, issue #298).
--
-- `axon_source_watches` / `axon_source_watch_runs` back `SqliteWatchStore`,
-- the concrete `crate::boundary::WatchStore` implementation used by the CLI's
-- `watch get|update|pause|resume|delete` verbs. This is deliberately a NEW
-- table pair, not a rewrite of the legacy `axon_watch_defs`/`axon_watch_runs`
-- tables (migration 0002): the legacy tables back the still-live
-- `axon watch create|list|history|exec` task_type/task_payload model and its
-- existing scheduler (`crates/axon-jobs/src/workers/watch_scheduler.rs`),
-- which this slice must not disturb. `axon_source_watches` stores a
-- `SourceRequest`-shaped watch (source string + `WatchSchedule` +
-- `AdapterOptions`), matching the `axon_api::source::{WatchRequest,
-- WatchResult}` contract types. `watch_id` is the store's own TEXT primary
-- key (see `WatchId`, a plain string newtype) rather than reusing legacy
-- `axon_watch_defs.id`.
CREATE TABLE IF NOT EXISTS axon_source_watches (
    watch_id        TEXT PRIMARY KEY,
    source          TEXT NOT NULL,
    source_id       TEXT NOT NULL,
    canonical_uri   TEXT NOT NULL,
    adapter_name    TEXT NOT NULL,
    adapter_version TEXT NOT NULL,
    scope           TEXT NOT NULL,
    embed           INTEGER NOT NULL DEFAULT 1,
    options_json    TEXT NOT NULL DEFAULT '{}',
    collection      TEXT,
    enabled         INTEGER NOT NULL DEFAULT 1,
    every_seconds   INTEGER NOT NULL,
    cron            TEXT,
    timezone        TEXT,
    next_run_at     INTEGER NOT NULL,
    last_job_id     TEXT,
    last_status     TEXT,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_source_watches_source_id ON axon_source_watches(source_id);
CREATE INDEX IF NOT EXISTS idx_source_watches_due ON axon_source_watches(next_run_at);

CREATE TABLE IF NOT EXISTS axon_source_watch_runs (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    watch_id   TEXT NOT NULL,
    job_id     TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (watch_id) REFERENCES axon_source_watches(watch_id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_source_watch_runs_watch_id ON axon_source_watch_runs(watch_id, created_at DESC, id DESC);
