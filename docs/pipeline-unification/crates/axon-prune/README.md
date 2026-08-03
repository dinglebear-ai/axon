# axon-prune Crate Contract
Last Modified: 2026-06-30

## Purpose

`axon-prune` owns destructive and semi-destructive cleanup execution: cleanup
debt processing, old generation pruning, orphan cleanup, dedupe, and dry-run
plans.

## Owns

- `PrunePlanner` and `PruneExecutor`
- cleanup debt execution against ledger, graph, memory, artifacts, and vector
  store boundaries
- dry-run plans, impact counts, safety checks, and deletion receipts
- dedupe and orphan cleanup policies

## Must Not Own

- ledger record ownership
- vector store implementation details beyond trait calls
- source acquisition, embedding, parsing, or transport rendering
- legacy-data migration for this clean-break target

## Public Modules

```text
lib.rs
plan.rs
executor.rs
debt.rs
generation.rs
orphan.rs
dedupe.rs
receipt.rs
safety.rs
testing.rs
```

## Public API

- `PrunePlanner`
- `PruneExecutor`
- `PrunePlan`
- `PruneTarget`
- `PruneImpact`
- `PruneReceipt`
- `DedupePlan`
- `FakePruneExecutor`

## Dependencies Allowed

- `axon-api`, `axon-error`, `axon-core`, `axon-observe`, `axon-ledger`,
  `axon-graph`, `axon-memory`, `axon-vectors`

## Dependencies Forbidden

- source adapters, embedding providers, LLM providers, transport crates

## Generated Artifacts

- prune DTO schemas
- deletion receipt fixtures

## Fixtures And Fakes

- cleanup debt fixture
- old generation prune fixture
- vector orphan fixture
- dedupe dry-run fixture

## Tests

- dry-run and execute plans report the same targets before mutation
- cleanup debt execution is idempotent
- deletion receipts include source ids, generations, counts, and skipped reasons
- broad destructive cleanup requires explicit request flags at service boundary

## Acceptance Criteria

- all stale cleanup paths converge here
- old generations are pruned intentionally through ledger cleanup debt
- empty DB reset is simple and does not require migration/tombstone behavior

See [../README.md](../README.md) and
[../../runtime/pruning-contract.md](../../runtime/pruning-contract.md).
