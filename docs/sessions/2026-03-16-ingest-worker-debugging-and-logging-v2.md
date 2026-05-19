# Session: Ingest Worker Follow-up — Schema Index, Timeout Tuning, PreparedDoc Docs
Date: 2026-03-16 (continuation of v1 session)

## Session Overview

Follow-up to the ingest worker debugging session. Added a missing Postgres index for stale-job watchdog queries, documented the `PreparedDoc` API in `crates/ingest/CLAUDE.md`, diagnosed embed timeout cascades caused by TEI transport errors, and bumped `AXON_EMBED_DOC_TIMEOUT_SECS` from 120s to 300s.

## Timeline

| Time (UTC) | Event |
|---|---|
| ~05:50 | Slow statement WARN observed: `UPDATE axon_ingest_jobs SET updated_at=NOW()` |
| ~05:51 | Diagnosed: missing partial index on `(updated_at) WHERE status='running'` causing full table scan during watchdog sweeps, blocking heartbeat UPDATEs |
| ~05:51 | Added index to `crates/jobs/ingest/schema.rs` |
| ~05:52 | Updated `crates/ingest/CLAUDE.md` with full `PreparedDoc` field reference table |
| ~05:57 | Embed timeout cascade observed: 100+ `embed timed out after 120s` warnings across docker/cli, tailscale, futures-rs simultaneously |
| ~05:57 | Root cause identified: TEI had 3 transport errors (brief connection drop), causing retry delays that pushed in-flight docs past the 120s timeout |
| ~05:57 | Identified that `12` doc failures = `AXON_EMBED_DOC_CONCURRENCY` default (CPU count = 12 cores) |
| ~06:00 | Bumped `AXON_EMBED_DOC_TIMEOUT_SECS` from 120s to 300s in `.env` |

## Key Findings

- **Missing index on `axon_ingest_jobs`**: Only index was `idx_axon_ingest_jobs_pending` on `(created_at) WHERE status='pending'`. The `reclaim_stale_running_jobs` query (`WHERE status='running' AND updated_at < threshold`) had no index — caused full table scan that blocked heartbeat UPDATEs with lock contention.
- **Embed timeout cascade root cause**: TEI transport errors (`error sending request for url`) triggered retry logic (delays: 1s, 2s, 4s, 8s, 16s = 31s max). Combined with docs waiting for semaphore permits, total wait exceeded 120s. Not a tree-sitter or chunk-count issue.
- **Why always 12 failures**: `AXON_EMBED_DOC_CONCURRENCY` defaults to `std::thread::available_parallelism()` — 12 cores on this machine. All 12 in-flight docs hit the TEI drop simultaneously and timed out together.
- **300s timeout is sufficient**: Retry budget (31s max) + semaphore queue wait fits well within 300s. Only a sustained TEI outage >300s would still cause failures.
- **PreparedDoc is the universal contract**: All ingest sources (crawl, github, reddit, youtube, embed) produce `PreparedDoc`. New ingest source = produce content, chunk it, build `PreparedDoc`, call `embed_prepared_docs`. Everything downstream is shared infrastructure.

## Files Modified

| File | Change |
|---|---|
| `crates/jobs/ingest/schema.rs` | Added `idx_axon_ingest_jobs_running_updated` partial index on `(updated_at ASC) WHERE status='running'` to fix slow heartbeat UPDATEs |
| `crates/ingest/CLAUDE.md` | Added `PreparedDoc` field reference table with types, required/optional, and downstream behavior for all 7 fields |
| `.env` | `AXON_EMBED_DOC_TIMEOUT_SECS`: 120 → 300 |

## Commands Executed

```bash
# Check index state
docker exec axon-postgres psql -U axon -d axon -c "\d axon_ingest_jobs"

# Confirm embed timeout env var
grep "AXON_EMBED_DOC_TIMEOUT" .env

# Check TEI for transport errors
ssh steamy-wsl "docker logs tei_max --tail 30"

# Verify doc concurrency default
grep -A3 "AXON_EMBED_DOC_CONCURRENCY" crates/vector/ops/tei/pipeline.rs
```

## Behavior Changes (Before/After)

| Area | Before | After |
|---|---|---|
| Heartbeat UPDATE latency | Slow (full table scan lock contention from watchdog sweep) | Fast — new partial index on `(updated_at) WHERE status='running'` |
| Embed timeout on TEI blip | 12 docs timeout at 120s (retry budget + queue wait exceeds threshold) | 300s headroom — retry budget (31s max) fits comfortably |
| `crates/ingest/CLAUDE.md` | Canonical pattern only — no field-level docs | Full `PreparedDoc` field reference table with types and downstream notes |

## Verification Evidence

| Check | Expected | Actual | Status |
|---|---|---|---|
| `\d axon_ingest_jobs` after index add (requires worker restart) | `idx_axon_ingest_jobs_running_updated` present | Schema shows only existing indexes (new one created on next worker startup via `ensure_schema`) | ⏳ PENDING restart |
| `grep AXON_EMBED_DOC_TIMEOUT .env` | `300` | `AXON_EMBED_DOC_TIMEOUT_SECS=300` | ✓ PASS |
| TEI logs during normal operation | Constant `Success` stream | Confirmed — 10 consecutive Success entries at 66–265ms | ✓ PASS |

## Risks and Rollback

- **Index add**: `CREATE INDEX IF NOT EXISTS` — idempotent, safe on next `ensure_schema` call. No data loss. Rollback: `DROP INDEX idx_axon_ingest_jobs_running_updated`.
- **Timeout bump**: Env var change only — rollback by setting `AXON_EMBED_DOC_TIMEOUT_SECS=120` and restarting workers. Only risk: failed docs that would have been skipped at 120s now retry longer, potentially keeping a job alive longer on sustained TEI outage.
- **Workers need restart**: Both index and timeout changes require worker restart to take effect. Current workers are still using 120s timeout.

## Decisions Not Taken

- **Reduce `AXON_EMBED_DOC_CONCURRENCY`**: Would reduce blast radius when TEI drops but slow down normal embedding. Rejected — 300s timeout is the right fix.
- **Retry failed docs at batch level**: Would recover the 12 skipped docs after a batch completes. Deferred — upsert-first means re-running ingest fills the gaps anyway.
- **Increase timeout to 600s**: Overkill — 300s is 9x the max retry budget with plenty of margin.

## Open Questions

- New index (`idx_axon_ingest_jobs_running_updated`) takes effect on next worker restart — confirm it appears in `\d axon_ingest_jobs` after `just dev`.
- `AXON_EMBED_DOC_TIMEOUT_SECS=300` picked up by workers on next restart — confirm no further 120s timeouts in log after restart.
- TEI transport errors: brief blip or recurring? Monitor `ssh steamy-wsl docker logs tei_max` for pattern.

## Next Steps

- Restart workers (`just dev`) to pick up index + timeout changes
- Monitor `axon.log` after restart to confirm clean embed pipeline (no timeout cascades)
- Consider adding `AXON_EMBED_DOC_CONCURRENCY` to `.env.example` with a note explaining it defaults to CPU count
