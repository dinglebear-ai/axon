-- Scheduler hot-path indexes. Reservation admission and grant polling are
-- scoped by capacity domain + instance, while per-job admission is scoped by
-- job + status. Keep these append-only so existing epoch-1 stores upgrade
-- without changing earlier migration receipts.
CREATE INDEX IF NOT EXISTS provider_reservations_scheduler_instance_state_idx
    ON provider_reservations (
        capacity_domain,
        instance_id,
        status,
        effective_priority,
        enqueue_sequence,
        reservation_id
    );

CREATE INDEX IF NOT EXISTS provider_reservations_scheduler_instance_sequence_idx
    ON provider_reservations (capacity_domain, instance_id, enqueue_sequence);

CREATE INDEX IF NOT EXISTS provider_reservations_scheduler_job_state_idx
    ON provider_reservations (job_id, status);
