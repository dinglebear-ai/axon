# Full-Pipeline Source Progress Contract

## Contract role

This contract defines the code-level boundaries and invariants required to implement `runtime/source-progress-spec.md`. It does not replace `StageCounts`, `SourceProgressEvent`, `JobHeartbeat`, or `ServiceJob`; it constrains how existing canonical DTOs are produced and consumed.

## Ownership

| Concern | Owner |
|---|---|
| adapter-local acquisition observations | `axon-adapters` |
| cumulative pipeline progress and throttling | `axon-services` |
| authoritative durable job snapshot | `axon-jobs` |
| canonical progress DTOs | `axon-api` |
| human labels and formatting | `axon-cli` |
| MCP/REST/JSON serialization | existing shared service/API projections |

No adapter may depend on `axon-jobs` or `axon-services`. No CLI code may infer pipeline state by querying providers or stores directly.

## Adapter boundary

The existing adapter boundary is retained:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcquisitionProgress {
    pub items_total: u64,
    pub items_done: u64,
    pub documents_done: u64,
}

#[async_trait]
pub trait AcquisitionProgressSink: Send + Sync {
    async fn report(&self, progress: AcquisitionProgress);
}

#[async_trait]
pub trait SourceAdapter: Send + Sync {
    async fn acquire_with_progress(
        &self,
        plan: &SourcePlan,
        diff: &SourceManifestDiff,
        progress: Option<&dyn AcquisitionProgressSink>,
    ) -> Result<SourceAcquisition> {
        self.acquire(plan, diff).await
    }
}
```

Required semantics:

- snapshots are batch-local at the adapter boundary;
- `items_total` equals added plus modified items in the supplied diff batch;
- `items_done` counts completed attempts, including non-fatal skips/failures;
- `documents_done` counts successfully acquired outputs;
- snapshots are monotonic;
- the sink has no error return by design, so observation cannot fail acquisition.

## Service progress coordinator

The current `JobAcquisitionProgress` embedded in `created_generation.rs` MUST be replaced by a focused module under `source/non_web/`, following the repository file-per-module convention.

The coordinator MUST own:

- job/source/adapter/source-kind identity;
- active phase;
- generation-global totals;
- batch offsets;
- latest accepted snapshot;
- write throttling;
- final-flush behavior;
- count validation/clamping;
- structured warning logging on persistence failure.

The coordinator SHOULD expose focused methods rather than one large argument-heavy function, for example:

```rust
ProgressCoordinator::new(context)
    .begin_phase(phase, totals, message)
    .await;

let batch = coordinator.acquisition_batch(batch_items, offsets);
adapter.acquire_with_progress(plan, diff, Some(&batch)).await?;
batch.complete_from(&acquisition).await;

