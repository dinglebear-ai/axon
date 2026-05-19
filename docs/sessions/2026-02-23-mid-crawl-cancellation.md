# Session: Mid-Crawl Cancellation

**Date:** 2026-02-23
**Branch:** fix-crawl
**Duration:** Single implementation session

---

## Session Overview

Implemented true mid-crawl cancellation for the axon crawl worker. Previously, calling `axon crawl cancel <id>` set the DB status to `canceled` and wrote a Redis key, but the running spider.rs crawl continued to completion — HTTP traffic to the target kept flowing for potentially many minutes. This session wired the Redis cancel key into the crawl runtime via `tokio::select!` so the crawl future is dropped within ~3 seconds of cancellation.

---

## Timeline

1. **Received implementation plan** — Detailed plan specifying exact changes to `worker_process.rs` only (no changes to engine.rs, job_context.rs, or spider API).
2. **Read target file** — `crates/jobs/crawl/runtime/worker/worker_process.rs` (429 lines pre-change).
3. **Implemented changes** — Three focused edits: imports, two new functions, one `tokio::select!` substitution.
4. **Ran `cargo check --bin axon`** — Confirmed no new errors introduced. Pre-existing compile error in `crates/ingest/sessions/gemini.rs:227` (unrelated to this session, pre-existing on the branch).
5. **Verified diff** — Clean diff confirmed changes isolated entirely to `worker_process.rs`.

---

## Key Findings

- `crates/ingest/sessions/gemini.rs:227` has a pre-existing type error (`Box<dyn Error>` vs `anyhow::Error`) from in-progress work on the branch — not introduced by this session.
- Spider's internal tokio tasks become orphans when the crawl future is dropped; they expire naturally via their own request timeouts (≤20s on `high-stable` profile). Acceptable.
- The `UPDATE WHERE status='running'` error handler in `process_job_impl` is a no-op when status is already `canceled` — DB correctness maintained with zero special-casing.
- `progress_tx` being dropped when the crawl future is dropped causes `progress_rx.recv()` to return `None`, so the progress task exits cleanly without a join error.

---

## Technical Decisions

- **`tokio::select!` over spawning a watcher task** — Keeps cancellation collocated with the crawl call, avoids shared state, and drop semantics handle cleanup automatically.
- **Fail-safe on Redis unavailability** — `is_crawl_canceled` returns `false` (not `true`) if Redis is unreachable. Crawl continues rather than silently canceling on transient infrastructure issues.
- **3-second poll interval** — Matches the pattern in `crates/jobs/embed/worker.rs::check_embed_canceled()` for consistency across job types.
- **Two 3-second timeouts inside `is_crawl_canceled`** — One for connection establishment, one for the GET command. Prevents the cancel poller from blocking longer than 6s per cycle even if Redis is slow.
- **`return Err(...)` inside the async block** — Returns from the `async { }` block expression (not the function), which propagates to `process_job_impl`'s error arm. The `WHERE status='running'` guard makes that a no-op.

---

## Files Modified

| File | Change | Purpose |
|------|--------|---------|
| `crates/jobs/crawl/runtime/worker/worker_process.rs` | Added imports, 2 new functions, modified `run_active_crawl_job` | Implement mid-crawl cancellation via Redis poll + `tokio::select!` |

**No other files were changed.**

---

## Code Changes

### Imports added (lines 11, 17)
```rust
use redis::AsyncCommands;
use std::time::Duration;
```

### New function `is_crawl_canceled` (lines 230–248)
```rust
async fn is_crawl_canceled(cfg: &Config, id: Uuid) -> bool {
    let Ok(client) = redis::Client::open(cfg.redis_url.clone()) else {
        return false;
    };
    let conn = tokio::time::timeout(
        Duration::from_secs(3),
        client.get_multiplexed_async_connection(),
    ).await;
    let Ok(Ok(mut conn)) = conn else { return false; };
    let key = format!("axon:crawl:cancel:{id}");
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        conn.get::<_, Option<String>>(&key),
    ).await;
    matches!(result, Ok(Ok(Some(_))))
}
```

### New function `poll_cancel_key` (lines 250–259)
```rust
async fn poll_cancel_key(cfg: &Config, id: Uuid) {
    loop {
        tokio::time::sleep(Duration::from_secs(3)).await;
        if is_crawl_canceled(cfg, id).await {
            return;
        }
    }
}
```

