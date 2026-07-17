-- Canonical clean-break jobs runtime schema.
-- Older stores are reset and reindexed; no historical family tables are created.

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
, lease_expires_at INTEGER, auth_snapshot_json TEXT);
CREATE TABLE IF NOT EXISTS "jobs" (
    job_id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('source', 'watch', 'map', 'extract', 'research', 'ask', 'query', 'retrieve', 'memory', 'graph', 'prune', 'provider_probe', 'reset')),
    intent TEXT CHECK (intent IS NULL OR intent IN ('run', 'acquire', 'refresh', 'watch', 'exec', 'retry', 'recover', 'cleanup', 'probe', 'reset', 'index', 'map', 'extract')),
    status TEXT NOT NULL CHECK (status IN ('queued', 'pending', 'running', 'waiting', 'blocked', 'canceling', 'completed', 'completed_degraded', 'failed', 'canceled', 'expired', 'skipped')),
    phase TEXT NOT NULL CHECK (phase IN ('queued', 'requested', 'resolving', 'routing', 'authorizing', 'planning', 'leasing', 'discovering', 'diffing', 'fetching', 'rendering', 'enriching', 'normalizing', 'parsing', 'graphing', 'preparing', 'batching', 'embedding', 'vectorizing', 'upserting', 'retrieving', 'synthesizing', 'evaluating', 'publishing', 'cleaning', 'complete', 'canceled')),
    priority TEXT NOT NULL CHECK (priority IN ('interactive', 'high', 'normal', 'background', 'maintenance')),
    source_id TEXT REFERENCES sources(source_id) ON DELETE SET NULL,
    watch_id TEXT REFERENCES axon_source_watches(watch_id) ON DELETE SET NULL,
    parent_job_id TEXT REFERENCES "jobs"(job_id) ON DELETE SET NULL,
    root_job_id TEXT REFERENCES "jobs"(job_id) ON DELETE SET NULL,
    attempt INTEGER NOT NULL DEFAULT 0 CHECK (attempt >= 0),
    counts_json TEXT CHECK (counts_json IS NULL OR json_valid(counts_json)),
    current_json TEXT CHECK (current_json IS NULL OR json_valid(current_json)),
    heartbeat_json TEXT CHECK (heartbeat_json IS NULL OR json_valid(heartbeat_json)),
    last_error_json TEXT CHECK (last_error_json IS NULL OR json_valid(last_error_json)),
    warnings_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(warnings_json)),
    request_json TEXT CHECK (request_json IS NULL OR json_valid(request_json)),
    metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json)),
    idempotency_key TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    started_at TEXT,
    finished_at TEXT,
    auth_snapshot_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(auth_snapshot_json)),
    config_snapshot_id TEXT NOT NULL DEFAULT '',
    stage_plan_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(stage_plan_json)),
    requirements_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(requirements_json)),
    result_schema TEXT NOT NULL DEFAULT '',
    error_json TEXT CHECK (error_json IS NULL OR json_valid(error_json)),
    last_event_sequence INTEGER NOT NULL DEFAULT 0 CHECK (last_event_sequence >= 0),
    cooldown_until TEXT,
    deadline_at TEXT
);
CREATE TABLE IF NOT EXISTS job_attempts (
    attempt_id TEXT PRIMARY KEY NOT NULL,
    job_id TEXT NOT NULL REFERENCES jobs(job_id) ON DELETE CASCADE,
    attempt INTEGER NOT NULL CHECK (attempt >= 0),
    status TEXT NOT NULL CHECK (status IN ('queued', 'pending', 'running', 'waiting', 'blocked', 'canceling', 'completed', 'completed_degraded', 'failed', 'canceled', 'expired', 'skipped')),
    worker_id TEXT,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    heartbeat_at TEXT,
    error_json TEXT CHECK (error_json IS NULL OR json_valid(error_json)),
    UNIQUE(job_id, attempt)
);
CREATE TABLE IF NOT EXISTS job_stages (
    stage_id TEXT PRIMARY KEY NOT NULL,
    job_id TEXT NOT NULL REFERENCES jobs(job_id) ON DELETE CASCADE,
    phase TEXT NOT NULL CHECK (phase IN ('queued', 'requested', 'resolving', 'routing', 'authorizing', 'planning', 'leasing', 'discovering', 'diffing', 'fetching', 'rendering', 'enriching', 'normalizing', 'parsing', 'graphing', 'preparing', 'batching', 'embedding', 'vectorizing', 'upserting', 'retrieving', 'synthesizing', 'evaluating', 'publishing', 'cleaning', 'complete', 'canceled')),
    status TEXT NOT NULL CHECK (status IN ('queued', 'pending', 'running', 'waiting', 'blocked', 'canceling', 'completed', 'completed_degraded', 'failed', 'canceled', 'expired', 'skipped')),
    required INTEGER NOT NULL CHECK (required IN (0, 1)),
    provider_requirements_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(provider_requirements_json)),
    counts_json TEXT CHECK (counts_json IS NULL OR json_valid(counts_json)),
    started_at TEXT,
    completed_at TEXT,
    error_json TEXT CHECK (error_json IS NULL OR json_valid(error_json))
);
CREATE TABLE IF NOT EXISTS job_events (
    event_id TEXT PRIMARY KEY NOT NULL,
    job_id TEXT NOT NULL REFERENCES jobs(job_id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    attempt INTEGER NOT NULL DEFAULT 0 CHECK (attempt >= 0),
    stage_id TEXT REFERENCES job_stages(stage_id) ON DELETE SET NULL,
    phase TEXT NOT NULL CHECK (phase IN ('queued', 'requested', 'resolving', 'routing', 'authorizing', 'planning', 'leasing', 'discovering', 'diffing', 'fetching', 'rendering', 'enriching', 'normalizing', 'parsing', 'graphing', 'preparing', 'batching', 'embedding', 'vectorizing', 'upserting', 'retrieving', 'synthesizing', 'evaluating', 'publishing', 'cleaning', 'complete', 'canceled')),
    status TEXT NOT NULL CHECK (status IN ('queued', 'pending', 'running', 'waiting', 'blocked', 'canceling', 'completed', 'completed_degraded', 'failed', 'canceled', 'expired', 'skipped')),
    severity TEXT NOT NULL CHECK (severity IN ('debug', 'info', 'warning', 'degraded', 'failed', 'fatal')),
    visibility TEXT NOT NULL CHECK (visibility IN ('public', 'internal', 'sensitive', 'redacted', 'derived')),
    message TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    dedupe_key TEXT,
    details_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(details_json)),
    UNIQUE(job_id, sequence)
);
CREATE TABLE IF NOT EXISTS job_heartbeats (
    job_id TEXT NOT NULL REFERENCES jobs(job_id) ON DELETE CASCADE,
    attempt INTEGER NOT NULL DEFAULT 0 CHECK (attempt >= 0),
    heartbeat_at TEXT NOT NULL,
    heartbeat_json TEXT NOT NULL CHECK (json_valid(heartbeat_json)),
    PRIMARY KEY (job_id, attempt)
);
CREATE TABLE IF NOT EXISTS provider_reservations (
    reservation_id TEXT PRIMARY KEY NOT NULL,
    job_id TEXT NOT NULL REFERENCES jobs(job_id) ON DELETE CASCADE,
    stage_id TEXT REFERENCES job_stages(stage_id) ON DELETE SET NULL,
    provider_kind TEXT NOT NULL CHECK (provider_kind IN ('embedding', 'vector', 'llm', 'fetch', 'render', 'search', 'storage', 'cache', 'network_capture', 'artifact')),
    provider_id TEXT,
    priority TEXT NOT NULL CHECK (priority IN ('interactive', 'high', 'normal', 'background', 'maintenance')),
    requested_units INTEGER NOT NULL CHECK (requested_units >= 0),
    granted_units INTEGER NOT NULL CHECK (granted_units >= 0),
    acquired_at TEXT,
    expires_at TEXT,
    status TEXT NOT NULL CHECK (status IN ('requested', 'queued', 'granted', 'active', 'released', 'expired', 'canceled', 'failed')),
    queue_depth INTEGER CHECK (queue_depth IS NULL OR queue_depth >= 0),
    cooling_json TEXT CHECK (cooling_json IS NULL OR json_valid(cooling_json)),
    updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS job_artifacts (
    artifact_id TEXT PRIMARY KEY NOT NULL,
    job_id TEXT NOT NULL REFERENCES jobs(job_id) ON DELETE CASCADE,
    artifact_kind TEXT NOT NULL,
    uri TEXT NOT NULL,
    size_bytes INTEGER CHECK (size_bytes IS NULL OR size_bytes >= 0),
    content_hash TEXT,
    created_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS axon_source_watch_runs (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    watch_id   TEXT NOT NULL,
    job_id     TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (watch_id) REFERENCES axon_source_watches(watch_id) ON DELETE CASCADE
);
CREATE TABLE IF NOT EXISTS config_snapshots (
    config_snapshot_id TEXT PRIMARY KEY NOT NULL,
    config_json TEXT NOT NULL CHECK (json_valid(config_json)),
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_axon_job_attempts_job_attempt
    ON job_attempts(job_id, attempt);
CREATE INDEX IF NOT EXISTS idx_axon_job_events_job_sequence
    ON job_events(job_id, sequence);
CREATE INDEX IF NOT EXISTS idx_axon_job_events_job_severity_sequence
    ON job_events(job_id, severity, sequence);
CREATE INDEX IF NOT EXISTS idx_axon_job_stages_job_stage
    ON job_stages(job_id, stage_id);
CREATE INDEX IF NOT EXISTS idx_axon_jobs_claim
    ON jobs(
        status,
        CASE priority
            WHEN 'interactive' THEN 0
            WHEN 'high' THEN 1
            WHEN 'normal' THEN 2
            WHEN 'background' THEN 3
            WHEN 'maintenance' THEN 4
            ELSE 5
        END,
        updated_at ASC,
        job_id ASC
    )
    WHERE status IN ('queued', 'waiting', 'blocked');
CREATE INDEX IF NOT EXISTS idx_axon_jobs_claim_cooldown
    ON jobs(status, cooldown_until)
    WHERE status IN ('queued', 'waiting', 'blocked');
CREATE INDEX IF NOT EXISTS idx_axon_jobs_deadline
    ON jobs(deadline_at)
    WHERE deadline_at IS NOT NULL AND status = 'running';
CREATE INDEX IF NOT EXISTS idx_axon_jobs_source_status_updated
    ON jobs(source_id, status, updated_at DESC, job_id DESC);
CREATE INDEX IF NOT EXISTS idx_axon_jobs_source_updated
    ON jobs(source_id, updated_at DESC, job_id DESC);
CREATE INDEX IF NOT EXISTS idx_axon_jobs_status_kind_updated
    ON jobs(status, kind, updated_at DESC, job_id DESC);
CREATE INDEX IF NOT EXISTS idx_axon_jobs_updated
    ON jobs(updated_at DESC, job_id DESC);
CREATE INDEX IF NOT EXISTS idx_axon_jobs_watch_status_updated
    ON jobs(watch_id, status, updated_at DESC, job_id DESC);
CREATE INDEX IF NOT EXISTS idx_axon_jobs_watch_updated
    ON jobs(watch_id, updated_at DESC, job_id DESC);
CREATE INDEX IF NOT EXISTS idx_source_watch_runs_watch_id ON axon_source_watch_runs(watch_id, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_source_watches_created_cursor
    ON axon_source_watches(created_at DESC, watch_id DESC);
CREATE INDEX IF NOT EXISTS idx_source_watches_due ON axon_source_watches(next_run_at);
CREATE INDEX IF NOT EXISTS idx_source_watches_due_lease
    ON axon_source_watches(enabled, next_run_at, lease_expires_at);
CREATE INDEX IF NOT EXISTS idx_source_watches_source_id ON axon_source_watches(source_id);
CREATE INDEX IF NOT EXISTS job_artifacts_job_id_idx ON job_artifacts(job_id);
CREATE INDEX IF NOT EXISTS job_artifacts_job_kind_idx ON job_artifacts(job_id, artifact_kind);
CREATE INDEX IF NOT EXISTS job_attempts_job_id_idx ON job_attempts(job_id);
CREATE UNIQUE INDEX IF NOT EXISTS job_events_job_dedupe_key_idx
    ON job_events(job_id, dedupe_key)
    WHERE dedupe_key IS NOT NULL;
CREATE INDEX IF NOT EXISTS job_events_job_phase_idx ON job_events(job_id, phase);
CREATE INDEX IF NOT EXISTS job_events_job_sequence_idx ON job_events(job_id, sequence);
CREATE INDEX IF NOT EXISTS job_events_job_severity_idx ON job_events(job_id, severity);
CREATE INDEX IF NOT EXISTS job_events_job_visibility_idx ON job_events(job_id, visibility);
CREATE INDEX IF NOT EXISTS job_heartbeats_heartbeat_at_idx ON job_heartbeats(heartbeat_at);
CREATE INDEX IF NOT EXISTS job_heartbeats_job_id_idx ON job_heartbeats(job_id);
CREATE INDEX IF NOT EXISTS job_stages_job_id_idx ON job_stages(job_id);
CREATE INDEX IF NOT EXISTS jobs_created_at_desc_idx ON jobs(created_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS jobs_idempotency_key_idx
    ON jobs(idempotency_key)
    WHERE idempotency_key IS NOT NULL;
CREATE INDEX IF NOT EXISTS jobs_kind_status_created_at_idx ON jobs(kind, status, created_at DESC);
CREATE INDEX IF NOT EXISTS jobs_source_id_created_at_idx ON jobs(source_id, created_at DESC);
CREATE INDEX IF NOT EXISTS jobs_source_id_idx ON jobs(source_id);
CREATE INDEX IF NOT EXISTS jobs_status_created_at_idx ON jobs(status, created_at DESC);
CREATE INDEX IF NOT EXISTS jobs_status_updated_at_idx ON jobs(status, updated_at);
CREATE INDEX IF NOT EXISTS jobs_watch_id_created_at_idx ON jobs(watch_id, created_at DESC);
CREATE INDEX IF NOT EXISTS jobs_watch_id_idx ON jobs(watch_id);
CREATE INDEX IF NOT EXISTS provider_reservations_job_id_idx ON provider_reservations(job_id);
CREATE INDEX IF NOT EXISTS provider_reservations_provider_kind_idx ON provider_reservations(provider_kind);
CREATE INDEX IF NOT EXISTS provider_reservations_stage_id_idx ON provider_reservations(stage_id);
