# Ledger Contract
Last Modified: 2026-06-30

## Contract

`axon-ledger` owns source lifecycle state for every mutable, refreshable, or
accountable source. The ledger is the system of record for what Axon believes it
has discovered, fetched, normalized, prepared, embedded, published, and cleaned.

Qdrant is not the ledger. Jobs are not the ledger. Artifacts are not the ledger.

## Ownership

| Area | Ledger Owns |
|---|---|
| Sources | stable source ids, canonical URI, adapter/scope, authority, status |
| Items | per-source item keys, hashes, versions, mtimes, parentage |
| Manifests | complete observed item sets per generation |
| Diffs | added/modified/removed/unchanged/skipped/failed decisions |
| Generations | active, committed, failed, abandoned, cleanup-pending snapshots |
| Documents | document status, chunk counts, vector point counts |
| Leases | refresh/watch/publish cleanup coordination |
| Cleanup Debt | durable destructive work to perform after publish |

## Public Boundary

```rust
#[async_trait]
pub trait LedgerStore: Send + Sync {
    async fn upsert_source(&self, source: SourceSummary) -> Result<()>;
    async fn get_source(&self, source_id: SourceId) -> Result<Option<SourceSummary>>;
    async fn list_sources(&self, request: SourceListRequest) -> Result<Page<SourceSummary>>;
    async fn create_generation(&self, request: CreateGenerationRequest) -> Result<SourceGeneration>;
    async fn put_manifest(&self, manifest: SourceManifest) -> Result<()>;
    async fn diff_manifest(&self, request: DiffManifestRequest) -> Result<SourceManifestDiff>;
    async fn record_item_status(&self, status: SourceItemStatus) -> Result<()>;
    async fn update_document_status(&self, status: DocumentStatus) -> Result<()>;
    async fn publish_generation(&self, request: PublishGenerationRequest) -> Result<PublishGenerationResult>;
    async fn record_cleanup_debt(&self, debt: CleanupDebt) -> Result<()>;
    async fn acquire_lease(&self, request: LeaseRequest) -> Result<Option<LeaseGuard>>;
    async fn release_lease(&self, lease_id: LeaseId) -> Result<()>;
}
```

## Generation Model

A generation is a publishable source snapshot.

Rules:

- every mutable source job creates a generation before fetch/prepare
- search/retrieve only use committed generations unless explicitly debugging
- generation publish is atomic from the user's perspective
- failed generations remain inspectable until job retention removes them
- old committed generations are removed through cleanup debt
- cleanup failure does not unpublish the new generation

Generation lifecycle uses the shared `LifecycleStatus` enum from
`foundation/types/enum-contract.md`. Publish semantics are represented by the
separate `publish_state` field, not by inventing a second status enum.

Generation lifecycle statuses:

| State | Meaning |
|---|---|
| `pending` | created but not yet writing item/document state |
| `running` | acquisition/preparation/vector writes in progress |
| `completed_degraded` | published with item-level degradation |
| `completed` | published cleanly |
| `failed` | not published |
| `canceled` | intentionally stopped before publish |

Generation publish states:

| State | Meaning |
|---|---|
| `planning` | generation plan is being built |
| `writing` | source items/documents/vectors are being written |
| `publishing` | atomic publish is in progress |
| `committed` | safe for search/retrieve |
| `cleanup_pending` | old generations have cleanup debt |
| `cleaning` | cleanup debt is executing |
| `cleaned` | cleanup debt completed |

## Manifest Diff Rules

Diffing compares the new manifest against the committed generation:

| Diff | Rule |
|---|---|
| added | item key absent in committed generation |
| modified | item key exists and hash/version/mtime changed |
| removed | committed item key absent from new manifest |
| unchanged | item key and freshness fields unchanged |
| skipped | policy skipped item before fetch |
| failed | item could not be classified safely |

Unchanged items reuse previous document/vector state by generation reference.
Removed items create cleanup debt.

## Leases

Leases prevent duplicate mutable-source refreshes.

Lease keys:

- `source:{source_id}:refresh`
- `source:{source_id}:publish`
- `source:{source_id}:cleanup`
- `watch:{watch_id}:run`

Lease records include:

- lease id
- owner id
- owner process metadata
- acquired at
- expires at
- heartbeat at
- job id

Stale leases are recoverable by `JobService::recover` and watch scheduler
recovery. Recovery must emit audit/progress events.

## Cleanup Debt

Ledger creates cleanup debt for:

- old generation vector deletes
- removed item vector deletes
- artifact retention deletes
- orphan graph evidence
- memory/vector deletion after forget
- failed publish leftovers

Cleanup debt is idempotent and retryable. The ledger owns debt state; `axon-prune`
executes it.

## Empty Store Assumption

This refactor assumes empty stores. No old ledger schema migration, backfill, or
compatibility alias is required. Schema migrations exist for forward development
after the new schema lands.

## Testing Requirements

- generation publish atomicity
- manifest diff correctness
- lease acquisition/expiry/recovery
- cleanup debt idempotency
- unchanged item reuse
- removed item cleanup creation
- fake ledger parity with SQLite ledger