### Modified `run_active_crawl_job` (lines 378–384)
```rust
// Before:
let (summary, seen_urls) =
    run_primary_with_optional_chrome_fallback(ctx, id, progress_tx).await?;

// After:
let (summary, seen_urls) = tokio::select! {
    result = run_primary_with_optional_chrome_fallback(ctx, id, progress_tx) => result?,
    _ = poll_cancel_key(&ctx.job_cfg, id) => {
        log_info(&format!("crawl job {id} canceled mid-crawl; stopping"));
        return Err(format!("crawl job {id} canceled").into());
    }
};
```

---

## Behavior Changes (Before/After)

| Scenario | Before | After |
|----------|--------|-------|
| `axon crawl cancel <id>` while crawl running | DB → `canceled`, Redis key set. Spider crawl continues to natural completion (minutes). | DB → `canceled`, Redis key set. Crawl future dropped within ≤3s poll cycle. HTTP traffic stops. |
| No-op for backfill/embed after cancel | Embed job still enqueued (wrong) | Embed job never reached — `return Err` exits before `maybe_enqueue_embed_job` |
| Redis unreachable during cancel | N/A | `is_crawl_canceled` returns `false`, crawl continues (safe default) |
| Cancel key checked | Only once, before job starts (`maybe_cancel_job_before_start`) | Continuously every 3s throughout the crawl |

---

## Commands Executed

```bash
# Type check
cargo check --bin axon
# Result: 20 pre-existing warnings, 1 pre-existing error in gemini.rs (unrelated)
# No new errors from worker_process.rs changes

# Verify diff is isolated
git diff crates/jobs/crawl/runtime/worker/worker_process.rs
# Result: Clean — only the 3 intended hunks

# Confirm gemini.rs error is pre-existing
git stash -- crates/ingest/sessions/gemini.rs && cargo check --bin axon 2>&1 | grep "^error"
# Result: MORE errors without the gemini.rs changes — confirming they are in-progress work
git stash pop
```

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check --bin axon` | No new errors in `worker_process.rs` | 0 new errors (20 pre-existing warnings, 1 pre-existing error in `gemini.rs`) | ✅ PASS |
| `git diff worker_process.rs` | 3 hunks: imports, 2 new functions, select! | Exactly 3 hunks, no extraneous changes | ✅ PASS |
| Stash `gemini.rs`, recheck | Confirm its error is pre-existing | More errors without it → confirmed pre-existing | ✅ CONFIRMED |

---

## Risks and Rollback

**Risk:** If Redis has consistent latency >3s, each cancel poll cycle blocks for up to 6s. Poll interval effectively becomes 6-9s instead of 3s. Still bounded and acceptable.

**Risk:** Spider orphan tasks run briefly after the crawl future drops. Maximum duration = `request_timeout_ms` from the performance profile (20s for `high-stable`). Not a correctness issue, minor resource usage.

**Rollback:** Revert `crates/jobs/crawl/runtime/worker/worker_process.rs` to HEAD state. Single-file change, trivial to revert.

---

## Decisions Not Taken

- **Calling `website.stop()`** — Spider's `.stop()` method sets `shutdown = true` but would require passing the `Website` struct handle out of `engine.rs`. The plan explicitly avoids touching `engine.rs`; drop semantics on the future achieve the same effect within one request timeout cycle.
- **Using `tokio::spawn` + `AbortHandle`** — More complex, requires sharing the abort handle, and would not change the ~3s detection latency. `tokio::select!` is cleaner and idiomatic.
- **Polling on a separate connection pool** — The cancel check is infrequent (every 3s) and short-lived; opening a fresh connection per check is acceptable overhead and avoids connection lifecycle complexity.

---

## Open Questions

- The `gemini.rs` pre-existing type error (`Box<dyn Error>` vs `anyhow::Error` at line 227) needs to be resolved before the branch can compile clean. Not related to this session's work but blocks `cargo build`.
- Manual E2E verification (start long crawl → cancel → watch logs stop) has not been performed — requires running workers infrastructure.

---

## Next Steps

1. Fix `crates/ingest/sessions/gemini.rs:227` type mismatch (pre-existing, blocks clean build).
2. E2E smoke test: `axon crawl https://docs.rs --wait false` → `axon crawl cancel <id>` → confirm logs stop within ~3s.
3. Confirm `axon status` shows `canceled` (not `failed`) after mid-crawl cancel.
