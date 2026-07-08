# Pruning Contract
Last Modified: 2026-06-30

## Contract

This is the target pruning contract. Current implementation still has direct
purge, dedupe, job cleanup/clear/recover, and family-specific stale cleanup
paths. Those paths must be folded into planned `axon-prune` / cleanup-debt
execution during the clean break.

`axon-prune` owns planned destructive cleanup. Pruning is not ad hoc deletion
from Qdrant, SQLite, or the filesystem. Every destructive operation has a plan,
scope, authorization requirement, dry-run shape, execution result, and audit
event.

## Ownership

| Flow | Owner |
|---|---|
| cleanup debt execution | `axon-prune` |
| user-requested source prune | `axon-prune` + `LedgerStore` |
| vector deletes | `axon-prune` + `VectorStore` |
| artifact deletes | `axon-prune` + `ArtifactStore` |
| graph orphan cleanup | `axon-prune` + `GraphStore` |
| memory forgetting cleanup | `axon-prune` + `MemoryStore` + `VectorStore` |
| dedupe | `axon-prune` + `VectorStore` |
| reset | `ResetService`, not ordinary prune |

## Public Types

```rust
pub struct PruneRequest {
    pub selector: PruneSelector,
    pub dry_run: bool,
    pub require_confirmation: bool,
    pub reason: String,
}

pub enum PruneSelector {
    Source { source_id: SourceId },
    Generation { source_id: SourceId, generation: SourceGenerationId },
    CleanupDebt { debt_id: CleanupDebtId },
    Collection { collection: String },
    Artifact { artifact_id: ArtifactId },
    Graph { node_id: Option<GraphNodeId>, edge_id: Option<GraphEdgeId> },
    Memory { memory_id: Option<MemoryId> },
    JobRetention { older_than_days: u32 },
    Cache { older_than_days: u32 },
}

pub struct PrunePlan {
    pub job_id: JobId,
    pub selector: PruneSelector,
    pub destructive: bool,
    pub requires_admin: bool,
    pub estimated: PruneEstimate,
    pub steps: Vec<PruneStep>,
    pub warnings: Vec<SourceWarning>,
}

pub struct PruneResult {
    pub job_id: JobId,
    pub status: LifecycleStatus,
    pub steps: Vec<PruneStepResult>,
    pub deleted_counts: PruneCounts,
    pub cleanup_debt_remaining: u64,
}
```

## Safety Rules

- default CLI/REST/MCP prune is dry-run unless explicitly executing
- destructive prune requires `axon:admin`
- source/generation vector deletes are generation-fenced
- artifact deletes are artifact-id based, never arbitrary path based
- prune plans must be reviewable as JSON
- partial failure records remaining cleanup debt
- repeated prune execution is idempotent

`PruneExecutor::execute()` is the single code path that performs any
destructive delete against a store boundary, so it is where the `axon:admin`
gate is actually enforced: it takes an explicit `PruneAuthz` argument and
refuses a `requires_admin: true` plan with `PruneDenied::AdminRequired` unless
`authz.is_admin` is set, before any step runs. There is no way to reach a
store delete through this crate without passing through that check.

**Automatic cleanup-debt drain is a documented exception, not a bypass.** The
in-process cleanup-debt drain that `axon-services` runs automatically after
every `index_source` call (`crates/axon-services/src/source/prune.rs::drain_cleanup_debt`)
is system-trusted, in-process maintenance — not a user-invoked "delete my
data" request — so it calls `PruneExecutor::execute()` with an explicit
`PruneAuthz::admin()` passed at the call site (mirroring how
`AuthSnapshot::trusted_system` is used elsewhere for system-triggered work).
The admin check still executes on every call; it is simply pre-authorized for
this specific, audited, system-owned path, and that authorization is visible
in the call site rather than defaulted or silently skipped.

## Cleanup Debt Execution

Cleanup debt execution is maintenance prune. It may run in the background, but
it still emits job events and audit events.

Debt execution order:

1. vector deletes
2. artifact deletes
3. graph prune
4. memory prune
5. ledger prune
6. job/cache retention

Ledger prune runs last so join metadata remains available while deleting vector
points/artifacts.

## Dedupe

Dedupe is a prune operation with a non-source selector. It must:

- compute duplicate candidates
- preserve the best point/chunk
- create a dry-run report
- delete only selected duplicate vector points
- update document/vector counts when applicable

## Testing Requirements

- dry-run does not delete
- admin required for destructive execution
- generation-fenced deletes cannot delete current generation by accident
- cleanup debt retries are idempotent
- partial failure records remaining debt
- artifact traversal is impossible
