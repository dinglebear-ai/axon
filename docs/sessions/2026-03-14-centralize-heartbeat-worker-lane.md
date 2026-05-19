# Session: Centralize Job Heartbeat in worker_lane

**Date:** 2026-03-14
**Branch:** main
**Commit:** `a8fae674`

---

## Session Overview

Implemented the pre-written plan "Centralize Job Heartbeat in `worker_lane` Implementation Plan". Moved `spawn_heartbeat_task` out of individual job workers (embed, extract, refresh, ingest) and into `worker_lane.rs` so every job processed by `run_job_worker` gets an automatic heartbeat. Fixed a silent bug where the graph worker had no heartbeat at all.

---

## Timeline

1. **Plan loaded** — Pre-existing plan at `/home/jmagar/.claude/plans/tranquil-wandering-elephant.md` identified the 7-task implementation.
2. **Task 1** — Added `heartbeat_interval_secs: u64` to `WorkerConfig` struct. Fixed all construction sites in embed, extract, refresh, ingest, and graph workers.
3. **Task 2** — Implemented `wrap_with_heartbeat()` in `worker_lane.rs`. Wired it into `run_job_worker` after semaphore creation. Added two unit tests.
4. **Tasks 3–6** — Removed manual `spawn_heartbeat_task` calls from embed, extract, refresh/processor, and ingest workers.
5. **Task 7** — Full verification: `cargo check`, `cargo test --lib`, `cargo clippy`. All passed. Commit landed as `a8fae674`.

---

## Key Findings

- **Graph worker had NO heartbeat** (`crates/jobs/graph/worker.rs`) — silent bug pre-existing. Now fixed automatically via `run_job_worker`.
- **Refresh heartbeat was deeply embedded** in `setup_refresh_job_context` return type (tuple of 5 elements including `watch::Sender<bool>` and `JoinHandle<()>`) and in `finalize_refresh_job` parameters. Required careful surgery across 5 call sites in `processor.rs`.
- **`PgPool::connect_lazy`** (sqlx 0.8) allows test pools without real DB connections. The heartbeat task's first `touch_running_job` call fails silently (`let _ = ...`), making the wrapper testable without infrastructure.
- **Pre-commit hook exit code 1** was a false alarm — the hooks actually passed (check, clippy, 1297 tests) but the shell timed out capturing output. The commit `a8fae674` landed successfully.

---

## Technical Decisions

- **`wrap_with_heartbeat` as a `ProcessFn → ProcessFn` wrapper** rather than a free function called inside `run_job_worker` directly. This keeps `run_job_worker` clean and makes the heartbeat composable — future wrapping (e.g., tracing spans) can layer on top.
- **One-line wire-up in `run_job_worker`** — `let process_fn = wrap_with_heartbeat(process_fn, wc.table, wc.heartbeat_interval_secs);` immediately after semaphore creation, before the AMQP/polling split. All paths get the heartbeat.
- **Crawl worker excluded by design** — crawl uses `crawl/runtime/worker/loops.rs` with `!Send` spider futures, not `run_job_worker`. Its manual heartbeat in `process.rs` is correct and intentional.
- **`INGEST_HEARTBEAT_INTERVAL_SECS` moved** from `ingest/process.rs` to `ingest.rs` (the module root that constructs `WorkerConfig`). Keeps the constant co-located with the struct that uses it.
- **`GRAPH_HEARTBEAT_INTERVAL_SECS: u64 = 30`** added as a new constant in `graph/worker.rs` — matches the ingest/extract interval since graph jobs can be long-running LLM extraction tasks.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/jobs/worker_lane.rs` | Added `spawn_heartbeat_task` to imports; added `heartbeat_interval_secs: u64` to `WorkerConfig`; added `wrap_with_heartbeat()` function; wired into `run_job_worker` |
| `crates/jobs/worker_lane/tests.rs` | Added `worker_config_has_heartbeat_interval` (sync) and `wrap_with_heartbeat_calls_inner_fn` (async) tests |
| `crates/jobs/embed/worker.rs` | Removed `spawn_heartbeat_task` import and manual spawn/stop block; added `heartbeat_interval_secs: EMBED_HEARTBEAT_INTERVAL_SECS` to `WorkerConfig` literal |
| `crates/jobs/extract/worker.rs` | Same pattern as embed; `EXTRACT_HEARTBEAT_INTERVAL_SECS` stays (still used in WorkerConfig) |
| `crates/jobs/refresh/processor.rs` | Removed `spawn_heartbeat_task` and `REFRESH_HEARTBEAT_INTERVAL_SECS` from imports; changed `setup_refresh_job_context` return type from 5-tuple to 3-tuple; removed heartbeat params from `finalize_refresh_job`; updated 2 early-exit paths |
| `crates/jobs/refresh/worker.rs` | Added `REFRESH_HEARTBEAT_INTERVAL_SECS` to imports from `super`; added `heartbeat_interval_secs` to `WorkerConfig` literal |
| `crates/jobs/ingest/process.rs` | Removed `spawn_heartbeat_task` import; removed `INGEST_HEARTBEAT_INTERVAL_SECS` const; removed heartbeat spawn/stop around the `match &job_cfg.source` block |
| `crates/jobs/ingest.rs` | Added `const INGEST_HEARTBEAT_INTERVAL_SECS: u64 = 30;`; added `heartbeat_interval_secs` to `WorkerConfig` literal |
| `crates/jobs/graph/worker.rs` | Added `const GRAPH_HEARTBEAT_INTERVAL_SECS: u64 = 30;`; added `heartbeat_interval_secs` to `WorkerConfig` literal |

---

## Commands Executed

```bash
# Verification runs
cargo check --bin axon          # clean (22s)
cargo test --lib                # 1292 passed, 0 failed, 5 ignored
cargo clippy -- -D warnings     # 0 warnings

