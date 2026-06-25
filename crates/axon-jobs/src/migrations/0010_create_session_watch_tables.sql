CREATE TABLE IF NOT EXISTS axon_session_watch_checkpoints (
    path_hash TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL,
    basename TEXT NOT NULL,
    redacted_display TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    file_mtime_ms INTEGER NOT NULL,
    content_hash TEXT,
    failure_count INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TEXT,
    last_indexed_at TEXT,
    last_error_code TEXT,
    last_error_redacted TEXT,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_axon_session_watch_checkpoints_provider
    ON axon_session_watch_checkpoints(provider);

CREATE INDEX IF NOT EXISTS idx_axon_session_watch_checkpoints_error
    ON axon_session_watch_checkpoints(last_error_code)
    WHERE last_error_code IS NOT NULL;

CREATE TABLE IF NOT EXISTS axon_session_watch_errors (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path_hash TEXT NOT NULL,
    provider TEXT NOT NULL,
    basename TEXT NOT NULL,
    error_code TEXT NOT NULL,
    error_redacted TEXT NOT NULL,
    occurred_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_axon_session_watch_errors_path
    ON axon_session_watch_errors(path_hash);
