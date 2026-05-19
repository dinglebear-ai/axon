# Session: AMQP Worker Resilience — Crawl Worker Hardening
**Date:** 2026-02-28
**Branch:** `feat/crawl-download-pack`

---

## Session Overview

Diagnosed and fully resolved a production AMQP worker outage caused by RabbitMQ's `consumer_timeout` killing crawl worker channels after 30 minutes. Root cause was missing `basic_qos(1)` on the crawl consumer, causing RabbitMQ to buffer multiple unacked messages per lane. Implemented a complete hardening suite across both the crawl worker (`loops.rs`) and the shared worker lane infrastructure (`worker_lane.rs`): reconnect loop with exponential backoff, heartbeat/sweep continuity during long jobs via pinned `tokio::select!`, `futures_util::join_all` for dynamic lane spawning, and bumped `WORKER_CONCURRENCY` from 2 → 5.

---

## Timeline

| Time (UTC) | Activity |
|---|---|
| ~01:38 | RabbitMQ killed channel 1 on both crawl lanes (`PRECONDITION_FAILED`, consumer_timeout=1800000ms) |
| ~01:42 | Confirmed mjbizdaily.com Chrome crawl still running despite dead AMQP channels |
| Session start | User asked "dont we allow 2 concurrent crawls? How come mjbizdaily is the only one thats crawling currently" |
| +5 min | Diagnosed full incident: lane startup order, cannabissciencetech 4s completion, cannabiswire 18min Chrome crawl, AMQP buffer timeout |
| +10 min | Identified `basic_qos(1)` missing from crawl worker (already present in `worker_lane.rs`) — applied fix |
| +15 min | User: "Anything we should do to tighten this up even more?" — identified 3 structural gaps |
| +20 min | User: "dispatch agents to thoroughly systematically and completely address ALL of the issues" |
| +30 min | Agent 1 (worktree a3f320ff): implemented all three fixes to `loops.rs` |
| +30 min | Agent 2 (worktree a124e69c): audited `worker_lane.rs`, fixed sweep-during-saturation blocking and flat reconnect backoff |
| +35 min | Merged both worktrees: 439 tests passing, 0 warnings, 0 errors |
| +36 min | User: bump to 5 concurrent crawls — changed `WORKER_CONCURRENCY = 5`, replaced hardcoded `join!(lane1, lane2)` with `futures_util::join_all` |

---

## Key Findings

1. **Root cause of "only 1 crawl active"**: Both lanes started at 01:08:03. cannabissciencetech completed in 4 seconds. Lane 1 picked up cannabiswire (Chrome, 18 min). mjbizdaily was on lane 2 (Chrome, ongoing). Only mjbizdaily showed in logs because the other lane's jobs completed rapidly.

2. **AMQP channel death at 01:38:03**: Without `basic_qos(1)`, RabbitMQ buffered the next N messages into lapin's consumer stream. While lane 1 was blocked in `handle_crawl_delivery → process_job`, those buffered-but-unread messages sat unacked for exactly 30 minutes, triggering `consumer_timeout`. Both channels died simultaneously (`channel=1` in both error lines).

3. **`basic_qos(1)` already present in `worker_lane.rs`** (`worker_lane.rs:134`) — only the crawl worker `loops.rs` was missing it. Applied fix to `loops.rs:210` before dispatching agents.

4. **Sweep stops during saturation in `worker_lane.rs`**: When all semaphore permits were consumed, `inflight.next().await` blocked without a timeout, preventing the watchdog sweep from firing for the duration of a full-capacity burst.

5. **`worker_lane.rs` reconnect backoff was flat 2s** — no exponential growth. A persistent AMQP failure would reconnect-storm at 2s intervals indefinitely.

---

## Technical Decisions

- **`tokio::pin!` + `select!` over `tokio::spawn` for process_job**: `process_job` returns `Box<dyn Error>` which is `!Send`, blocking `tokio::spawn`. Pinning the future and polling it alongside tick futures in a `select!` loop achieves the same effect (heartbeats/sweeps fire during jobs) without requiring `Send` bounds.

- **`futures_util::join_all` over `tokio::join!` for dynamic lanes**: `tokio::join!` requires a compile-time fixed number of futures. `join_all` accepts a runtime-computed iterator of `!Send` futures, making lane count driven entirely by `WORKER_CONCURRENCY` with no code changes needed to add more.

- **`Arc<Config>` for lane ownership**: `run_amqp_lane_with_reconnect` needs an owned `Config` for the reconnect loop. Wrapped in `Arc` to avoid cloning the full struct per lane. `PgPool` cloned directly (Arc-backed internally, O(1)).

- **Exponential backoff 2s → 60s cap on reconnect**: Flat 2s would storm RabbitMQ during an outage. Cap at 60s prevents indefinite backoff growth while still being polite under sustained failures.

- **`WORKER_CONCURRENCY = 5`**: User-specified. The dynamic `join_all` approach means changing the constant is the only required change — no hardcoded lane list anywhere.

---

## Files Modified

| File | Change | Purpose |
|---|---|---|
| `crates/jobs/crawl/runtime.rs` | `WORKER_CONCURRENCY: 2 → 5` | 5 concurrent crawl slots |
| `crates/jobs/crawl/runtime/worker/loops.rs` | Full rewrite from agents | `basic_qos(1)`, reconnect loop, `run_job_with_ticks`, `LaneTimers`, `claim_delivery`, `join_all` for dynamic lanes |
| `crates/jobs/worker_lane.rs` | Sweep-during-saturation fix + exponential backoff | Watchdog keeps firing under full load; reconnect doesn't storm |

