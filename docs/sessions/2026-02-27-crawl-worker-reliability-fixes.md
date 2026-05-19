# Crawl Worker Reliability Fixes
**Date:** 2026-02-27
**Branch:** `feat/crawl-download-pack`

---

## Session Overview

Implemented four operational reliability fixes for the crawl worker, targeting issues discovered during live debugging:

1. **Watchdog Redis cancel signal on reclaim** — when the watchdog forcibly fails a stale job, it now sets the Redis cancel key so the in-flight worker process detects cancellation within one 3-second poll cycle (~3s vs the previous 10–15 minute natural crawl completion)
2. **Watchdog sweep gated to lane 1** — eliminates 4 of 5 redundant concurrent DB sweeps that fired every 30 seconds across all lanes
3. **Progress DB update throttle** — caps intermediate crawl progress writes at ~2/sec (previously unbounded — every collected page triggered a DB UPDATE)
4. **Post-review fix** — added connection timeout to the Redis signal helper to match the established codebase pattern

All changes passed `cargo check` clean compile and 440/440 unit tests.

---

## Timeline

| Time | Activity |
|---|---|
| Session start | Read plan from conversation, loaded files |
| T+5 min | Read 4 target files: `watchdog.rs`, `runtime.rs`, `loops.rs`, `process.rs`, `db.rs` |
| T+10 min | Implemented all 4 fixes sequentially |
| T+15 min | `cargo check` clean, 440 tests passing |
| T+20 min | Rust async patterns + best practices review via skills |
| T+22 min | Found and fixed missing timeout on Redis connect in `signal_reclaimed_cancel_keys` |
| T+25 min | Final `cargo check` + `cargo test --lib` green |

---

## Key Findings

- `WatchdogSweepStats` previously had `#[derive(Copy)]` with 3×u64 fields (24 bytes — right at the Copy size limit). Adding `reclaimed_ids: Vec<Uuid>` correctly forces `Copy` removal since `Vec<T>` heap-allocates.
- The startup sweep in `run_worker` calls `reclaim_stale_running_jobs` directly (not through `run_watchdog_sweep`), so the lane=1 gate does **not** affect it. Correct — previous container's stale jobs are reclaimed at startup before any in-flight processes exist.
- `signal_reclaimed_cancel_keys` was originally missing a connection timeout on `get_multiplexed_async_connection()`. The `connect_cancel_redis` function in `process.rs:276` already established the pattern: 3-second `tokio::time::timeout` wrapper. Fixed post-review.
- The 500ms progress throttle: first message processed only after ≥500ms from task spawn. The final result JSON is written separately by `process_job_impl` (not via the progress channel), so throttling intermediate progress is safe per the plan.

---

## Technical Decisions

**Why lane 1 gate instead of a `Mutex<last_sweep_time>`?**
The `if lane != 1 { return; }` guard is zero-overhead and eliminates N-1 redundant sweeps with no shared state. A distributed lock would add complexity with no benefit since all lanes share the same PostgreSQL pool — one SELECT/UPDATE is enough.

**Why a new Redis client per sweep in `signal_reclaimed_cancel_keys`?**
Reclaims are rare (only stale jobs) and the function is called at most every 30s from lane 1 only. Holding a long-lived Redis connection for a function that rarely executes is wasteful. The comment added: "A fresh client is created per sweep to avoid holding a long-lived connection across idle periods."

**Why `set_ex` with 24h TTL?**
Matches `db.rs:cancel_job()` exactly (line 241). The cancel key is consumed by `poll_cancel_key` in `process.rs` on the next 3-second poll cycle. 24h ensures the key persists if the process dies before consuming it.

**Why `Duration::from_millis(500)` for progress throttle?**
The plan specified ≤2/sec. 500ms → 2 writes/sec per running job. At 5 concurrent crawls this caps at 10 progress writes/sec total vs the previous ~250+/sec on fast sites.

---

## Files Modified

| File | Change |
|---|---|
| `crates/jobs/common/watchdog.rs` | Removed `Copy` derive from `WatchdogSweepStats`; added `reclaimed_ids: Vec<Uuid>`; push `id` to `reclaimed_ids` when `rows_affected > 0` |
| `crates/jobs/crawl/runtime.rs` | Removed `Copy` derive from `CrawlWatchdogSweepStats`; added `reclaimed_ids: Vec<Uuid>` |
| `crates/jobs/crawl/runtime/worker/loops.rs` | Added `use redis::AsyncCommands`; propagated `reclaimed_ids` in wrapper; added `signal_reclaimed_cancel_keys()` helper; added lane=1 gate to `run_watchdog_sweep`; added `redis_url: &str` param; updated 3 callers; added 3s timeout on Redis connect (post-review fix) |
| `crates/jobs/crawl/runtime/worker/process.rs` | Added `last_update: Instant` + 500ms elapsed guard in `spawn_progress_task` |

