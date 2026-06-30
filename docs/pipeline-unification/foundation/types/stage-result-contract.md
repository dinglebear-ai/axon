# Stage Result Contract
Last Modified: 2026-06-30

## Contract

Every pipeline stage has a concrete input, output, status, degradation policy,
and event emission rule. Stage results are serializable DTOs owned by
`axon-api`.

Payload DTOs are not stage results by themselves. Every stage returns either
`StageExecutionResult<T>` for a simple payload or a concrete result DTO that
includes `StageResultHeader`.

## Stage Result Registry

| Stage | Input | Output | Required Result Type |
|---|---|---|---|
| `requested` | transport input | `SourceRequest` | `StageExecutionResult<SourceRequest>` |
| `resolving` | `SourceRequest` | `ResolvedSource` | `StageExecutionResult<ResolvedSource>` |
| `routing` | `ResolvedSource` | `RoutePlan` | `StageExecutionResult<RoutePlan>` |
| `authorizing` | `RoutePlan` | security decision | `AuthorizationResult` |
| `planning` | `RoutePlan` | `SourcePlan` | `StageExecutionResult<SourcePlan>` |
| `leasing` | `SourcePlan` | lease guard | `LeaseResult` |
| `discovering` | `SourcePlan` | `SourceManifest` | `StageExecutionResult<SourceManifest>` |
| `diffing` | `SourceManifest` | `SourceManifestDiff` | `SourceManifestDiff` |
| `fetching` | `SourceManifestDiff` | `SourceAcquisition` | `SourceAcquisition` |
| `enriching` | `AcquiredSourceItem` | `SourceEnrichment` | `SourceEnrichment` |
| `normalizing` | acquired/enriched item | `SourceDocument` | `StageExecutionResult<SourceDocument>` |
| `parsing` | `SourceDocument` | facts/candidates | `ParseResult` |
| `graphing` | `GraphCandidate[]` | graph write result | `GraphWriteResult` |
| `preparing` | `SourceDocument` | `PreparedDocument` | `StageExecutionResult<PreparedDocument>` |
| `batching` | prepared docs | batches | `StageExecutionResult<EmbeddingBatch>` |
| `embedding` | `EmbeddingBatch` | `EmbeddingResult` | `EmbeddingResult` |
| `vectorizing` | embeddings + prepared chunks | `VectorPointBatch` | `StageExecutionResult<VectorPointBatch>` |
| `upserting` | `VectorPointBatch` | write result | `VectorStoreWriteResult` |
| `publishing` | publish plan | publish result | `PublishGenerationResult` |
| `cleaning` | cleanup debt | cleanup result | `CleanupDebtResult` |
| `complete` | terminal stage results | `SourceResult` | `SourceResult` |

## Required Stage Result Fields

Every stage result includes:

```rust
pub struct StageResultHeader {
    pub job_id: JobId,
    pub stage_id: StageId,
    pub phase: PipelinePhase,
    pub status: LifecycleStatus,
    pub started_at: Timestamp,
    pub completed_at: Option<Timestamp>,
    pub counts: StageCounts,
    pub warnings: Vec<SourceWarning>,
    pub error: Option<SourceError>,
}

pub struct StageExecutionResult<T> {
    pub header: StageResultHeader,
    pub data: T,
}
```

## Concrete Stage Results

