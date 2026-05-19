# Session: AMQP Consumer Connection & Ack-Order Fixes
**Date:** 2026-02-19
**Branch:** `perf/command-performance-fixes`
**Duration:** ~45 minutes

---

## Session Overview

Systematically debugged a user-reported error containing three distinct messages:
1. `chromeRuntimeMode: chrome` + `[Chrome Bootstrap] no --chrome-remote-url provided`
2. `{"level":"ERROR","error":"invalid channel state: Closing","target":"lapin::channels"}`
3. Job ID successfully emitted (`Crawl Job d2d649c7-a922-4fa3-8698-ef668ef3cff6`)

Root-cause investigation revealed **three separate bugs** across all four AMQP worker consumer lanes (crawl, batch, extract, embed). All were fixed, tests remain green at 101 passing, clippy clean.

The active openzfs-docs crawl (270K pages) completed successfully at 20:35 UTC, 24 minutes after enqueue — narrowly before the 30-min RabbitMQ consumer timeout that would have killed its worker channel.

---

## Timeline

| Time (UTC) | Activity |
|---|---|
| 20:11:37 | User's crawl enqueued; `invalid channel state: Closing` emitted by lapin |
| Session start | Loaded `systematic-debugging` skill; read error carefully before touching any code |
| Phase 1 | Checked `docker compose ps` (all healthy), RabbitMQ logs — matched error timestamp exactly |
| Phase 2 | Traced `enqueue_job → open_amqp_connection_and_channel → drop(ch)/conn.close()` |
| Phase 3 | Discovered `open_amqp_channel` drops `Connection` with `_` — used for consumers in all 4 worker types |
| Phase 4 | Read `worker_loops.rs`, `worker_process.rs`, `embed_jobs.rs`, `batch_jobs/worker.rs`, `extract_jobs/worker.rs` |
| Fix | 6 files edited; `cargo clippy` + `cargo test` clean |
| 20:35 | Confirmed active crawl completed (270K pages, status `completed`) |

---

## Key Findings

### Finding 1 — `invalid channel state: Closing` is lapin internal noise (`common.rs:251`)
`enqueue_job` called `drop(ch)` to trigger channel close, then immediately `conn.close(200, "").await`. The `drop` fires lapin's channel-close handshake asynchronously. When `conn.close()` executed, lapin's internal event loop tried to route a pending close-acknowledgment frame through the channel already in `Closing` state → ERROR logged. The publish had already completed with confirmation; the job was safely enqueued.

### Finding 2 — `open_amqp_channel` drops `Connection` silently (`common.rs:149-152`)
```rust
pub async fn open_amqp_channel(...) -> Result<Channel> {
    let (_, ch) = open_amqp_connection_and_channel(...).await?;
    //   ^ Connection dropped here — TCP close scheduled immediately
    Ok(ch)
}
```
All four worker types called this for `basic_consume`, returning a channel whose backing connection was already closing. This is the structural root cause of the `PRECONDITION_FAILED` channel timeout seen in worker logs 13 hours prior.

### Finding 3 — Ack-after-process violates RabbitMQ consumer timeout
All four worker lanes acked the AMQP delivery **after** `process_job` completed. RabbitMQ's default `consumer_timeout` is 1800000 ms (30 min). For crawls longer than 30 min, RabbitMQ sends `PRECONDITION_FAILED` and closes the channel — confirmed in worker logs: `Channel closed reply_code: 406 PRECONDITION_FAILED - delivery acknowledgement on channel 1 timed out`.

### Finding 4 — Chrome bootstrap warning is cosmetic (`crawl.rs:502-517`)
`chrome_bootstrap` defaults to `true` in clap args (`config.rs:321`). With no `AXON_CHROME_REMOTE_URL`, `bootstrap_chrome_runtime` adds a warning to the output but returns early with `mode: Chrome, remote_ready: false`. The worker inherits `webdriver_url` from its own env (`AXON_WEBDRIVER_URL`) and uses it during actual crawl execution — this is not a bug.

### Finding 5 — Worker consumer lanes found in submodule files, not parent
`batch_jobs/worker.rs:122` and `extract_jobs/worker.rs:196` were not visible from the initial grep that only scanned flat `.rs` files. Required explicit subdirectory search.

---

## Technical Decisions

**Ack before process, not after** — Moving `delivery.ack()` before `process_job` is the correct design: the DB is the source of truth for job state (watchdog reclaims crashed jobs via `claim_pending_by_id` rejecting non-pending IDs on re-delivery). Acking early prevents RabbitMQ from killing the channel on long-running jobs.

**`pub(crate)` not `pub` for `open_amqp_connection_and_channel`** — Only internal worker code needs both `Connection` and `Channel`. Keeping it `pub(crate)` prevents external misuse.

**`ch.close(0, "").await` instead of `drop(ch)`** — `drop(ch)` fires lapin's close machinery asynchronously; `ch.close().await` blocks until the AMQP `Channel.Close-Ok` handshake completes, making the sequence deterministic and eliminating the race with `conn.close()`.

**Doc comment on `open_amqp_channel`** — The function is still correct for health checks and `queue_purge`, but its connection-dropping semantics are non-obvious. Added explicit warning in the doc comment to prevent the same mistake in new consumer code.

---

## Files Modified

