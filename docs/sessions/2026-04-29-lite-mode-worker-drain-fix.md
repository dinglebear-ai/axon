# Lite Mode Worker Drain Fix

**Date:** 2026-04-29  
**Branch:** main  
**Version:** v0.35.1

---

## Session Overview

Debugged and fixed three cascading issues in lite mode (`AXON_LITE=1`):

1. Crawl jobs stuck as "pending" forever — no workers processing them
2. `axon crawl worker` silently doing nothing despite printing success
3. `axon crawl <url>` completing immediately with a job ID but the job never running

Root cause: lite mode workers (tokio tasks) are only spawned when explicitly needed; fire-and-forget CLI commands never spawned workers, and the drain mechanisms were either stubs or only gated on `--wait`.

Additionally fixed a stack overflow crash loop in `axon serve` caused by `pulse_chat_probe` running `establish_acp_session` on a 2MB tokio worker thread.

---

## Timeline

1. **Identified stuck pending jobs** — `axon status` showed two crawl jobs (`6b063438`, `a51ebc9d`) permanently pending with no worker consuming them
2. **Fixed `axon crawl worker`** — `LiteServiceRuntime::run_worker` was a stub returning `Ok(WorkerMode::InProcess)` without doing anything; replaced with real drain loop
3. **Fixed misleading worker message** — changed "Lite mode: workers run in-process automatically. No separate worker needed." to "Lite mode: queue drained."
4. **Added `notify_worker` to `LiteBackend`** — workers field is private; needed a public method to wake a specific worker type
5. **Fixed serve stack overflow** — `pulse_chat_probe` called `tokio::spawn` (2MB worker thread) for `establish_acp_session` which has a deeply-nested future; fixed by adding `thread_stack_size(8 MiB)` to tokio runtime builder in `main.rs`
6. **Fixed fire-and-forget crawl in lite mode** — mirrored MCP server pattern: always spawn workers for async job commands in lite mode; after `run_once` enqueues, drain the queue before exit
7. **Rust code review** — identified embed jobs orphaning bug (fire-and-forget drain only waits for crawl, not the embed jobs spawned by the crawl worker)
8. **Added drain progress output** — `run_worker` now prints "draining axon_crawl_jobs queue..." on start and periodic "still draining (Ns elapsed)..." heartbeats

---

## Key Findings

- **`LiteBackend::enqueue` already notifies workers** (`lite.rs:124-133`) — double-notify in `run_worker` is harmless
- **`run_crawl_job_lite` spawns embed jobs** (`workers/runners.rs:50-72`) via direct `ops::enqueue_job` call (bypasses `LiteBackend::enqueue`, so embed worker is NOT notified — relies on 5s `POLL_INTERVAL`)
- **MCP server always calls `new_with_workers`** (`mcp/server.rs:83`) — workers are lazy-initialized on first request and stay alive for the server lifetime; CLI should follow the same pattern
- **`WorkerHandles::drop` aborts all tasks** (`workers.rs:48-54`) — process exit kills all workers; this is why fire-and-forget is fundamentally broken without explicit drain

---

## Technical Decisions

### "Always spawn workers for async job commands in lite mode" vs "spawn only when --wait"

Chose to always spawn workers (mirroring MCP server), then drain after `run_once`. Previous approach gated on `--wait` which meant fire-and-forget left jobs stranded. The MCP server analogy made the correct pattern obvious.

### Drain after `run_once`, not inside command handlers

The drain is added in `lib.rs` after `run_once` returns, not inside each command handler. This keeps the command handlers unchanged and the drain logic in one place.

### `wait = true` mutation rejected

An earlier attempt forced `cfg.wait = true` in lite mode before passing config to handlers. Rejected because it mutated config state and routed crawl through `sync_crawl` (bypassing the job queue entirely), losing job visibility. The correct approach spawns workers and drains post-enqueue.

---

## Files Modified

