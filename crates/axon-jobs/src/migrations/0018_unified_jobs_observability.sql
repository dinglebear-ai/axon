CREATE TABLE jobs (
    job_id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('source', 'watch', 'map', 'extract', 'research', 'ask', 'query', 'retrieve', 'memory', 'graph', 'prune', 'provider_probe', 'reset')),
    intent TEXT CHECK (intent IS NULL OR intent IN ('run', 'acquire', 'refresh', 'watch', 'exec', 'retry', 'recover', 'cleanup', 'probe', 'reset')),
    status TEXT NOT NULL CHECK (status IN ('queued', 'pending', 'running', 'waiting', 'blocked', 'canceling', 'completed', 'completed_degraded', 'failed', 'canceled', 'expired', 'skipped')),
    phase TEXT NOT NULL CHECK (phase IN ('queued', 'requested', 'resolving', 'routing', 'authorizing', 'planning', 'leasing', 'discovering', 'diffing', 'fetching', 'rendering', 'enriching', 'normalizing', 'parsing', 'graphing', 'preparing', 'batching', 'embedding', 'vectorizing', 'upserting', 'retrieving', 'synthesizing', 'evaluating', 'publishing', 'cleaning', 'complete', 'canceled')),
    priority TEXT NOT NULL CHECK (priority IN ('interactive', 'high', 'normal', 'background', 'maintenance')),
    source_id TEXT REFERENCES sources(source_id) ON DELETE SET NULL,
    watch_id TEXT REFERENCES axon_watch_defs(id) ON DELETE SET NULL,
    parent_job_id TEXT REFERENCES jobs(job_id) ON DELETE SET NULL,
    root_job_id TEXT REFERENCES jobs(job_id) ON DELETE SET NULL,
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
    finished_at TEXT
);

CREATE UNIQUE INDEX jobs_idempotency_key_idx
    ON jobs(idempotency_key)
    WHERE idempotency_key IS NOT NULL;

CREATE INDEX jobs_created_at_desc_idx ON jobs(created_at DESC);
CREATE INDEX jobs_status_created_at_idx ON jobs(status, created_at DESC);
CREATE INDEX jobs_kind_status_created_at_idx ON jobs(kind, status, created_at DESC);
CREATE INDEX jobs_status_updated_at_idx ON jobs(status, updated_at);
CREATE INDEX jobs_source_id_idx ON jobs(source_id);
CREATE INDEX jobs_watch_id_idx ON jobs(watch_id);
CREATE INDEX jobs_source_id_created_at_idx ON jobs(source_id, created_at DESC);
CREATE INDEX jobs_watch_id_created_at_idx ON jobs(watch_id, created_at DESC);

CREATE TABLE job_attempts (
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

CREATE INDEX job_attempts_job_id_idx ON job_attempts(job_id);

CREATE TABLE job_stages (
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

CREATE INDEX job_stages_job_id_idx ON job_stages(job_id);

CREATE TABLE job_events (
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

CREATE UNIQUE INDEX job_events_job_dedupe_key_idx
    ON job_events(job_id, dedupe_key)
    WHERE dedupe_key IS NOT NULL;
CREATE INDEX job_events_job_sequence_idx ON job_events(job_id, sequence);
CREATE INDEX job_events_job_phase_idx ON job_events(job_id, phase);
CREATE INDEX job_events_job_severity_idx ON job_events(job_id, severity);
CREATE INDEX job_events_job_visibility_idx ON job_events(job_id, visibility);

CREATE TABLE job_heartbeats (
    job_id TEXT NOT NULL REFERENCES jobs(job_id) ON DELETE CASCADE,
    attempt INTEGER NOT NULL DEFAULT 0 CHECK (attempt >= 0),
    heartbeat_at TEXT NOT NULL,
    heartbeat_json TEXT NOT NULL CHECK (json_valid(heartbeat_json)),
    PRIMARY KEY (job_id, attempt)
);

CREATE INDEX job_heartbeats_job_id_idx ON job_heartbeats(job_id);
CREATE INDEX job_heartbeats_heartbeat_at_idx ON job_heartbeats(heartbeat_at);

CREATE TABLE provider_reservations (
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

CREATE INDEX provider_reservations_job_id_idx ON provider_reservations(job_id);
CREATE INDEX provider_reservations_stage_id_idx ON provider_reservations(stage_id);
CREATE INDEX provider_reservations_provider_kind_idx ON provider_reservations(provider_kind);

CREATE TABLE job_artifacts (
    artifact_id TEXT PRIMARY KEY NOT NULL,
    job_id TEXT NOT NULL REFERENCES jobs(job_id) ON DELETE CASCADE,
    artifact_kind TEXT NOT NULL,
    uri TEXT NOT NULL,
    size_bytes INTEGER CHECK (size_bytes IS NULL OR size_bytes >= 0),
    content_hash TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX job_artifacts_job_id_idx ON job_artifacts(job_id);
CREATE INDEX job_artifacts_job_kind_idx ON job_artifacts(job_id, artifact_kind);
