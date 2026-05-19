# Session: AMQP Consumer Timeout + Embed Pipeline Fixes

**Date**: 2026-03-16
**Branch**: `feat/pulse-shell-and-hybrid-search`
**Commit**: `e2362a68`
**Version**: `0.25.2` → `0.25.3`

---

## Session Overview

Diagnosed and fixed four production bugs affecting the ingest embed pipeline:

1. **AMQP consumer_timeout** — saturation path never polled `consumer.next()`, leaving QoS=1 deliveries unacked in lapin's buffer until RabbitMQ closed the channel after 1800s with `PRECONDITION_FAILED - delivery acknowledgement on channel 1 timed out`
2. **`doc_concurrency` clamp wrong** — intended `min(CPUs, 8)` fix was written as `clamp(2, 16)`, so 12-CPU machines still ran 12 concurrent TEI embed workers instead of 8, causing thundering herd on the 8-permit semaphore
3. **`qdrant_upsert` no-retry** — bare `send().await?.error_for_status()?` with zero fault tolerance on transient Qdrant errors
4. **AST chunking observability gap** — no console-visible log at start of tree-sitter collection phase; the process appeared hung for minutes between embed waves

All fixes pushed together as a single commit (1338 tests passing, 0 failures).

---

## Timeline

| Time | Activity |
|------|----------|
| Session start | User asked about logging/observability into AST chunking phase |
| Investigation | Identified `collect_and_embed_batched` had no `collect_start` log; every-25-file progress logs existed but were INFO-only (file only, not console) |
| Fix 1 | Added `log_info` collect_start banner to `batch.rs` |
| Fix 2 | Corrected `doc_concurrency` clamp from `(2, 16)` to `(2, 8)` in `pipeline.rs` |
| Fix 3 | Added 3-attempt retry with exponential backoff to `qdrant_upsert` in `qdrant_store.rs` |
| Fix 4 (main) | Added `handle_saturation_delivery()`, `VecDeque<Uuid>` pre-ack queue, `consumer.next()` arm to saturation `select!`, and re-enqueue-on-exit in `amqp.rs`; added `claim_preacked_job()` in `delivery.rs` |
| `cargo check` | Caught `mismatched types` — `d.ack()` returns `Result<bool, _>`, not `Result<(), _>`; fixed `Ok(())` → `Ok(_)` |
| `/quick-push` | Version bump, changelog, commit, push |

---

## Key Findings

### AMQP Consumer Timeout Root Cause

- `run_amqp_lane` in `amqp.rs`: when `semaphore.available_permits() == 0 && !inflight.is_empty()`, the saturation `select!` only had two arms: `inflight.next()` and `sweep_interval.tick()`
- With `basic_qos(1)`, RabbitMQ pushes the next delivery the moment the previous one is acked — that delivery arrived in lapin's internal buffer and sat **unacked** for the entire duration of the running job (potentially 30+ minutes)
- After 1800s RabbitMQ closes the channel: `PRECONDITION_FAILED - delivery acknowledgement on channel 1 timed out`
- The `nack` pattern (requeue) was considered but rejected: it creates a tight loop where the job is immediately redelivered and nacked again while the lane is still saturated

### `doc_concurrency` Clamp Bug

- `pipeline.rs:274`: `CPUs.clamp(2, 16)` — with 12 CPUs, `12.clamp(2, 16) = 12`
- Intended fix was `min(CPUs, 8)` = `CPUs.clamp(2, 8)` — one character change
- With 12 docs × 12 lanes = 144 concurrent embed attempts competing for the 8-permit semaphore, combined with 300s doc timeout, this was the root cause of `embed_pipeline completed with 12/xxx doc failures`

### Pre-Ack Pattern

Selected over nack because:
- No redelivery loop risk during saturation
- RabbitMQ unacked slot cleared immediately (prevents consumer_timeout)
- UUID stored in `VecDeque<Uuid>` and processed when permits free
- On lane exit, unprocessed UUIDs re-enqueued via `batch_enqueue_jobs` so watchdog doesn't have to wait

---

## Technical Decisions

| Decision | Rationale |
|----------|-----------|
| Pre-ack + VecDeque vs nack + requeue | Nack creates tight redelivery loop during saturation; pre-ack clears the unacked slot cleanly |
| `Ok(_)` not `Ok(())` in ack match | `lapin::BasicAck.ack()` returns `Result<bool, lapin::Error>`, not `Result<(), _>`; compiler enforces this |
| 3 retries, 500ms/1000ms backoff for qdrant_upsert | Matches TEI retry pattern; conservative enough to avoid thundering herd on Qdrant |
| Re-enqueue pre-acked on exit (best-effort) | Without this, jobs in VecDeque wait the full stale timeout (300s + 60s) before watchdog reclaims them |
| `collect_start` log at INFO level | Matches existing log level convention; user was told to tail `axon.log` for INFO-level output |

---

## Files Modified