# Targeted verification
grep -rn "spawn_heartbeat_task" crates/jobs/embed/ crates/jobs/extract/ crates/jobs/refresh/ crates/jobs/ingest/
# → no matches (manual calls removed)

grep -n "spawn_heartbeat_task" crates/jobs/crawl/runtime/worker/process.rs
# → 2 matches (manual call preserved)

grep "heartbeat_interval_secs" crates/jobs/graph/worker.rs
# → 1 match in WorkerConfig literal

cargo test --lib -- worker_lane::tests
# → 16 tests passed (including 2 new: worker_config_has_heartbeat_interval, wrap_with_heartbeat_calls_inner_fn)
```

---

## Behavior Changes (Before/After)

| Worker | Before | After |
|--------|--------|-------|
| embed | Manual `spawn_heartbeat_task` in `process_embed_job` | Automatic via `wrap_with_heartbeat` in `run_job_worker` |
| extract | Manual `spawn_heartbeat_task` in `process_extract_job` | Automatic via `wrap_with_heartbeat` |
| refresh | Manual `spawn_heartbeat_task` in `setup_refresh_job_context` | Automatic via `wrap_with_heartbeat` |
| ingest | Manual `spawn_heartbeat_task` in `process_ingest_job` | Automatic via `wrap_with_heartbeat` |
| graph | **NO heartbeat** (silent bug) | Automatic via `wrap_with_heartbeat` — bug fixed |
| crawl | Manual in `process.rs` (own loop) | Unchanged — correct by design |

Graph jobs can now run for extended periods without being marked stale by the watchdog — previously they would time out after `AXON_JOB_STALE_TIMEOUT_SECS` (default 300s).

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | clean | clean | ✅ |
| `cargo test --lib` | 1292+ pass, 0 fail | 1292 passed, 0 failed | ✅ |
| `cargo clippy -- -D warnings` | 0 warnings | 0 warnings | ✅ |
| `grep spawn_heartbeat_task crates/jobs/{embed,extract,refresh,ingest}/` | no matches | no matches | ✅ |
| `grep spawn_heartbeat_task crates/jobs/crawl/runtime/worker/process.rs` | 2 matches | 2 matches | ✅ |
| `cargo test -- worker_lane::tests` | 16 pass | 16 passed | ✅ |
| `git log --oneline -1` | commit `a8fae674` | `a8fae674 feat(jobs): centralize heartbeat in worker_lane via wrap_with_heartbeat` | ✅ |

---

## Source IDs + Collections Touched

None — this session was a pure code refactor with no embedding operations.

---

## Risks and Rollback

- **Risk:** If `spawn_heartbeat_task` has latency (e.g., first DB touch is slow), it now adds latency before the inner `ProcessFn` starts. Mitigation: `spawn_heartbeat_task` uses `tokio::spawn` — the task is launched in background immediately, the inner future starts without waiting for the first tick.
- **Risk:** Heartbeat interval mismatch if a worker constructs `WorkerConfig` with wrong `heartbeat_interval_secs`. Mitigation: compiler enforces the field — cannot be omitted.
- **Rollback:** `git revert a8fae674` — reverts all 9 files atomically. Would need to manually re-add the heartbeat constants removed from `ingest/process.rs`.

---

## Decisions Not Taken

- **Put `wrap_with_heartbeat` in `common/job_ops.rs`** — rejected; it depends on `ProcessFn` which is defined in `worker_lane.rs`, creating a circular dependency.
- **Make `heartbeat_interval_secs: Option<u64>`** — rejected; every runner needs heartbeat. An `Option` would allow new workers to silently opt out, recreating the original problem.
- **Use `tokio::time::interval` instead of loop+sleep in `spawn_heartbeat_task`** — not changed in this session; `spawn_heartbeat_task` pre-exists and its internals were not part of the scope.

---

## Open Questions

- The `setup_refresh_job_context` function in `refresh/processor.rs` is 87 lines (above the 80-line soft warning, below the 120-line hard limit). The pre-commit monolith hook warns but does not fail. Consider splitting it in a follow-up if it grows.

---

## Next Steps

- Monitor graph worker jobs in production — first time they'll have a heartbeat. Watch for unexpected behavior from the new `touch_running_job` calls during graph extraction.
- Consider documenting `wrap_with_heartbeat` in `crates/jobs/CLAUDE.md` as part of the worker lane contract.