coordinator.complete_embedding(batch_chunk_count).await;
coordinator.complete_upsert(write.points_written).await;
```

Exact names may differ, but ownership and behavior may not.

## Count validation

Before writing `StageCounts`, the coordinator MUST:

- use saturating arithmetic for offsets;
- clamp completed counts to known totals;
- reject regression by retaining the last accepted value;
- treat an adapter-provided total different from the batch total as malformed input;
- log malformed snapshots at warning level without failing the pipeline;
- force a final correct batch snapshot derived from returned `SourceAcquisition`.

## Phase transition contract

A phase transition MUST write a new `JobStatusUpdate` containing:

- `LifecycleStatus::Running`;
- the exact active `PipelinePhase`;
- phase-appropriate `StageCounts` when totals are known;
- `ProgressCurrent.adapter` from the routed adapter;
- a redacted human message;
- no stale counts from the prior phase.

`record_running_phase` MUST either accept counts or be replaced by coordinator methods. Calling it with `counts: None` for a measurable stage is a contract violation.

## Downstream batch contract

`source/non_web/vectorize.rs` currently owns document preparation, embedding batches, point construction, and upserts. It MUST report cumulative checkpoints at these boundaries:

1. after `prepare_documents` returns for a source-document batch;
2. after `embedding_batch` is built;
3. after `EmbeddingProvider::embed` returns;
4. after `point_batch` returns;
5. after `VectorStore::upsert` returns;
6. after bounded document-status persistence completes.

The orchestration API MUST pass a progress coordinator or narrow reporter into `prepare_embed_publish` and `vectorize_batch`; it MUST NOT make embedding/vector crates depend on the job store.

## Durable job-store contract

`JobStore::update_status` remains authoritative. The implementation MUST account for current replacement semantics in `SqliteUnifiedJobStore::update_job_status`:

- `counts_json` and `current_json` are replaced on each update;
- every measurable transition therefore supplies complete current-phase counts;
- terminal status writes replace live counts with terminal result counts;
- progress write retries remain owned by `axon-jobs`.

The fake job store MUST preserve equivalent observable semantics for tests.

## Event and heartbeat contract

Every persisted progress update already produces an observability event and heartbeat through `SqliteUnifiedJobStore::observe_status`. Additional source events SHOULD be emitted at meaningful batch completion boundaries, but duplicate high-frequency event storms MUST be avoided.

Provider reservation heartbeats MUST include the latest phase counts. A heartbeat with `counts: None` during known embedding/upsert work violates this contract.

## Service projection contract

`ServiceJob` requires canonical source identity sufficient for presentation. Raw adapter name is not a canonical source family because route aliases include `github`, `gitlab`, `gitea`, `crates`, `npm`, and `pypi`.

The projection MUST expose or derive `SourceKind` from durable job request/route metadata. Presentation MUST NOT classify progress solely from `ProgressCurrent.adapter`.

A preferred clean-break change is to add an optional canonical `source_kind` field to `ServiceJob` and populate it from persisted request metadata, with adapter fallback only for old local rows. Any DTO change must update all constructors, serialization tests, MCP/REST projections, and relevant docs/schemas.

## CLI rendering contract

`axon-cli/src/commands/job_progress.rs` MUST render by phase:

- `Fetching`: source-kind unit plus percentage.
- `Enriching`: items enriched.
- `Normalizing`: documents normalized.
- `Preparing`: documents prepared and chunks discovered.
- `Batching`: chunks batched.
- `Embedding`: chunks embedded and percentage.
- `Vectorizing`: vectors built and percentage.
- `Upserting`: vectors written and percentage.
- `Publishing`: generation commit state.

Elapsed time remains appended by `status.rs` and MUST not be duplicated in progress payload rendering.

Unknown totals render a completed count without a percentage. Empty or absent measurable counts fall back to the phase label, not a fabricated zero-total percentage.

## Compatibility and graceful degradation

- The default adapter method provides batch-level progress through runner finalization.
- Missing source kind falls back to generic `items`, never a guessed family noun.
- Old local rows containing legacy `pages_crawled`, `md_created`, `videos_done`, or `files_done` remain readable until their compatibility path is intentionally removed.
- Progress-store or observability failures are warnings and do not alter successful domain results.
- Authoritative terminal job write failures remain errors because callers otherwise cannot trust completion.

## Required tests

### axon-adapters

- default `acquire_with_progress` delegates without requiring an override;
- web concurrent and sequential paths report monotonic per-item snapshots;
- web failures and 304 skips advance attempts without inflating documents.

### axon-services

- runner fallback publishes zero/start and final batch snapshots for `FakeSourceAdapter`;
- multi-batch acquisition accumulates global offsets;
- malformed/regressing adapter snapshots are clamped and logged;
- progress persistence failure does not fail the source operation;
- preparing/embedding/vectorizing/upserting advance cumulative counts over multiple batches;
- `embed=false` leaves no stale embedding/vector counts;
- reservation heartbeat carries latest counts.

### axon-jobs

- SQLite and fake stores expose equivalent replacement semantics;
- progress updates produce observe events and heartbeats with matching counts.

### service projection

- canonical source kind is preserved for every `SourceKind`;
- adapter aliases map to their canonical family;
- old rows degrade to generic or adapter-derived behavior safely.

### axon-cli

- table-driven phase rendering for every measurable phase;
- source-kind singular/plural labels;
- unknown totals and percentage clamping;
- legacy payload compatibility;
- elapsed duration appears exactly once.

### end to end

A deterministic integration fixture MUST run a source with enough documents/chunks to create multiple acquisition and embedding batches, capture status snapshots while blocked at controlled provider gates, and prove ordered transitions and increasing counts through fetching, preparing, embedding, vectorizing, upserting, publishing, and complete.

## Quality gates

Completion requires all applicable repository gates, at minimum:

- `cargo fmt --all -- --check`
- targeted crate tests for adapters, services, jobs, API, CLI, MCP, and web projections touched
- `cargo test --no-run --workspace --lib --locked`
- `cargo clippy --all-targets --locked -- -D warnings`
- `cargo xtask check-layering`
- `cargo xtask check-no-mod-rs`
- `cargo xtask check-fetch-divergence`
- monolith check with no new allowlist entries
- `git diff --check`
- repository pre-commit and pre-push hooks

The final proof record MUST include exact test names, command outcomes, a clean worktree, matching local/remote commit hashes, and captured human plus JSON status output from the deterministic integration fixture.
