# axon-prune

Produce reviewed cleanup plans, generation-fenced execution, authorization decisions, and deletion receipts.

## Read before changing

[pruning](../../../docs/reference/runtime/pruning.md) · [ledger](../../../docs/reference/runtime/ledger.md) · [operations](../../../docs/operations/operations.md)

## Implementation map

[Crate exports](lib.rs) and [manifest](../Cargo.toml); focused entry points:
[plan.rs](plan.rs) · [executor.rs](executor.rs) · [safety.rs](safety.rs) · [receipt.rs](receipt.rs) · [debt.rs](debt.rs) · [generation.rs](generation.rs) · [orphan.rs](orphan.rs) · [dedupe.rs](dedupe.rs)

## Change requirements

- Operate against existing state; the historical empty-database cutover is not a runtime assumption. Do not discard migration, recovery, or tombstone requirements.

- Plans and execution must resolve the same targets. Revalidate generation/safety before applying each step through PruneTarget, and preserve cleanup-debt order.

- Receipts identify actual deletions, skipped reasons, partial progress, and source/generation scope. A failed step is not an empty successful cleanup, and retry guidance must account for already-applied effects.

- Keep stores injected and ownership in ledger/graph/memory/vector/artifact domains. Broad or destructive operations require the service’s explicit confirmation and authorization boundary.

## Verification for code changes

Plan/executor/safety/receipt sidecars and service prune/reset integrations; test stale plans, denied execution, partial failures, replay, and current-generation protection.

Use focused `cargo test -p axon-prune` targets. For contract changes, follow
[generated-contract validation](../../../docs/development/documentation.md);
update the linked references and affected transport consumers together.
