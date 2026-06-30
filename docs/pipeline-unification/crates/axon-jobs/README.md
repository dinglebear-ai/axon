# axon-jobs Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-jobs` owns the single durable job runtime for pipeline work, scheduled
watch work, progress, heartbeats, attempts, cancellation, recovery, and worker
coordination.

## Owns

- `JobStore`, `JobRuntime`, and SQLite implementation
- one job model for source, map, extract, ask/research when async, memory,
  prune, and maintenance work
- attempts, reservations, heartbeats, events, leases, cancellation, and recovery
- scheduled watch triggers and run-now execution
- worker lane configuration and backpressure coordination

## Must Not Own

- domain logic for source acquisition, parsing, embedding, vector writes,
  retrieval, LLM synthesis, or pruning
- transport output formatting
- provider implementation internals

## Public Modules

```text
lib.rs
store.rs
sqlite.rs
migration.rs
runtime.rs
job.rs
attempt.rs
event.rs
heartbeat.rs
scheduler.rs
watch.rs
worker.rs
reservation.rs
recovery.rs
testing.rs
```

## Public API

- `JobStore`
- `JobRuntime`
- `SqliteJobStore`
- `JobRecord`
- `JobAttempt`
- `JobEventRecord`
- `JobHeartbeatRecord`
- `JobScheduler`
- `WorkerRuntime`
- `FakeJobRuntime`

## Dependencies Allowed

- `axon-api`, `axon-error`, `axon-core`, `axon-authz`, `axon-observe`
- SQLite/migration crates
- injected worker traits/functions supplied by the composition layer

## Dependencies Forbidden

- `axon-services`
- transport crates
- direct provider clients when a service/provider trait exists
- domain implementation details in scheduler/storage modules

## Generated Artifacts

- job database schema in [../../schemas/database-schema.md](../../schemas/database-schema.md)
- job/progress/event schema fixtures

## Fixtures And Fakes

- fake job runtime
- temp SQLite job store
- stalled heartbeat fixture
- cancellation fixture
- scheduled watch fixture

## Tests

- only one durable job shape exists
- heartbeats, progress, logs, ledger rows, and vector payloads share `job_id`
- stale attempts recover without double-publishing generations
- provider reservations prevent embedding and LLM overload
- cancellation is cooperative and leaves durable failure/degraded state

## Acceptance Criteria

- async and watch work is observable through the same job model
- jobs schedule and run injected workers; jobs do not reimplement services
- worker throughput is tunable without starving source discovery or cleanup

See [../README.md](../README.md) and
[../../runtime/job-contract.md](../../runtime/job-contract.md).
