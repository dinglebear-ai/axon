# axon-ledger design-contract maintenance

This directory documents the `axon-ledger` boundary: Own durable source records, manifest diffs, generations, document status, leases, retention, and cleanup debt.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-ledger/src/AGENTS.md) · [Crate exports](../../../../crates/axon-ledger/src/lib.rs)

[ledger](../../../../docs/reference/runtime/ledger.md) · [pruning](../../../../docs/reference/runtime/pruning.md) · [database schema](../../../../docs/reference/runtime/database-schema.md) · [vector payload](../../../../docs/reference/sources/vector-payload.md)

## Review the actual boundary

- Production databases already exist. Remove fresh-database assumptions: append migrations and test upgrades, restart/recovery, and existing generations.

- Source accounting does not acquire, embed, write Qdrant points, or execute cleanup itself. Report source/generation and conflict context on rejected transitions.

For implementation evidence, inspect [store.rs](../../../../crates/axon-ledger/src/store.rs), [sqlite.rs](../../../../crates/axon-ledger/src/sqlite.rs), [sqlite/](../../../../crates/axon-ledger/src/sqlite/).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

store_tests, sqlite_tests and generation/lease/debt sidecars, including stale leases, replay, failed publication, partial snapshots, and existing-schema upgrades.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
