-- `config_snapshots` — the real backing table for `jobs.config_snapshot_id`
-- (added in 0019_unified_jobs_contract_fields.sql as a bare TEXT column).
--
-- `docs/pipeline-unification/schemas/database-schema.md`'s "Required Tables"
-- registry and `docs/pipeline-unification/runtime/schema-contract.md` both
-- list `config_snapshots` (owned by `axon-jobs`, PK `config_snapshot_id`) as
-- a canonical target table, explicitly distinct from the rejected legacy
-- name `job_config_snapshots`. `config_snapshot_id` is a deterministic hash
-- of the serialized config/provider material a job ran with (see
-- `axon-services::config_snapshot_hash` and
-- `axon-jobs::config_snapshot::config_snapshot_json`); this table lets that
-- id resolve to real, stored content instead of being a label with nothing
-- behind it.
--
-- Content-addressed: `config_snapshot_id` is a hash of `config_json`, so the
-- same snapshot content from different jobs collapses to one row
-- (`INSERT OR IGNORE`, no dedup logic needed at the call site).
CREATE TABLE config_snapshots (
    config_snapshot_id TEXT PRIMARY KEY NOT NULL,
    config_json TEXT NOT NULL CHECK (json_valid(config_json)),
    created_at TEXT NOT NULL
);
