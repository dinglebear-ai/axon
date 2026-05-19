# Session: Ingest Worker Debugging and Logging Instrumentation
Date: 2026-03-16

## Session Overview

Investigated stuck GitHub ingest jobs, diagnosed root causes (orphaned worker + TEI restart), recovered the queue, and added `log_info` instrumentation to the ingest_github pipeline to eliminate the multi-minute visibility gap between job start and job completion.

## Timeline

| Time (UTC) | Event |
|---|---|
| ~04:30 | User reports ingestions are stuck |
| ~04:39 | Discovered worker PID 2385498 pegged at 89% CPU, 6 jobs stuck in `running` |
| ~04:40 | Confirmed TEI had restarted at 05:02, spending ~4 min reloading model weights |
| ~04:45 | Killed worker PID 2385498, reset 6 running jobs to pending via SQL |
| ~04:50 | Restarted ingest worker; second worker also died when TEI retries exhausted |
| ~05:07 | TEI confirmed back online via `ssh steamy-wsl docker logs tei_max` |
| ~05:10 | Killed orphaned workers, reset 12 more stuck jobs, restarted cleanly |
| ~05:15 | Discovered console log is WARN-only; real INFO logs at `$AXON_DATA_DIR/axon/logs/axon.log` |
| ~05:20 | Identified the logging gap: zero visibility during git clone, file enumeration, AST chunking, embedding phases |
| ~05:25 | Added `log_info` instrumentation to `files.rs`, `batch.rs`, `github.rs` |
| ~05:40 | Rebuilt with `just dev`, workers running clean |
| ~05:43 | TEI confirmed steady stream of `Success` with no retries |

## Key Findings

- **Console stderr is WARN-only**: `init_tracing()` in `crates/core/logging.rs` sets `console_filter = "warn"`. All `log_info` calls are invisible on stderr. Real INFO logs go to `$AXON_DATA_DIR/axon/logs/axon.log` in JSON format.
- **Wrong log file**: We were watching `/tmp/axon-ingest-worker.log` (stderr redirect, WARN only) — missed all INFO-level progress for 30+ minutes.
- **Heartbeat is silent**: `spawn_heartbeat_task` in `crates/jobs/common/job_ops.rs:211` fires a silent `UPDATE axon_ingest_jobs SET updated_at = NOW()` every 30s. No log output. Its only purpose is stale-job watchdog prevention.
- **Tree-sitter confirmed working**: URLs in logs show per-function line ranges (e.g., `input.rs#L299-L311`) — these only exist when `line_range_for_chunk` runs post-AST-chunking. Prose chunking produces larger arbitrary ranges.
- **TEI restart root cause**: `tei_max` on `steamy-wsl` had restarted and spent ~4 minutes re-downloading `model.safetensors` before accepting requests. Workers exhausted their retry budget during this window.
- **AMQP queue depth**: After multiple restarts, RabbitMQ accumulated 448 messages. Safe because `claim_next_pending` uses `WHERE status='pending'` — duplicates are harmless.

## Technical Decisions

- **Added `log_info` at phase boundaries, not per-file**: Logging every file would be too noisy for repos with 2000+ files. Progress every 25 files (`FILE_PROGRESS_EVERY = 25`) strikes the right balance.
- **`just dev` over targeted restart**: User chose `just dev` to rebuild all workers cleanly. Since we needed to kill the ingest worker anyway to pick up new binary, this was the right call.
- **Did not add tree-sitter-specific logging**: Confirmed via line-range URL evidence that it's working. Adding explicit "used AST / fell back" logs deferred — would require touching `chunk_code()` in vector ops.
- **Checked Postgres directly via `docker exec`**: `psql` not available locally; all job state queries went through `docker exec axon-postgres psql -U axon -d axon`.

## Files Modified

