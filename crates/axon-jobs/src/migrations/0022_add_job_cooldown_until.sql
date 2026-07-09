-- Provider cooling: bounded cooldown window on the unified `jobs` table so a
-- job that hit a saturated provider (e.g. TEI 429 after local retries are
-- exhausted) backs off instead of being reclaimed and re-failing in a tight
-- loop. See docs/pipeline-unification/plans/2026-07-08-provider-cooling.md.
--
-- `cooldown_until` is stored as TEXT (RFC3339), matching every other
-- timestamp column on `jobs` (created_at/updated_at/started_at/finished_at) —
-- all written via the `Timestamp(pub String)` newtype, not epoch integers.
--
-- The claim query (`claim_next_unified_job_unchecked` in
-- crates/axon-jobs/src/workers/unified.rs) filters
-- `status IN ('queued', 'waiting', 'blocked')` and must additionally exclude
-- rows whose `cooldown_until` is still in the future. A partial index scoped
-- to `status = 'waiting'` was tried first, but SQLite's partial-index
-- matcher cannot prove that a 3-value `status IN (...)` predicate is
-- contained by a `WHERE status = 'waiting'` partial index, so the planner
-- fell back to a full table scan of `jobs` on every worker poll (verified
-- via `EXPLAIN QUERY PLAN` against ANALYZE'd data — `SCAN jobs` instead of a
-- `SEARCH`). Scoping the partial index to the exact same 3-value status set
-- the claim query filters on lets the planner prove containment and use the
-- index (`SEARCH jobs USING INDEX idx_axon_jobs_claim_cooldown`). The
-- pre-existing `idx_axon_jobs_claim` (migration 0019) is left untouched.
ALTER TABLE jobs ADD COLUMN cooldown_until TEXT;

CREATE INDEX idx_axon_jobs_claim_cooldown
    ON jobs(status, cooldown_until)
    WHERE status IN ('queued', 'waiting', 'blocked');