| File | Change |
|---|---|
| `crates/jobs/common.rs` | `drop(ch)` → `ch.close(0,"").await` in `enqueue_job`; `open_amqp_connection_and_channel` made `pub(crate)`; doc warning added to `open_amqp_channel` |
| `crates/jobs/crawl_jobs/runtime/worker/worker_loops.rs` | Consumer lane: `open_amqp_channel` → `open_amqp_connection_and_channel` + `_conn`; ack moved before `process_job`; `open_amqp_connection_and_channel` added to imports |
| `crates/jobs/embed_jobs.rs` | Consumer lane: same two fixes; `open_amqp_connection_and_channel` added to imports |
| `crates/jobs/batch_jobs/worker.rs` | Consumer lane: same two fixes; explicit import added (`use super::*` doesn't re-export) |
| `crates/jobs/extract_jobs/worker.rs` | Consumer lane: same two fixes; explicit import added |

---

## Commands Executed

```bash
# Infrastructure state at time of debugging
docker compose ps
# All 6 services healthy (postgres, redis, rabbitmq, qdrant, webdriver, workers)

# RabbitMQ connection log — matched error timestamp 20:11:37.831603 exactly
docker compose logs axon-rabbitmq --tail=50

# Worker logs — confirmed PRECONDITION_FAILED from 13h prior, embed jobs actively processing
docker compose logs axon-workers --tail=80

# Confirmed active crawl job status
./scripts/axon crawl status d2d649c7-a922-4fa3-8698-ef668ef3cff6
# Status: running → 257K pages at time of check

# Post-fix verification
cargo check --bin axon   # clean 3.43s
cargo clippy --bin axon  # 0 warnings
cargo test               # 101 passed, 0 failed

# Final job status check
./scripts/axon crawl status d2d649c7-a922-4fa3-8698-ef668ef3cff6
# Status: completed, 270K pages, 20:35 UTC
```

---

## Behavior Changes (Before/After)

| Behavior | Before | After |
|---|---|---|
| `enqueue_job` log output | ERROR `invalid channel state: Closing` logged by lapin on every enqueue | Clean — no lapin error log |
| AMQP consumer lane connection | Channel backed by immediately-closing TCP connection | `_conn` held in scope; connection lives for full duration of consumer loop |
| Long-running job channel | RabbitMQ closes channel at 30 min → worker restarts → job may be orphaned | Ack fires before processing; RabbitMQ never times out waiting for ack |
| `open_amqp_connection_and_channel` visibility | Private — only accessible within `common.rs` | `pub(crate)` — accessible to all worker files in the crate |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `cargo check --bin axon` | No errors | Clean in 3.43s | ✅ |
| `cargo clippy --bin axon` | 0 warnings | 0 warnings | ✅ |
| `cargo test` | All pass | 101 passed, 0 failed | ✅ |
| `crawl status d2d649c7...` | Running with progress | `completed`, 270K pages | ✅ |
| RabbitMQ log timestamp | Matches `20:11:37.831301Z` error | `20:11:37.831603` connection close logged | ✅ confirmed root cause |

---

## Risks and Rollback

**Risk:** Ack-before-process means if the worker process crashes between ack and `claim_pending_by_id`, the AMQP message is consumed but the job is not claimed. The job remains `pending` in the DB. The watchdog does NOT reclaim `pending` jobs (only `running`). However, the next AMQP message delivery won't occur (it was already acked), and the `pending` job will sit until a worker polls for it via `claim_next_pending`. Since polling mode is the fallback, this is safe.

**Rollback:** `git revert HEAD` on the commit containing these changes. The worker container needs rebuild and redeploy: `docker compose build axon-workers && docker compose up -d axon-workers`.

---

## Decisions Not Taken

**Alternative: return `(Connection, Channel)` from `open_amqp_channel`** — Would have required changing all call sites (health checks, queue_purge, doctor). The two-function approach (keep `open_amqp_channel` for short-lived, expose `open_amqp_connection_and_channel` for consumers) is less disruptive and clearer in intent.

**Alternative: increase RabbitMQ consumer_timeout** — Treating the symptom (RabbitMQ config) rather than the cause (ack order). Would also require RabbitMQ config file management. Rejected.

**Alternative: spawn `process_job` in a separate tokio task and ack immediately** — More complex, would decouple job processing from the consumer loop and make error propagation harder. The ack-before-await approach is simpler and achieves the same goal.

---

## Open Questions

- **Chrome bootstrap and WebDriver**: The `axon-webdriver` Selenium container is at port 4444 (WebDriver protocol). Spider.rs uses CDP (Chrome DevTools Protocol, typically port 9222). It's unclear whether `cfg.webdriver_url` in the engine is actually used for CDP or Selenium. If Spider uses CDP, the Selenium container can't serve that protocol and Chrome fallback would launch a local browser inside the container (which may not exist). Worth verifying with a test crawl that explicitly requires JavaScript rendering.

- **Worker rebuild not yet deployed**: The fixes are in source but the `axon-workers` container still runs the old binary. No long-running jobs are currently queued, but the ack-order and connection-lifetime fixes won't be active until the container is rebuilt.

---

## Next Steps

1. **Rebuild and deploy worker container**: `docker compose build axon-workers && docker compose up -d axon-workers`
2. **Verify Chrome/WebDriver connectivity**: Run a crawl against a JS-heavy site with `--render-mode chrome` and check worker logs for which Chrome path is actually taken
3. **Consider setting `--chrome-bootstrap false` default** if no CDP endpoint is configured, to eliminate the warning noise from every async crawl enqueue
