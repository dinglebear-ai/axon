# axon-prune design-contract maintenance

This directory documents the `axon-prune` boundary: Produce reviewed cleanup plans, generation-fenced execution, authorization decisions, and deletion receipts.
Rust implementations belong in the crate, not in this documentation directory.

## Read together

[Design contract](README.md) · [Current implementation guide](../../../../crates/axon-prune/src/AGENTS.md) · [Crate exports](../../../../crates/axon-prune/src/lib.rs)

[pruning](../../../../docs/reference/runtime/pruning.md) · [ledger](../../../../docs/reference/runtime/ledger.md) · [operations](../../../../docs/operations/operations.md)

## Review the actual boundary

- Operate against existing state; the historical empty-database cutover is not a runtime assumption. Do not discard migration, recovery, or tombstone requirements.

- Keep stores injected and ownership in ledger/graph/memory/vector/artifact domains. Broad or destructive operations require the service’s explicit confirmation and authorization boundary.

For implementation evidence, inspect [plan.rs](../../../../crates/axon-prune/src/plan.rs), [executor.rs](../../../../crates/axon-prune/src/executor.rs), [safety.rs](../../../../crates/axon-prune/src/safety.rs).
Check the manifest and actual callers before describing a dependency or API as
shipped. Distinguish current behavior, intended constraints, and remaining work;
historical phase/cutover prose is not authority to restore removed runtime paths
or to assume that existing databases are empty. Preserve dated outcomes.

## Verification and paired edits

Plan/executor/safety/receipt sidecars and service prune/reset integrations; test stale plans, denied execution, partial failures, replay, and current-generation protection.

When shapes or behavior change, update this contract and its linked live guide.
Regenerate schema projections from owning inputs rather than hand-editing them;
see [documentation validation](../../../development/documentation.md).
Documentation-only edits need link/structural checks, not provider deployment.