---

## Commands Executed

```bash
# Compile check
cargo check --bin axon
# Result: Finished `dev` profile in 0.80s

# Test suite
cargo test --lib
# Result: test result: ok. 440 passed; 0 failed; 3 ignored

# Post-review fix compile check
cargo check --bin axon
# Result: Finished in 1.19s

# Post-review tests
cargo test --lib
# Result: test result: ok. 440 passed; 0 failed
```

---

## Behavior Changes (Before/After)

| Behavior | Before | After |
|---|---|---|
| Watchdog-reclaimed job continues crawling | Yes — in-flight process has no signal, continues for 10–15min | No — Redis cancel key set within 3s; `poll_cancel_key` detects it on next poll |
| Watchdog sweep concurrency | 5 sweeps fire simultaneously every 30s | Only lane 1 sweeps; 4 of 5 are no-ops |
| Watchdog sweep SQL load | 5× SELECT + 5× UPDATE per sweep window | 1× SELECT + 1× UPDATE per sweep window |
| Progress DB writes | Every page collected → DB UPDATE (unbounded) | ≤2 DB UPDATEs/sec per crawl job |
| `WatchdogSweepStats` type | `Copy` + 3×u64 | Non-Copy + 3×u64 + `Vec<Uuid>` |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `cargo check --bin axon` | Clean compile | `Finished dev profile in 0.80s` | ✅ Pass |
| `cargo test --lib` | 440 pass, 0 fail | `440 passed; 0 failed` | ✅ Pass |
| `cargo check` post-review fix | Clean compile | `Finished in 1.19s` | ✅ Pass |
| `cargo test --lib` post-review | 440 pass, 0 fail | `440 passed; 0 failed` | ✅ Pass |

---

## Source IDs + Collections Touched

None — this session involved no Axon embed/retrieve/crawl operations.

---

## Risks and Rollback

**Risk:** Lane 1 AMQP reconnect backoff (up to 60s) pauses sweeps for up to 60s. Acceptable — stale timeout window is 300s + 60s confirm = 360s grace, so 60s sweep gap is within tolerance.

**Risk:** `signal_reclaimed_cancel_keys` creates a new Redis connection per sweep. If Redis is unavailable, the warning is logged and the function returns without crashing. No regression from previous behavior (watchdog never signaled Redis before).

**Rollback:** All changes are in 4 files. Revert via:
```bash
git diff HEAD~1 -- crates/jobs/common/watchdog.rs crates/jobs/crawl/runtime.rs \
  crates/jobs/crawl/runtime/worker/loops.rs crates/jobs/crawl/runtime/worker/process.rs
git checkout HEAD~1 -- <file>
```

---

## Decisions Not Taken

- **Shared Redis connection in `signal_reclaimed_cancel_keys`**: Would require passing `Arc<Mutex<Connection>>` or storing it on a struct. Overkill for a rarely-called function — rejected in favor of per-sweep fresh client.
- **Signaling Redis on startup reclaim**: The startup sweep path (`run_worker` lines 368–385) was left unchanged. Startup reclaims target jobs from dead previous container instances, not in-flight processes — no process to cancel.
- **Redis pipeline for batch `set_ex`**: Would optimize multi-job reclaim batches. Reclaim batches are typically 1–3 jobs; sequential writes are fast enough. Rejected to keep code simple.
- **`tokio::spawn` for `signal_reclaimed_cancel_keys`**: Would fully decouple it from the sweep timer. Rejected — the 3-second timeout already bounds the latency; spawning would require cloning `redis_url` as `String` and adds task overhead.

---

## Open Questions

- Should the startup sweep in `run_worker` also signal Redis cancel keys for reclaimed IDs? Currently it does not, on the rationale that the previous process is dead. If a split-brain scenario is possible (two worker containers running briefly), this could miss a live process.
- The `WatchdogSweepStats.reclaimed_ids` field is now also present in `generic_reclaim` (the non-crawl path used by embed/extract workers). Those callers don't use `reclaimed_ids` — should it be scoped to crawl only? Currently it's on the shared struct for uniformity.

---

## Next Steps

- Deploy: `docker compose build --no-cache axon-workers && docker compose up -d axon-workers`
- Verify sweep logs show `lane=1` only after deployment
- Queue 5+ concurrent crawls and confirm all 5 lanes pick up jobs
- Observe that a watchdog-reclaimed job's cancel key appears in Redis within ~3s of reclaim