| File | Change |
|------|--------|
| `crates/vector/ops/tei/pipeline.rs:274` | `clamp(2, 16)` → `clamp(2, 8)` |
| `crates/ingest/github/files/batch.rs:32-37` | Added `log_info` collect_start banner with `files_total`, `batch_concurrency`, `embed_batch_size` |
| `crates/vector/ops/tei/qdrant_store.rs:329-364` | Replaced bare `send().await?.error_for_status()?` with 3-attempt retry loop (500ms → 1000ms backoff) |
| `crates/jobs/worker_lane/delivery.rs` | Added `claim_preacked_job()` function (no delivery to ack, acquires semaphore, claims DB row, returns job future) |
| `crates/jobs/worker_lane/amqp.rs` | Added `handle_saturation_delivery()`, `VecDeque<Uuid>` drain loop, `consumer.next()` arm in saturation `select!`, re-enqueue block on exit |
| `Cargo.toml` | `0.25.2` → `0.25.3` |
| `CHANGELOG.md` | Added v0.25.3 section |

---

## Commands Executed

```bash
# Type check after edits
cargo check -p axon

# Compilation error found and fixed:
# error[E0308]: mismatched types
#  --> crates/jobs/worker_lane/amqp.rs:184:20
#   | Ok(()) -- expected bool, found ()
# Fix: Ok(()) → Ok(_)

# Pre-commit hook (1338 tests, 0 failures)
cargo test

# Push
git push  # → e2362a68
```

---

## Behavior Changes (Before / After)

| Surface | Before | After |
|---------|--------|-------|
| AMQP saturation path | Never polled `consumer.next()` → unacked delivery for 30+ min → channel close | `consumer.next()` arm added; delivery immediately pre-acked; UUID queued for processing |
| Long GitHub ingest jobs | `PRECONDITION_FAILED` channel close after 1800s, partial job failure | Channel stays open; delivery cleared from RabbitMQ's unacked window |
| `doc_concurrency` on 12-CPU host | `12.clamp(2, 16) = 12` concurrent embed workers | `12.clamp(2, 8) = 8` — matches semaphore permit count |
| `embed_pipeline completed with N/xxx doc failures` | Thundering herd → 300s timeout → failures | Reduced competition on 8-permit semaphore |
| `qdrant_upsert` transient errors | Hard failure, job aborted | Retried up to 3× with backoff before failing |
| `collect_and_embed_batched` start | No visible log — appeared hung | `github collect_start files_total=N batch_concurrency=M embed_batch_size=K` logged at INFO |

---

## Verification Evidence

| Command | Expected | Actual | Status |
|---------|----------|--------|--------|
| `cargo check -p axon` | Clean compile | Clean after `Ok(_)` fix | ✅ |
| `cargo test` | 1338 tests, 0 failures | 1338 tests, 0 failures | ✅ |
| `git push` | Pushed to remote | `e2362a68` pushed | ✅ |

---

## Source IDs + Collections Touched

None — no Axon embed/retrieve operations were performed during this session (code-only changes).

---

## Risks and Rollback

| Risk | Severity | Mitigation |
|------|----------|------------|
| Pre-ack pattern: if lane crashes before processing VecDeque, UUID is lost | Low | Re-enqueue block on exit handles this; watchdog reclaims as fallback |
| Re-enqueue on exit: `batch_enqueue_jobs` is best-effort (fire-and-forget) | Low | Watchdog stale sweep catches within 360s anyway |
| `doc_concurrency` clamp change: fewer parallel embeds may slow large batch | Negligible | 8 is the semaphore permit count — more than 8 concurrent was wasted contention |

**Rollback**: `git revert e2362a68` — single commit, clean revert.

---

## Decisions Not Taken

| Alternative | Rejected Because |
|-------------|-----------------|
| Nack + requeue during saturation | Creates tight redelivery loop — job redelivered immediately while lane is still full |
| Increase QoS prefetch (not 1) | Would require coordinated change with broker; side effects unclear |
| Spawn-blocking for tree-sitter CPU work | Larger scope change; the logging fix gives immediate observability without restructuring the embed pipeline |
| Increase qdrant_upsert to 5 retries | 3 retries with backoff (total ~1.5s) is sufficient for transient errors; 5 increases worst-case latency |

---

## Open Questions

- Is the `join_all` reconnect semantics issue (all N lanes must die before reconnect fires) still worth addressing? With the consumer_timeout fix, this is lower priority.
- `cortex` collection named-vector migration (3–6 hours maintenance window) — still deferred.
- Should `doc_concurrency` be configurable via env var (`AXON_EMBED_DOC_CONCURRENCY`) for non-12-CPU hosts?

---

## Next Steps

1. Restart ingest workers to pick up AMQP and `doc_concurrency` fixes
2. Tail logs during next GitHub repo ingest to confirm `collect_start` is visible and consumer_timeout no longer fires:
   ```bash
   tail -f $AXON_DATA_DIR/axon/logs/axon.log | grep -E "github|consumer|PRECONDITION"
   ```
3. Monitor `embed_pipeline completed with N/xxx doc failures` — should drop significantly with `clamp(2, 8)`
