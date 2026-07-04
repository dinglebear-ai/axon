# axon-prune — Agent Guide

`axon-prune` owns **destructive and semi-destructive cleanup execution**: cleanup
debt processing, old-generation pruning, orphan cleanup, dedupe, and dry-run
plans. It answers "what would be deleted, is it safe, and what was actually
removed." It executes against ledger/graph/memory/artifact/vector boundaries via
trait calls — it never owns those stores. Full contract (owns / API / deps /
tests):
[../../../docs/pipeline-unification/crates/axon-prune/README.md](../../../docs/pipeline-unification/crates/axon-prune/README.md)
· behavior spec:
[../../../docs/pipeline-unification/runtime/pruning-contract.md](../../../docs/pipeline-unification/runtime/pruning-contract.md).

## Status — PR0 skeleton
Modules below are **markers only**. Real implementation lands in **Phase 11
(Reset, Prune, And Empty-DB Cutover)**, converging today's scattered stale-cleanup
paths (Qdrant scroll deletes, refresh, dedupe) onto ledger-driven cleanup debt.
Do not add ledger record ownership, source acquisition, embedding, or transport
rendering here.

## Module map
| File | Owns |
|---|---|
| `plan.rs` | `PrunePlanner`, `PrunePlan`, `PruneTarget`, `PruneImpact` — dry-run + impact counts |
| `executor.rs` | `PruneExecutor` — applies a plan against store boundaries |
| `debt.rs` | cleanup-debt execution (recorded by `axon-ledger`, run here) |
| `generation.rs` | old-generation pruning policy |
| `orphan.rs` | vector/artifact orphan cleanup policy |
| `dedupe.rs` | `DedupePlan` — near-duplicate dedupe policy |
| `receipt.rs` | `PruneReceipt` — source ids, generations, counts, skipped reasons |
| `safety.rs` | safety checks + broad-destructive request gating |
| `testing.rs` | `FakePruneExecutor` + debt/generation/orphan/dedupe fixtures |

## Boundary — keep OUT of this crate
- Ledger record ownership — `axon-ledger` records `CleanupDebt`; this crate executes it.
- Vector store implementation detail beyond trait calls.
- Source acquisition, embedding, parsing, transport rendering.
- Legacy-data migration — this is a clean-break, empty-DB target.

## Dependencies
- **Allowed:** `axon-api`, `axon-error`, `axon-core`, `axon-observe`, `axon-ledger`, `axon-graph`, `axon-memory`, `axon-vectors`.
- **Forbidden:** source adapters, embedding providers, LLM providers, transport crates. Enforced by `cargo xtask check-layering`.

## Invariants (review checklist)
- **Dry-run and execute report the same targets** before any mutation.
- **Cleanup debt execution is idempotent** — re-running a cleanup is safe.
- **Deletion receipts include** source ids, generations, counts, and skipped reasons.
- **Broad destructive cleanup requires explicit request flags** at the service boundary.
- All stale cleanup paths converge here; old generations are pruned intentionally through ledger cleanup debt.
- **Empty-DB reset is simple** — no migration/tombstone behavior.

## DTO ownership
Wire DTOs (`PrunePlan`, `PruneTarget`, `PruneImpact`, `PruneReceipt`,
`DedupePlan`) are defined in **`axon-api`**; this crate produces and returns them —
it does not redefine transport-facing shapes.

## Keep in sync when shapes change
`README.md` (crate contract) · `runtime/pruning-contract.md` ·
`runtime/ledger-contract.md` + `schemas/database-schema.md` (cleanup-debt tables) ·
the prune DTO components in `axon-api`.
