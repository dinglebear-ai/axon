-- Lease state for recurring source-request-backed watches.
--
-- `axon_source_watches` is the canonical recurring watch table. Its scheduler
-- leases rows directly from this table and enqueues `JobKind::Source` jobs,
-- without reading or writing legacy `axon_watch_defs` / `axon_watch_runs`.
ALTER TABLE axon_source_watches ADD COLUMN lease_expires_at INTEGER;
ALTER TABLE axon_source_watches ADD COLUMN auth_snapshot_json TEXT;

CREATE INDEX IF NOT EXISTS idx_source_watches_due_lease
    ON axon_source_watches(enabled, next_run_at, lease_expires_at);