| File | Change |
|---|---|
| `crates/ingest/github/files.rs` | Added `log_info` before `clone_repo()` call — was the longest single dark period |
| `crates/ingest/github/files/batch.rs` | Added `log_info` for every-25-file progress checkpoint and before each `embed_prepared_docs` batch flush |
| `crates/ingest/github.rs` | Added `log_info` before `tokio::join!` (tasks_start) and per-task completion in `tally_results` |
| `/home/jmagar/.claude/projects/-home-jmagar-workspace-axon-rust/memory/feedback_logging_visibility.md` | New memory entry: always instrument long-running async phases |

## Commands Executed

```bash
# Kill stuck worker
kill 2385498

# Reset stuck running jobs
docker exec axon-postgres psql -U axon -d axon -c \
  "UPDATE axon_ingest_jobs SET status='pending', started_at=NULL WHERE status='running';"

# Check job progress
docker exec axon-postgres psql -U axon -d axon -c \
  "SELECT target, result_json->>'phase', (result_json->>'files_done')::int, \
   (result_json->>'files_total')::int, (result_json->>'chunks_embedded')::int \
   FROM axon_ingest_jobs WHERE status='running' ORDER BY updated_at DESC;"

# Check TEI health
ssh steamy-wsl "docker logs tei_max --tail 30"

# Read real INFO logs
tail -f /home/jmagar/appdata/axon/logs/axon.log | grep "github\|ingest"

# Rebuild and restart all workers
just dev
```

## Behavior Changes (Before/After)

| Area | Before | After |
|---|---|---|
| `github clone_start` log | Silent — no log before `git clone` | `github clone_start repo=owner/name branch=main` at INFO |
| File embed progress | DB only (via `progress_tx`) | `github files_progress files_done=25 files_total=342 chunks_embedded=847` every 25 files at INFO |
| Batch flush | Silent | `github embed_batch_start batch_size=50` at INFO |
| Task completion | Silent until `log_done` at very end | `github task_done task=files repo=... chunks=N` per task at INFO |
| Pre-join log | Silent | `github tasks_start repo=... has_wiki=... include_source=...` at INFO |

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `cargo check -p axon` | Clean compile | `Finished dev profile` | ✓ PASS |
| `ssh steamy-wsl docker logs tei_max --tail 30` | Constant `Success` stream | 30 consecutive `Success` entries, inference 33–105ms | ✓ PASS |
| DB progress query | Active jobs with phase + file counts | 6 running jobs all `embedding_batch` with live counts | ✓ PASS |
| TEI retry check | No 429/503 errors | Zero retry entries in TEI logs | ✓ PASS |

## Risks and Rollback

- **Logging changes are additive** — no behavior change, only new `log_info` calls. Rollback: revert 3 files.
- **`just dev` kills all workers** — jobs were safely in-flight when killed; AMQP messages remained in queue; new worker reclaimed them via `reenqueue_orphaned_pending_jobs` on startup.
- **Manual SQL resets** — used `UPDATE ... SET status='pending'` without checking for in-flight AMQP acks. Safe because workers use `claim_next_pending` with `WHERE status='pending'` atomic claim.

## Decisions Not Taken

- **`just dev` vs targeted ingest worker restart**: Considered killing only the ingest worker and rebuilding just that binary. Chose `just dev` since we needed a rebuild anyway and it's cleaner.
- **Adding tree-sitter logging**: Deferred. URL evidence (`#L299-L311` line ranges) confirmed it's working without needing explicit log lines.
- **Adding per-file logging**: Would be too noisy for large repos (2000+ files). Every 25 files is the right granularity.

## Open Questions

- **embed timeout at 120s for rust-lang/cargo** (seen in logs at 04:39): These were from the TEI-down period. Need to confirm current runs have zero timeouts.
- **6 jobs from pre-restart era (agentclientprotocol, modelcontextprotocol, docker/mcp-community-registry)** stuck in `cloning` at 05:36: Watchdog should reclaim them. Confirm they eventually complete.

## Next Steps

- Add regression tests for ingest pipeline logging (tracing-test crate or behavior tests)
- Monitor `axon.log` during active ingest runs to confirm new log lines flowing
- Consider adding `chunk_code` fallback logging in `crates/vector/ops/input/code/` for tree-sitter hit/miss visibility
- Confirm 35 pending jobs drain cleanly with zero embed timeouts