---

## Behavior Changes (Before / After)

| Behavior | Before | After |
|---|---|---|
| AMQP channel timeout | Channel killed after 30 min (buffered unacked messages) | `basic_qos(1)` prevents buffering; only 1 unacked message per lane at a time |
| Channel death recovery | Lane exits → `tokio::join!` waits for sibling → s6 restarts whole process | Reconnect loop with exp. backoff (2s→60s); lane reconnects autonomously |
| Heartbeat during long job | Stops firing (blocked in `handle_crawl_delivery`) | Fires every 60s via `run_job_with_ticks` `select!` |
| Watchdog sweep during job | Stops firing during long jobs | Fires per `STALE_SWEEP_INTERVAL_SECS` via pinned `select!` |
| Watchdog sweep under saturation (`worker_lane.rs`) | Blocked on `inflight.next().await` indefinitely | `select!` races drain against sweep deadline |
| Reconnect backoff (`worker_lane.rs`) | Flat 2s every attempt | Exponential 2s→4s→…→60s cap, reset on clean exit |
| Crawl lane count | Hardcoded 2 via `tokio::join!(lane1, lane2)` | Dynamic: `futures_util::join_all((1..=WORKER_CONCURRENCY).map(...))` |
| Concurrent crawl slots | 2 | 5 |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `cargo check --bin axon` (post basic_qos) | 0 errors | 0 errors, Finished | ✅ PASS |
| `cargo check --bin axon` (post merge) | 0 errors, 0 warnings | 0 errors, Finished | ✅ PASS |
| `cargo clippy --bin axon` (post merge) | 0 warnings | 0 warnings, Finished | ✅ PASS |
| `cargo test --lib` (post merge) | All pass | 439 passed, 0 failed | ✅ PASS |
| `cargo check --bin axon` (post WORKER_CONCURRENCY=5) | 0 errors | 0 errors, Finished | ✅ PASS |
| Agent 2 monolith check | No hard fails | `loops.rs` PASSED, `worker_lane.rs` PASSED (2 warnings <120 line limit) | ✅ PASS |

---

## Code Locations

- `basic_qos(1)` added: `loops.rs:210`
- `run_amqp_lane_with_reconnect`: `loops.rs:326` — infinite reconnect loop with exp. backoff
- `run_job_with_ticks`: `loops.rs` — pins process_job, fires heartbeat+sweep via `select!`
- `LaneTimers` struct: `loops.rs` — bundles interval references for `run_job_with_ticks`
- `claim_delivery`: `loops.rs` — parses/claims/acks a delivery, returns `Option<Uuid>`
- Dynamic lane spawn: `loops.rs:407` — `futures_util::join_all((1..=WORKER_CONCURRENCY).map(...))`
- `WORKER_CONCURRENCY`: `crates/jobs/crawl/runtime.rs:14`
- Sweep-during-saturation fix: `worker_lane.rs` — `select!` racing `inflight.next()` vs sweep interval

---

## Risks and Rollback

- **5 concurrent Chrome crawls**: Each Chrome crawl holds a CDP session. Chrome has a limited number of concurrent tab slots. Under `WORKER_CONCURRENCY=5` with all lanes hitting Chrome-required sites, we may saturate the Chrome container. Monitor `docker compose logs axon-chrome` for CDP slot exhaustion. If needed, reduce back to 3 or add a per-lane Chrome concurrency limit.
- **Higher Postgres contention**: 5 lanes × per-page progress writes at high page rates = more concurrent Postgres writes. Monitor `pg_stat_activity` if query times degrade.
- **Rollback**: `WORKER_CONCURRENCY: usize = 2` in `runtime.rs:14`. No schema changes, no infrastructure changes. `git revert` the 3 changed files if needed.

---

## Decisions Not Taken

| Option | Rejected Because |
|---|---|
| `tokio::spawn` for lane independence | `process_job` returns `Box<dyn Error>` which is `!Send` — spawn requires `Send + 'static` |
| `tokio::join!` for 5 fixed lanes | Compile-time macro, not runtime-configurable; changing concurrency would require code edits |
| Flat reconnect backoff | Storms RabbitMQ during outages; exponential is the right pattern |
| Removing `worker_lane.rs` reconnect and relying on s6 | s6 adds 1s minimum cooldown and reinitializes the DB pool; reconnecting within the lane is cheaper and faster |

---

## Open Questions

- Will 5 concurrent Chrome crawls saturate the `axon-chrome` CDP proxy? Chrome container management API is at port 6000; needs monitoring under load.
- Should `WORKER_CONCURRENCY` be configurable via env var (`AXON_CRAWL_CONCURRENCY`) rather than a compile-time constant? Currently requires code change + rebuild to adjust.
- The 54-job bulk crawl is partially complete — mjbizdaily.com was mid-crawl when the channel died. Its job status in DB is unknown (may still be `running`, may have been reclaimed by watchdog after rebuild).

---

## Next Steps

1. `docker compose up -d --build axon-workers` — rebuild with `WORKER_CONCURRENCY=5` and all resilience fixes
2. Monitor `docker compose logs -f axon-workers` for 5 simultaneous lane heartbeats and job starts
3. Watch Chrome container logs if multiple Chrome-required sites hit simultaneously
4. Consider adding `AXON_CRAWL_CONCURRENCY` env var to make lane count runtime-configurable without rebuild
5. Consider applying same `run_job_with_ticks` pattern to any future long-running non-crawl workers