| File | Change |
|------|--------|
| `main.rs` | Added `thread_stack_size(8 MiB)` to tokio runtime builder |
| `lib.rs` | Added `command_to_job_kind()`, changed `needs_workers` to always spawn for async job commands in lite mode, added post-enqueue drain loop, updated `log_done` logic |
| `crates/jobs/lite.rs` | Added `notify_worker(kind: JobKind) -> bool` public method |
| `crates/services/runtime.rs` | Replaced stub `run_worker` with real drain loop + progress output |
| `crates/cli/commands/common_jobs.rs` | Fixed misleading `WorkerMode::InProcess` message |

---

## Behavior Changes (Before/After)

| Scenario | Before | After |
|----------|--------|-------|
| `axon crawl <url>` (lite mode) | Job enqueued, never processed; prompt returns immediately | Workers spawned; job processes; prompt returns after completion |
| `axon crawl worker` (lite mode) | Printed "Lite mode: workers run in-process automatically. No separate worker needed." and exited immediately | Prints "draining axon_crawl_jobs queue...", processes all pending jobs, exits with "Lite mode: queue drained." |
| `axon crawl worker` (no workers running) | Same misleading no-op | Returns error: "no in-process workers running — use `axon serve` or `--wait true`" |
| `axon serve` | Crash loop: `thread 'tokio-rt-worker' has overflowed its stack` | Runs cleanly; `establish_acp_session` future fits in 8MB worker thread stack |
| Job management subcommands (`status`, `list`, etc.) | Workers not spawned (fast) | Workers not spawned (unchanged, fast) |

---

## Open Issues / Remaining Bugs

### Embed Jobs Orphaned After Crawl Drain (Major)

**Location:** `lib.rs:259-263`, root cause at `workers/runners.rs:50-72`

`run_crawl_job_lite` calls `ops::enqueue_job` directly to spawn embed jobs. This bypasses `LiteBackend::enqueue`, so the embed worker is not notified and relies on the 5s poll interval. The fire-and-forget drain loop calls `run_worker(JobKind::Crawl)` and exits when `has_active_jobs(Crawl)` returns false — but at that point, embed jobs are in 'pending' state and the embed worker hasn't been woken. Process exit kills the embed worker.

**Fix needed in `lib.rs`:**
```rust
if cfg.lite_mode && is_async_enqueue_mode(cfg) {
    if let Some(kind) = command_to_job_kind(cfg.command) {
        let _ = service_context.jobs.run_worker(kind).await;
        // Crawl runner auto-enqueues embed jobs — drain those too before exit
        if kind == JobKind::Crawl && cfg.embed {
            let _ = service_context.jobs.run_worker(JobKind::Embed).await;
        }
    }
}
```

This works because: crawl job enqueues embed job before `mark_completed` → `has_active_jobs(Crawl)` goes false AFTER embed job is in DB → subsequent `run_worker(Embed)` notifies embed worker and waits for it.

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check` after all changes | Clean compile | "1 crates compiled" (no errors) | ✅ |
| `axon crawl worker` on 2 pending jobs | Process jobs + print "queue drained" | Processed 3 jobs (2 pending + 1 sitemap-spawned) + exited cleanly | ✅ |
| Dead code warning after removing `is_worker_subcommand` | Warning gone after deletion | Removed, clean compile | ✅ |

---

## Decisions Not Taken

- **`--wait = true` mutation in lite mode** — bypasses job queue, loses job visibility, routes through `sync_crawl` which is a different code path
- **`is_async_enqueue_mode` in `needs_workers`** — workers spawned but process exits after `run_once` before workers finish (original "first wrong fix")
- **Spawning a detached child process** — overly complex; tokio tasks in the same process with explicit drain is simpler and correct

---

## Next Steps

1. **Fix embed jobs drain** — add `run_worker(JobKind::Embed)` after crawl drain in `lib.rs` (see Open Issues above)
2. **Test full crawl+embed flow** — run `axon crawl <url>` in lite mode and verify `axon query` returns results after completion
3. **Consider cron mode** — in `lib.rs`, the cron path returns early before the drain block; cron + lite mode + async command will process jobs via the running workers but the process must stay alive between iterations (currently OK since workers run throughout)
