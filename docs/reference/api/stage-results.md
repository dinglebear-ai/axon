# Stage Results

Last Modified: 2026-07-19

Stage results describe pipeline progress and outcomes across every stage —
acquisition, preparation, graphing, embedding, publishing, and cleanup. They
are transport-neutral: CLI, MCP, REST, web, and mobile clients consume the
same DTO shapes.

> Contract source:
> [`docs/pipeline-unification/foundation/types/stage-result-contract.md`](../../pipeline-unification/foundation/types/stage-result-contract.md).
> Live DTO source: [`crates/axon-api/src/source/stage.rs`](../../../crates/axon-api/src/source/stage.rs),
> `lifecycle.rs`, `vector.rs`, `common.rs`. Machine shape:
> [`schemas.json`](schemas.json).

## Shape rule

Every stage returns either `StageExecutionResult<T>` (for a simple payload `T`)
or a concrete result DTO that embeds a `StageResultHeader` — **except** two
header-less exceptions:

- `EmbeddingResult` (`embedding` stage) — carries its own `batch_id`/`job_id`.
- `SourceResult` (`complete` stage) — the terminal job-summary DTO; already
  carries `job_id`/`status`/`warnings` directly and is never wrapped a second
  time.

## `StageResultHeader`

```rust
struct StageResultHeader {
    job_id: JobId,
    stage_id: StageId,
    phase: PipelinePhase,
    status: LifecycleStatus,
    started_at: Timestamp,
    completed_at: Option<Timestamp>,
    counts: StageCounts,
    warnings: Vec<SourceWarning>,
    error: Option<SourceError>,
}
```

`StageCounts` carries `items_total`/`items_done`, `documents_total`/`done`,
`chunks_total`/`done`, `bytes_total`/`done` (totals are `Option<u64>`; done
counters are `u64`).

## Stage → result type

| Stage | Result type |
|---|---|
| `requested` | `StageExecutionResult<SourceRequest>` |
| `resolving` | `StageExecutionResult<ResolvedSource>` |
| `routing` | `StageExecutionResult<RoutePlan>` |
| `authorizing` | `AuthorizationResult` (`decision`, `caller`) |
| `planning` | `StageExecutionResult<SourcePlan>` |
| `leasing` | `LeaseResult` (`lease_key`, `acquired`, `owner`, `expires_at`) |
| `discovering` | `StageExecutionResult<SourceManifest>` |
| `diffing` | `SourceManifestDiff` (`added`/`modified`/`removed`/`unchanged`/`skipped`/`failed`, `counts`) |
| `fetching` | `SourceAcquisition` (`fetched_items`, `artifacts`) |
| `enriching` | `SourceEnrichment` (`enrichment_kind`, `parse_hints`, `chunk_hints`, `graph_candidates`) |
| `normalizing` | `StageExecutionResult<SourceDocument>` |
| `parsing` | `ParseResult` (`facts`, `graph_candidates`, `parser_id`, `parser_version`, `warnings`, `errors`) |
| `graphing` | `GraphWriteResult` (`candidates_seen`, `nodes_upserted`, `edges_upserted`, `evidence_records`) |
| `preparing` | `StageExecutionResult<PreparedDocument>` |
| `batching` | `StageExecutionResult<EmbeddingBatch>` |
| `embedding` | `EmbeddingResult` (header-less: `batch_id`, `job_id`, `provider_id`, `model`, `dimensions`, `vectors`, `usage`, `warnings`) |
| `vectorizing` | `StageExecutionResult<VectorPointBatch>` |
| `upserting` | `VectorStoreWriteResult` (`collection`, `points_attempted`, `points_written`, `payload_indexes_created`, `usage`) |
| `publishing` | `PublishGenerationResult` (`generation`, `published_at`, `document_count`, `chunk_count`, `vector_point_count`, `cleanup_debt`) |
| `cleaning` | `CleanupDebtResult` (`debt_id`, `kind`, `status`, `items_attempted`, `items_cleaned`, `next_retry_at`) |
| `complete` | `SourceResult` (header-less terminal DTO) |

## `SourceResult` (terminal aggregation)

```rust
struct SourceResult {
    job_id, source_id, canonical_uri, source_kind, adapter, scope,
    status: LifecycleStatus,
    ledger: LedgerSummary,        // source_id, generation, committed_generation, status, counts
    graph: GraphWriteSummary,     // nodes_upserted, edges_upserted, evidence_records, degraded
    counts: SourceCounts,         // items_total, items_changed, documents_total, chunks_total,
                                  // vector_points_total, bytes_total
    warnings: Vec<SourceWarning>,
    inline: Option<InlineSourceResult>,
    job: Option<JobDescriptor>,   // kind, id, status_url, events_url, stream_url, poll_after_ms
    watch: Option<WatchResult>,
    artifacts: Vec<ArtifactRef>,
    errors: Vec<SourceError>,
}
```

## Degradation rules

- **Optional enrichment** can degrade without failing the generation.
- **Required** fetch/prepare/embed/vector-write/publish failures keep the
  generation uncommitted unless source policy explicitly allows partial
  degraded publish.
- **Item-level** failures are stored in the stage result **and** ledger item
  status.
- **Cleanup** failures produce `CleanupDebt` and do **not** unpublish a
  completed generation.

## Event emission

Every stage emits `stage_started`, progress events (per batch/item), and a
terminal `stage_completed`/`stage_degraded`/`stage_failed`. Events use
`SourceProgressEvent` and reference the stage result id or stage id.

## Ownership

DTO ownership lives in `axon-api`. Emission lives in the service/domain stage
that performs the work. Transports render these DTOs — they do not invent
surface-specific progress models.

If a stage result shape changes, update this file, `crates/axon-api/src/source/stage.rs`,
and regenerate via `cargo xtask schemas api` in the same PR.
