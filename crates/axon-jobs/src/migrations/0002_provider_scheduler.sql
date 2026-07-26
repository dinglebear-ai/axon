-- Provider scheduler foundation. This is append-only: epoch-1 reservation
-- rows remain readable, but non-terminal work is cancelled and retried under
-- the new scheduler authority rather than being silently granted.
ALTER TABLE provider_reservations ADD COLUMN capacity_domain TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE provider_reservations ADD COLUMN instance_id TEXT;
ALTER TABLE provider_reservations ADD COLUMN authority_id TEXT;
ALTER TABLE provider_reservations ADD COLUMN enqueue_sequence INTEGER;
ALTER TABLE provider_reservations ADD COLUMN requested_priority TEXT;
ALTER TABLE provider_reservations ADD COLUMN effective_priority TEXT;
ALTER TABLE provider_reservations ADD COLUMN queue_deadline TEXT;
ALTER TABLE provider_reservations ADD COLUMN grant_deadline TEXT;
ALTER TABLE provider_reservations ADD COLUMN lease_owner TEXT;
ALTER TABLE provider_reservations ADD COLUMN attempt INTEGER NOT NULL DEFAULT 0;
ALTER TABLE provider_reservations ADD COLUMN fence TEXT;
ALTER TABLE provider_reservations ADD COLUMN renewed_at TEXT;
ALTER TABLE provider_reservations ADD COLUMN terminal_reason TEXT;
ALTER TABLE provider_reservations ADD COLUMN quarantined INTEGER NOT NULL DEFAULT 0 CHECK (quarantined IN (0, 1));

UPDATE provider_reservations
SET status = 'canceled',
    terminal_reason = 'migration_cancelled',
    updated_at = datetime('now')
WHERE status IN ('requested', 'queued', 'granted', 'active');

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
