-- Widen the durable provider scheduler's provider_kind CHECK to include
-- the parser capacity domain. SQLite cannot alter CHECK constraints in place,
-- so rebuild the table append-only while preserving every scheduler column,
-- foreign key, row, and index from migrations 0001-0003.
CREATE TABLE provider_reservations_v4 (
    reservation_id TEXT PRIMARY KEY NOT NULL,
    job_id TEXT NOT NULL REFERENCES jobs(job_id) ON DELETE CASCADE,
    stage_id TEXT REFERENCES job_stages(stage_id) ON DELETE SET NULL,
    provider_kind TEXT NOT NULL CHECK (provider_kind IN ('embedding', 'vector', 'llm', 'fetch', 'render', 'search', 'storage', 'cache', 'network_capture', 'artifact', 'parser')),
    provider_id TEXT,
    priority TEXT NOT NULL CHECK (priority IN ('interactive', 'high', 'normal', 'background', 'maintenance')),
    requested_units INTEGER NOT NULL CHECK (requested_units >= 0),
    granted_units INTEGER NOT NULL CHECK (granted_units >= 0),
    acquired_at TEXT,
    expires_at TEXT,
    status TEXT NOT NULL CHECK (status IN ('requested', 'queued', 'granted', 'active', 'released', 'expired', 'canceled', 'failed')),
    queue_depth INTEGER CHECK (queue_depth IS NULL OR queue_depth >= 0),
    cooling_json TEXT CHECK (cooling_json IS NULL OR json_valid(cooling_json)),
    updated_at TEXT NOT NULL,
    capacity_domain TEXT NOT NULL DEFAULT 'legacy',
    instance_id TEXT,
    authority_id TEXT,
    enqueue_sequence INTEGER,
    requested_priority TEXT,
    effective_priority TEXT,
    queue_deadline TEXT,
    grant_deadline TEXT,
    lease_owner TEXT,
    attempt INTEGER NOT NULL DEFAULT 0,
    fence TEXT,
    renewed_at TEXT,
    terminal_reason TEXT,
    quarantined INTEGER NOT NULL DEFAULT 0 CHECK (quarantined IN (0, 1))
);

INSERT INTO provider_reservations_v4 (
    reservation_id, job_id, stage_id, provider_kind, provider_id, priority,
    requested_units, granted_units, acquired_at, expires_at, status, queue_depth,
    cooling_json, updated_at, capacity_domain, instance_id, authority_id,
    enqueue_sequence, requested_priority, effective_priority, queue_deadline,
    grant_deadline, lease_owner, attempt, fence, renewed_at, terminal_reason,
    quarantined
)
SELECT
    reservation_id, job_id, stage_id, provider_kind, provider_id, priority,
    requested_units, granted_units, acquired_at, expires_at, status, queue_depth,
    cooling_json, updated_at, capacity_domain, instance_id, authority_id,
    enqueue_sequence, requested_priority, effective_priority, queue_deadline,
    grant_deadline, lease_owner, attempt, fence, renewed_at, terminal_reason,
    quarantined
FROM provider_reservations;

DROP TABLE provider_reservations;
ALTER TABLE provider_reservations_v4 RENAME TO provider_reservations;

CREATE INDEX provider_reservations_job_id_idx ON provider_reservations(job_id);
CREATE INDEX provider_reservations_provider_kind_idx ON provider_reservations(provider_kind);
CREATE INDEX provider_reservations_stage_id_idx ON provider_reservations(stage_id);
CREATE INDEX provider_reservations_scheduler_head_idx
    ON provider_reservations (
        capacity_domain,
        status,
        quarantined,
        effective_priority,
        enqueue_sequence,
        reservation_id
    );
CREATE INDEX provider_reservations_scheduler_expiry_idx
    ON provider_reservations (status, grant_deadline, expires_at);
CREATE INDEX provider_reservations_scheduler_owner_idx
    ON provider_reservations (authority_id, lease_owner, status);
CREATE INDEX provider_reservations_scheduler_instance_state_idx
    ON provider_reservations (
        capacity_domain,
        instance_id,
        status,
        effective_priority,
        enqueue_sequence,
        reservation_id
    );
CREATE INDEX provider_reservations_scheduler_instance_sequence_idx
    ON provider_reservations (capacity_domain, instance_id, enqueue_sequence);
CREATE INDEX provider_reservations_scheduler_job_state_idx
    ON provider_reservations (job_id, status);
