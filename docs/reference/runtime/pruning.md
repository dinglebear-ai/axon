# Pruning

Last Modified: 2026-07-19

`axon-prune` owns planned destructive cleanup: cleanup-debt processing,
old-generation pruning, orphan cleanup, duplicate policy, and dry-run plans. It
executes against ledger/graph/memory/artifact/vector boundaries via trait calls
— it **never** owns those stores. Cleanup is debt-driven and idempotent.

> Contract source:
> [`docs/pipeline-unification/runtime/pruning-contract.md`](../../pipeline-unification/runtime/pruning-contract.md).
> Implementation: [`crates/axon-prune/src/`](../../../crates/axon-prune/src/).
> DTOs (`PrunePlan`/`PruneTarget`/`PruneImpact`/`PruneReceipt`/`DedupePlan`)
> live in `axon-api`.

## Layering (enforced)

`axon-ledger` **records** cleanup debt; `axon-prune` **executes** it.
`axon-prune` may depend on `axon-api`/`axon-error`/`axon-core`/`axon-observe`/
`axon-ledger`/`axon-graph`/`axon-memory`/`axon-vectors`; it is forbidden from
depending on source adapters, embedding/LLM providers, or transport crates
(enforced by `cargo xtask check-layering`).

## Plan / exec model

```text
PruneRequest { selector, dry_run, require_confirmation, reason }
  → PrunePlan   { job_id, selector, destructive, requires_admin, estimated, steps, warnings }
  → PruneResult { job_id, status, steps, deleted_counts, cleanup_debt_remaining }
```

**Plan-first:** the default CLI/REST/MCP prune is a dry-run plan unless
explicitly executing. Destructive prune requires `axon:admin` scope. Plans are
reviewable as JSON.

## Single enforcement chokepoint

`PruneExecutor::execute()` (`crates/axon-prune/src/executor.rs`) is the **only**
code path that performs a destructive delete against a store boundary. It takes
an explicit `PruneAuthz` argument and refuses a `requires_admin: true` plan
with `PruneDenied::AdminRequired` unless `authz.is_admin` is set — **before**
any step runs. There is no way to reach a store delete through this crate
without passing that check.

## Cleanup debt kinds (7)

`vector_delete`, `artifact_delete`, `ledger_prune`, `graph_prune`,
`memory_prune`, `job_retention`, `cache_prune`.

## `PruneSelector` variants

`Source{source_id}`, `Generation{source_id, generation}`,
`CleanupDebt{debt_id}`, `Collection{collection}`, `Artifact{artifact_id}`,
`Graph{node_id, edge_id}`, `Memory{memory_id}`,
`JobRetention{older_than_days}`, `Cache{older_than_days}`.

## Debt execution order (6 steps)

1. vector deletes
2. artifact deletes
3. graph prune
4. memory prune
5. ledger prune
6. job/cache retention

Ledger prune runs **last** so join metadata stays available while deleting
vector points/artifacts.

## Idempotent drain (system-trusted exception)

The in-process drain that `axon-services` runs automatically after every
`index_source` (`crates/axon-services/src/source/prune.rs::drain_cleanup_debt`)
is system-trusted, in-process maintenance. It calls `PruneExecutor::execute()`
with explicit `PruneAuthz::admin()` at the call site (mirrors
`AuthSnapshot::trusted_system`). The admin check still runs on every call — it
is pre-authorized for this specific audited system-owned path, and that
authorization is visible at the call site. Cleanup debt retries are idempotent;
partial failure records remaining debt; re-running cleanup is safe.

## Dedupe

A prune operation with a non-source selector. Computes duplicate candidates,
preserves the best point/chunk, creates a dry-run report, deletes only selected
duplicate vector points, and updates document/vector counts. `DedupePlan` is
internal, not a public action. Transports call `axon_services::prune::dedupe`
with caller-derived prune authz.

## Safety rules

- Default dry-run.
- Destructive requires `axon:admin` + `--confirm`.
- Source/generation vector deletes are generation-fenced (cannot delete the
  current generation by accident).
- Artifact deletes are artifact-id-based, never arbitrary-path.
- Prune plans are reviewable as JSON.
- Partial failure records remaining cleanup debt.
- Repeated execution is idempotent.

## CLI

```bash
axon prune plan <target>           # dry-run reviewable plan
axon prune exec <plan_id> --confirm   # destructive execution (admin)
axon reset plan                     # dry-run reset of local stores
axon reset exec --confirm           # destructive reset
```

`reset` is **not** ordinary prune — it is owned by a separate `ResetService`
and does not go through `PruneExecutor::execute()`.

## Module map

`plan.rs` (PrunePlanner/PrunePlan/impact counts), `executor.rs` (PruneExecutor),
`debt.rs` (cleanup-debt execution), `generation.rs` (old-generation policy),
`orphan.rs` (orphan cleanup), `dedupe.rs` (DedupePlan),
`receipt.rs` (PruneReceipt), `safety.rs` (safety checks + broad-destructive
gating), `testing.rs` (FakePruneExecutor + fixtures).

If the prune surface changes, update this file and
[`crates/axon-prune/src/CLAUDE.md`](../../../crates/axon-prune/src/CLAUDE.md)
in the same PR.
