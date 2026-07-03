-- axon-observe durable observability tables.
--
-- These back the production `SqliteObservabilitySink`. They intentionally mirror
-- the observability-contract shapes (SourceProgressEvent / JobHeartbeat /
-- provider degradation) without importing axon-jobs, so the crate stays
-- self-contained and layering-clean.

CREATE TABLE IF NOT EXISTS axon_observe_events (
    event_id     TEXT NOT NULL PRIMARY KEY,
    job_id       TEXT NOT NULL,
    sequence     INTEGER NOT NULL,
    phase        TEXT NOT NULL,
    status       TEXT NOT NULL,
    severity     TEXT NOT NULL,
    visibility   TEXT NOT NULL,
    message      TEXT NOT NULL,
    timestamp    TEXT NOT NULL,
    event_json   TEXT NOT NULL,
    created_at   INTEGER NOT NULL
);

-- One event stream per job, strictly increasing sequence. The UNIQUE index is
-- the durable guard behind the in-process SequenceRegistry: a duplicate
-- (job_id, sequence) is a contract violation and fails the insert.
CREATE UNIQUE INDEX IF NOT EXISTS idx_observe_events_job_sequence
    ON axon_observe_events (job_id, sequence);

CREATE INDEX IF NOT EXISTS idx_observe_events_job_created
    ON axon_observe_events (job_id, created_at);

CREATE TABLE IF NOT EXISTS axon_observe_heartbeats (
    job_id              TEXT NOT NULL PRIMARY KEY,
    attempt             INTEGER NOT NULL,
    worker_id           TEXT,
    phase               TEXT NOT NULL,
    status              TEXT NOT NULL,
    heartbeat_at        TEXT NOT NULL,
    last_event_sequence INTEGER,
    heartbeat_json      TEXT NOT NULL,
    updated_at          INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS axon_observe_provider_health (
    provider_id    TEXT NOT NULL PRIMARY KEY,
    provider_kind  TEXT NOT NULL,
    status         TEXT NOT NULL,
    cooldown_until TEXT,
    last_error_code TEXT,
    updated_at     INTEGER NOT NULL
);