```rust
pub struct SourceAcquisition {
    pub header: StageResultHeader,
    pub source_id: SourceId,
    pub generation: SourceGenerationId,
    pub adapter: AdapterRef,
    pub scope: SourceScope,
    pub manifest: SourceManifest,
    pub fetched_items: Vec<AcquiredSourceItem>,
    pub artifacts: Vec<ArtifactRef>,
}

pub struct AuthorizationResult {
    pub header: StageResultHeader,
    pub source_id: Option<SourceId>,
    pub decision: SecurityDecision,
    pub caller: CallerContext,
}

pub struct LeaseResult {
    pub header: StageResultHeader,
    pub lease_key: String,
    pub acquired: bool,
    pub owner: String,
    pub expires_at: Timestamp,
}

pub struct SourceManifestDiff {
    pub header: StageResultHeader,
    pub source_id: SourceId,
    pub previous_generation: Option<SourceGenerationId>,
    pub next_generation: SourceGenerationId,
    pub added: Vec<ManifestItem>,
    pub modified: Vec<ManifestItem>,
    pub removed: Vec<ManifestItem>,
    pub unchanged: Vec<ManifestItem>,
    pub skipped: Vec<ManifestItemFailure>,
    pub failed: Vec<ManifestItemFailure>,
    pub counts: DiffCounts,
}

pub struct SourceEnrichment {
    pub header: StageResultHeader,
    pub source_id: SourceId,
    pub source_item_key: SourceItemKey,
    pub enrichment_kind: EnrichmentKind,
    pub status: EnrichmentStatus,
    pub metadata: MetadataMap,
    pub parse_hints: Vec<ParserHint>,
    pub chunk_hints: Vec<ChunkHint>,
    pub graph_candidates: Vec<GraphCandidate>,
    pub artifacts: Vec<ArtifactRef>,
}

pub struct ParseResult {
    pub header: StageResultHeader,
    pub document_id: DocumentId,
    pub facts: Vec<SourceParseFacts>,
    pub graph_candidates: Vec<GraphCandidate>,
    pub parser_id: String,
    pub parser_version: String,
}

pub struct GraphWriteResult {
    pub header: StageResultHeader,
    pub source_id: SourceId,
    pub candidates_seen: u64,
    pub nodes_upserted: u64,
    pub edges_upserted: u64,
    pub evidence_records: u64,
    pub warnings: Vec<SourceWarning>,
}

pub struct VectorStoreWriteResult {
    pub header: StageResultHeader,
    pub collection: String,
    pub points_attempted: u64,
    pub points_written: u64,
    pub payload_indexes_created: Vec<String>,
    pub usage: ProviderUsage,
}

pub struct PublishGenerationResult {
    pub header: StageResultHeader,
    pub source_id: SourceId,
    pub generation: SourceGenerationId,
    pub published_at: Timestamp,
    pub document_count: u64,
    pub chunk_count: u64,
    pub vector_point_count: u64,
    pub cleanup_debt: Vec<CleanupDebtId>,
}

pub struct CleanupDebtResult {
    pub header: StageResultHeader,
    pub debt_id: CleanupDebtId,
    pub kind: CleanupDebtKind,
    pub status: LifecycleStatus,
    pub items_attempted: u64,
    pub items_cleaned: u64,
    pub next_retry_at: Option<Timestamp>,
}

pub struct StageCounts {
    pub items_total: Option<u64>,
    pub items_done: u64,
    pub documents_total: Option<u64>,
    pub documents_done: u64,
    pub chunks_total: Option<u64>,
    pub chunks_done: u64,
    pub bytes_total: Option<u64>,
    pub bytes_done: u64,
}

pub struct ManifestItemFailure {
    pub item: ManifestItem,
    pub error: SourceError,
}

pub struct DiffCounts {
    pub added: u64,
    pub modified: u64,
    pub removed: u64,
    pub unchanged: u64,
    pub skipped: u64,
    pub failed: u64,
}
```

## Degradation Rules

- Optional enrichment can degrade without failing the generation.
- Required fetch, prepare, embed, vector write, and publish failures keep the
  generation uncommitted unless source policy says otherwise.
- Item-level failures are stored in the stage result and in ledger item status.
- Cleanup failures produce `CleanupDebt` and do not unpublish a completed
  generation.

## Event Emission

Every stage emits:

- `stage_started`
- progress events for batches/items
- `stage_completed`, `stage_degraded`, or `stage_failed`

Events use `SourceProgressEvent` and include the stage result id or stage id.

## Completion Checklist

- every stage has a concrete result type
- payload DTOs appear as stage results only through `StageExecutionResult<T>` or
  a concrete result wrapper with `StageResultHeader`
- every result has success/degraded/failed fixtures
- every result updates job progress
- every result can be rendered by CLI/MCP/REST status surfaces
