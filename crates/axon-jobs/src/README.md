# src/jobs
Last Modified: 2026-05-06

Async job runtime and lifecycle management for axon's SQLite backend.

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
- `runtime.rs`: `SqliteJobBackend` — SQLite pool + in-process worker spawning (with `new()` enqueue-only and `new_with_workers()` constructors).
- `workers.rs` + `workers/`: per-kind in-process workers.
- `store.rs`, `query.rs`, `cancel.rs`, `ops.rs` (+ `ops/`): SQL helpers.
- `config_snapshot.rs`: per-job config snapshotting.
- `migrations/`: SQLite migration files.
- `crawl.rs`, `embed.rs`, `extract.rs`, `ingest.rs`: per-kind schema + payload helpers.
- `watch.rs`: SQLite-backed watch task scheduler.
- `status.rs`: shared `JobStatus` enum.
- `error.rs`: job error types.

## Integration Points
- Enqueue is initiated from `src/services/<kind>::*_start` via `ServiceContext.jobs`.
- Crawl execution delegates into `src/crawl`.
- Embed/query workflows interact with `src/vector/ops`.
- Service callers go through `ServiceJobRuntime` (`src/services/runtime.rs`),
  not `JobBackend` directly.

## Notes
- SQLite is the source of truth for job state; there is no alternate Postgres/Redis/AMQP runtime.
- `SqliteJobBackend::new(cfg)` is enqueue-only — safe for fire-and-forget CLI commands.
  Use `SqliteJobBackend::new_with_workers(cfg)` for serve / mcp / web (anywhere the process
  must drain the queue itself).
- Recovery behavior is driven by `AXON_JOB_STALE_TIMEOUT_SECS` and `AXON_JOB_STALE_CONFIRM_SECS`.

## Related Docs
- [Repository README](../../README.md)
- [Architecture](../../docs/architecture/overview.md)
- [Docs Index](../../docs/README.md)
