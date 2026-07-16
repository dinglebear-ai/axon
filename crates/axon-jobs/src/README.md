# src/jobs
Last Modified: 2026-05-06

Async source-oriented job runtime and lifecycle management for axon's SQLite backend.

## Purpose
- Persist source, extract, watch, memory, graph, prune, reset, and other unified
  jobs in SQLite.
- Run in-process tokio workers that drain the queues without external brokers.
- Expose enqueue/list/get/events/cancel/retry/recover/cleanup/clear controls via
  the `JobStore` boundary consumed by the services layer.

## Responsibilities
- SQLite-backed job persistence.
- Atomic claim/run/complete/fail/cancel state transitions.
- In-process worker dispatch over canonical `JobKind` values.
- Per-job heartbeat (`updated_at` touch every 30s) plus periodic + startup watchdog (`reclaim_stale_running_jobs`).
- In-process cancellation via `CancelStore` + `CancellationToken` for unified job runners.
- Source-oriented job payloads, shared lifecycle schemas, and the watch scheduler.

## Key Files
- `boundary.rs`: transport-neutral `JobStore` boundary.
- `unified.rs` + `unified/`: unified enqueue, lifecycle, event, and pagination operations.
- `runtime.rs`: runtime composition and worker notification.
- `workers.rs` + `workers/`: canonical in-process worker lanes.
- `store.rs`: hardened SQLite pool and migration composition.
- `config_snapshot.rs` + `config_snapshot_store.rs`: per-job config snapshots.
- `migrations/`: SQLite migration files.
- `watch_schedule.rs` and `watch_store*.rs`: SQLite-backed source-watch scheduler/store.
- `state_machine.rs`, `status.rs`, and `error.rs`: lifecycle and error contracts.

## Integration Points
- Source and operation services submit canonical job requests through
  `ServiceContext` and the `JobStore` boundary.
- Injected workers call the owning domain/service boundaries for acquisition,
  preparation, embedding, publication, extraction, memory, pruning, and reset.
- CLI, MCP, and REST callers use `axon-services`/`axon-api`; they do not import
  this crate directly.

## Notes
- SQLite is the source of truth for job state; there is no alternate Postgres/Redis/AMQP runtime.
- Detached jobs advance only while an in-process worker runtime is active.
- Recovery behavior is driven by `AXON_JOB_STALE_TIMEOUT_SECS` and `AXON_JOB_STALE_CONFIRM_SECS`.

## Related Docs
- [Repository README](../../../README.md)
- [Architecture](../../../docs/architecture/overview.md)
- [Docs Index](../../../docs/README.md)
