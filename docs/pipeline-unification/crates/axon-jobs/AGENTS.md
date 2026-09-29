# axon-jobs design-contract maintenance

This directory documents the `axon-jobs` boundary: Own unified SQLite execution state, scheduling, workers, watches, attempts, heartbeats, and reservations.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-jobs/src/AGENTS.md) · [Crate exports](../../../../crates/axon-jobs/src/lib.rs)

[job lifecycle](../../../../docs/reference/job-lifecycle.md) · [jobs](../../../../docs/reference/runtime/jobs.md) · [events](../../../../docs/reference/runtime/events.md) · [database schema](../../../../docs/reference/runtime/database-schema.md)

## Review the actual boundary

- Run injected domain work without depending back on axon-services. Keep one durable source job ID and avoid per-source-family stores or child embedding handoffs.

- Configuration snapshots and embedding-cache persistence are concrete responsibilities here. Keep cache writes bounded and do not let cache failure corrupt job lifecycle state.

For implementation evidence, inspect [store.rs](../../../../crates/axon-jobs/src/store.rs), [unified.rs](../../../../crates/axon-jobs/src/unified.rs), [state_machine.rs](../../../../crates/axon-jobs/src/state_machine.rs).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

state_machine_tests, provider_cooling_tests, tx_tests, watch scheduler/store sidecars, and source recovery integration tests. Test interrupted attempts and concurrent claims.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
