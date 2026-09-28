# axon-jobs

Own unified SQLite execution state, scheduling, workers, watches, attempts, heartbeats, and reservations.

## Read before changing

[job lifecycle](../../../docs/reference/job-lifecycle.md) · [jobs](../../../docs/reference/runtime/jobs.md) · [events](../../../docs/reference/runtime/events.md) · [database schema](../../../docs/reference/runtime/database-schema.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[store.rs](store.rs) · [unified.rs](unified.rs) · [state_machine.rs](state_machine.rs) · [scheduler.rs](scheduler.rs) · [workers.rs](workers.rs) · [watch_store.rs](watch_store.rs) · [config_snapshot_store.rs](config_snapshot_store.rs) · [embedding_cache_store.rs](embedding_cache_store.rs)

## Change requirements

- Run injected domain work without depending back on axon-services. Keep one durable source job ID and avoid per-source-family stores or child embedding handoffs.

- Preserve attempt/lease fencing, heartbeats, cancellation, provider cooling, and recovery. A restarted worker must not double-publish a generation.

- Watches persist requests/schedules and enqueue ordinary source jobs. Claiming a watch or a job is not proof of completed work; record outcomes and retry eligibility.

- Configuration snapshots and embedding-cache persistence are concrete responsibilities here. Keep cache writes bounded and do not let cache failure corrupt job lifecycle state.

## Verification for code changes

state_machine_tests, provider_cooling_tests, tx_tests, watch scheduler/store sidecars, and source recovery integration tests. Test interrupted attempts and concurrent claims.

Use focused `cargo test -p axon-jobs` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
