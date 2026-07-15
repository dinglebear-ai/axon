-- Final job-family cleanup: the only durable job kinds are the canonical
-- SourceRequest/runtime kinds. Historical migrations may have widened the
-- jobs.kind CHECK while work was in flight; this migration tightens it again
-- and drops the old per-family tables from upgraded databases.

CREATE TABLE jobs_v4 (
    job_id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('source', 'watch', 'map', 'extract', 'research', 'ask', 'query', 'retrieve', 'memory', 'graph', 'prune', 'provider_probe', 'reset')),
    intent TEXT CHECK (intent IS NULL OR intent IN ('run', 'acquire', 'refresh', 'watch', 'exec', 'retry', 'recover', 'cleanup', 'probe', 'reset', 'index', 'map', 'extract')),
    status TEXT NOT NULL CHECK (status IN ('queued', 'pending', 'running', 'waiting', 'blocked', 'canceling', 'completed', 'completed_degraded', 'failed', 'canceled', 'expired', 'skipped')),
    phase TEXT NOT NULL CHECK (phase IN ('queued', 'requested', 'resolving', 'routing', 'authorizing', 'planning', 'leasing', 'discovering', 'diffing', 'fetching', 'rendering', 'enriching', 'normalizing', 'parsing', 'graphing', 'preparing', 'batching', 'embedding', 'vectorizing', 'upserting', 'retrieving', 'synthesizing', 'evaluating', 'publishing', 'cleaning', 'complete', 'canceled')),
    priority TEXT NOT NULL CHECK (priority IN ('interactive', 'high', 'normal', 'background', 'maintenance')),
    source_id TEXT REFERENCES sources(source_id) ON DELETE SET NULL,
    watch_id TEXT REFERENCES axon_watch_defs(id) ON DELETE SET NULL,
    parent_job_id TEXT REFERENCES jobs_v4(job_id) ON DELETE SET NULL,
    root_job_id TEXT REFERENCES jobs_v4(job_id) ON DELETE SET NULL,
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

INSERT INTO jobs_v4 (
    job_id, kind, intent, status, phase, priority, source_id, watch_id,
    parent_job_id, root_job_id, attempt, counts_json, current_json,
    heartbeat_json, last_error_json, warnings_json, request_json,
    metadata_json, idempotency_key, created_at, updated_at, started_at,
    finished_at, auth_snapshot_json, config_snapshot_id, stage_plan_json,
    requirements_json, result_schema, error_json, last_event_sequence,
    cooldown_until, deadline_at
)
SELECT
    job_id, kind, intent, status, phase, priority, source_id, watch_id,
    parent_job_id, root_job_id, attempt, counts_json, current_json,
    heartbeat_json, last_error_json, warnings_json, request_json,
    metadata_json, idempotency_key, created_at, updated_at, started_at,
    finished_at, auth_snapshot_json, config_snapshot_id, stage_plan_json,
    requirements_json, result_schema, error_json, last_event_sequence,
    cooldown_until, deadline_at
FROM jobs
WHERE kind IN ('source', 'watch', 'map', 'extract', 'research', 'ask', 'query', 'retrieve', 'memory', 'graph', 'prune', 'provider_probe', 'reset');

DROP TABLE jobs;
ALTER TABLE jobs_v4 RENAME TO jobs;

DELETE FROM job_attempts WHERE job_id NOT IN (SELECT job_id FROM jobs);
DELETE FROM job_stages WHERE job_id NOT IN (SELECT job_id FROM jobs);
DELETE FROM job_events WHERE job_id NOT IN (SELECT job_id FROM jobs);
DELETE FROM job_heartbeats WHERE job_id NOT IN (SELECT job_id FROM jobs);
DELETE FROM provider_reservations WHERE job_id NOT IN (SELECT job_id FROM jobs);
DELETE FROM job_artifacts WHERE job_id NOT IN (SELECT job_id FROM jobs);

DROP TABLE IF EXISTS axon_ingest_payloads;
DROP TABLE IF EXISTS axon_crawl_jobs;
DROP TABLE IF EXISTS axon_embed_jobs;
DROP TABLE IF EXISTS axon_extract_jobs;
DROP TABLE IF EXISTS axon_ingest_jobs;
DROP TABLE IF EXISTS axon_job_cutover_receipts;

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
CREATE INDEX idx_axon_jobs_status_kind_updated
    ON jobs(status, kind, updated_at DESC, job_id DESC);
CREATE INDEX idx_axon_jobs_source_status_updated
    ON jobs(source_id, status, updated_at DESC, job_id DESC);
CREATE INDEX idx_axon_jobs_watch_status_updated
    ON jobs(watch_id, status, updated_at DESC, job_id DESC);
CREATE INDEX idx_axon_jobs_updated
    ON jobs(updated_at DESC, job_id DESC);
CREATE INDEX idx_axon_jobs_source_updated
    ON jobs(source_id, updated_at DESC, job_id DESC);
CREATE INDEX idx_axon_jobs_watch_updated
    ON jobs(watch_id, updated_at DESC, job_id DESC);
CREATE INDEX idx_axon_jobs_claim
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
CREATE INDEX idx_axon_jobs_claim_cooldown
    ON jobs(status, cooldown_until)
    WHERE status IN ('queued', 'waiting', 'blocked');
CREATE INDEX idx_axon_jobs_deadline
    ON jobs(deadline_at)
    WHERE deadline_at IS NOT NULL AND status = 'running';
