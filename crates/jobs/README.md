# crates/jobs
Last Modified: 2026-05-06

Async job runtime and lifecycle management for axon's lite-mode backend.

## Purpose
- Persist crawl/extract/embed/ingest jobs in SQLite.
- Run in-process tokio workers that drain the queues without external brokers.
- Expose status/cancel/list/cleanup/recover/worker controls via the `JobBackend` trait
  and the richer `ServiceJobRuntime` consumed by the services layer.

## Responsibilities
- SQLite-backed job persistence.
- Atomic claim/run/complete/fail/cancel state transitions.
- In-process worker dispatch per `JobKind`.
- Per-job heartbeat (`updated_at` touch every 30s) plus periodic + startup watchdog (`reclaim_stale_running_jobs`).
- In-process cancellation via `CancelStore` + `CancellationToken` for crawl/embed/extract/ingest runners.
- Per-domain job family schemas (crawl/extract/embed/ingest) and the watch scheduler.

## Key Files
- `backend.rs`: `JobBackend` trait + `JobPayload` + `JobKind` + `JobStatusRow` + `JobSummary`.
- `lite.rs`: `LiteBackend` — SQLite pool + in-process worker spawning (with `new()` enqueue-only and `new_with_workers()` constructors).
- `lite/workers.rs` + `lite/workers/`: per-kind in-process workers.
- `lite/store.rs`, `lite/query.rs`, `lite/cancel.rs`, `lite/ops.rs` (+ `lite/ops/`): SQL helpers.
- `lite/config_snapshot.rs`: per-job config snapshotting.
- `lite/migrations/`: SQLite migration files.
- `crawl.rs`, `embed.rs`, `extract.rs`, `ingest.rs`: per-kind schema + payload helpers.
- `watch_lite.rs`: SQLite-backed watch task scheduler.
- `status.rs`: shared `JobStatus` enum.
- `error.rs`: job error types.

## Integration Points
- Enqueue is initiated from `crates/services/<kind>::*_start` via `ServiceContext.jobs`.
- Crawl execution delegates into `crates/crawl`.
- Embed/query workflows interact with `crates/vector/ops`.
- Service callers go through `ServiceJobRuntime` (`crates/services/runtime.rs`),
  not `JobBackend` directly.

## Notes
- SQLite is the source of truth for job state; lite mode is the only supported runtime.
- `LiteBackend::new(cfg)` is enqueue-only — safe for fire-and-forget CLI commands.
  Use `LiteBackend::new_with_workers(cfg)` for serve / mcp / web (anywhere the process
  must drain the queue itself).
- Recovery behavior is driven by `AXON_JOB_STALE_TIMEOUT_SECS` and `AXON_JOB_STALE_CONFIRM_SECS`.

## Related Docs
- [Repository README](../../README.md)
- [Architecture](../../docs/ARCHITECTURE.md)
- [Docs Index](../../docs/README.md)
