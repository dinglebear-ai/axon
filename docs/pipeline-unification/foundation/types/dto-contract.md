# DTO Contract
Last Modified: 2026-06-30

## Contract

DTOs are the transport-neutral data shapes owned by `axon-api`. CLI, MCP, REST,
jobs, watches, stores, providers, and services use these shapes directly or
convert into them at the transport boundary. No transport may invent an
alternate domain DTO for the same concept.

## Rules

- DTO names are CamelCase Rust structs.
- Serialized JSON fields are snake_case.
- External request DTOs use `serde(deny_unknown_fields)` unless an explicit
  `options` or `metadata` map is the intended extension point.
- IDs use typed aliases/newtypes, not naked strings in implementation.
- Large content uses `ContentRef` or `ArtifactRef`, not inline giant strings.
- Secrets never appear in DTOs unless represented as `SecretRef` or redacted
  labels.
- DTOs do not contain provider clients, file handles, database connections, or
  transport response objects.

## Common Scalar Types

```rust
pub struct JobId(pub Uuid);
pub struct StageId(pub Uuid);
pub struct SourceId(pub String);
pub struct SourceItemKey(pub String);
pub struct SourceGenerationId(pub String);
pub struct DocumentId(pub String);
pub struct ChunkId(pub String);
pub struct BatchId(pub Uuid);
pub struct ProviderId(pub String);
pub struct ArtifactId(pub String);
pub struct CleanupDebtId(pub String);
pub struct WatchId(pub String);
pub struct MemoryId(pub String);
pub struct GraphNodeId(pub String);
pub struct GraphEdgeId(pub String);
pub struct ConfigSnapshotId(pub String);
pub type Timestamp = time::OffsetDateTime;
pub type MetadataMap = serde_json::Map<String, serde_json::Value>;
```

## Supporting DTOs

These support the primary DTOs below. They are not optional implementation
details; if a primary DTO references one of these names, `axon-api` owns the
shape.

```rust
pub struct ExecutionPolicy {
    pub mode: ExecutionMode,
    pub wait_timeout_secs: Option<u64>,
    pub priority: JobPriority,
    pub detached: bool,
    pub heartbeat_interval_secs: u64,
}

pub struct OutputPolicy {
    pub json: bool,
    pub response_mode: ResponseMode,
    pub inline_limit_bytes: u64,
    pub artifact_mode: ArtifactMode,
    pub include_progress: bool,
}

pub struct SourceLimits {
    pub max_items: Option<u64>,
    pub max_pages: Option<u64>,
    pub max_depth: Option<u32>,
    pub max_bytes_per_item: Option<u64>,
    pub max_total_bytes: Option<u64>,
    pub max_chunks: Option<u64>,
    pub provider_timeout_ms: Option<u64>,
}

pub struct EffectiveLimits {
    pub request: SourceLimits,
    pub adapter_defaults: SourceLimits,
    pub config_defaults: SourceLimits,
    pub effective: SourceLimits,
}

pub struct AdapterOptions {
    pub values: MetadataMap,
}

pub struct AuthorityHint {
    pub canonical_uri: Option<String>,
    pub authority: AuthorityLevel,
    pub evidence: Vec<AuthorityEvidence>,
}

pub struct AdapterCandidate {
    pub adapter: AdapterRef,
    pub supported_scopes: Vec<SourceScope>,
    pub confidence: f32,
    pub reason: String,
}

pub struct AdapterRef {
    pub name: String,
    pub version: String,
}

pub struct ProviderRequirement {
    pub provider_kind: ProviderKind,
    pub capability: String,
    pub required: bool,
    pub reason: String,
}

pub struct CredentialRequirement {
    pub credential_kind: CredentialKind,
    pub secret_ref: Option<SecretRef>,
    pub required: bool,
    pub reason: String,
}

pub struct ChunkHint {
    pub profile: ChunkProfile,
    pub reason: String,
    pub options: MetadataMap,
}

pub struct ParserHint {
    pub parser_id: String,
    pub reason: String,
    pub options: MetadataMap,
}

pub struct JobStagePlan {
    pub phase: PipelinePhase,
    pub required: bool,
    pub provider_requirements: Vec<ProviderRequirement>,
    pub estimated_items: Option<u64>,
}

pub struct ProviderReservationRequest {
    pub provider_kind: ProviderKind,
    pub priority: JobPriority,
    pub units: u32,
    pub reason: String,
}

pub struct SourceWarning {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub source_item_key: Option<SourceItemKey>,
    pub retryable: bool,
}

pub struct SourceError {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub source_item_key: Option<SourceItemKey>,
    pub retryable: bool,
    pub provider_id: Option<ProviderId>,
    pub cause: Option<String>,
}

pub enum ContentRef {
    InlineText { text: String },
    InlineBytes { bytes_base64: String, mime_type: String },
    Artifact { artifact_id: ArtifactId },
    External { uri: String, integrity: Option<String> },
}

pub struct ArtifactRef {
    pub artifact_id: ArtifactId,
    pub artifact_kind: ArtifactKind,
    pub uri: String,
    pub size_bytes: Option<u64>,
    pub content_hash: Option<String>,
    pub created_at: Timestamp,
}

pub struct FetchPlan {
    pub uri: String,
    pub method: String,
    pub headers: RedactedHeaders,
    pub render_required: bool,
    pub cache_policy: CachePolicy,
}

pub struct RedactedHeaders {
    pub headers: Vec<RedactedHeader>,
}

pub struct RedactedHeader {
    pub name: String,
    pub value: String,
    pub redacted: bool,
}

pub struct SourceRange {
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    pub byte_start: Option<u64>,
    pub byte_end: Option<u64>,
    pub dom_selector: Option<String>,
    pub json_pointer: Option<String>,
    pub yaml_path: Option<String>,
    pub xml_path: Option<String>,
    pub csv_row_start: Option<u32>,
    pub csv_row_end: Option<u32>,
    pub time_start_ms: Option<u64>,
    pub time_end_ms: Option<u64>,
    pub session_turn_start: Option<u32>,
    pub session_turn_end: Option<u32>,
}

pub struct ChunkLocator {
    pub canonical_uri: String,
    pub path: Option<String>,
    pub heading_path: Vec<String>,
    pub symbol: Option<String>,
    pub range: SourceRange,
}

pub struct CleanupKey {
    pub kind: CleanupDebtKind,
    pub selector: CleanupSelector,
}

pub struct GraphRef {
    pub node_id: Option<GraphNodeId>,
    pub edge_id: Option<GraphEdgeId>,
    pub candidate_id: Option<String>,
}

pub struct SourceCounts {
    pub items_total: u64,
    pub items_changed: u64,
    pub documents_total: u64,
    pub chunks_total: u64,
    pub vector_points_total: u64,
    pub bytes_total: u64,
}

pub struct LedgerSummary {
    pub source_id: SourceId,
    pub generation: SourceGenerationId,
    pub committed_generation: Option<SourceGenerationId>,
    pub status: LifecycleStatus,
    pub counts: SourceCounts,
}

pub struct GraphWriteSummary {
    pub nodes_upserted: u64,
    pub edges_upserted: u64,
    pub evidence_records: u64,
    pub degraded: bool,
}
```

## Capability and Service Support DTOs

Every trait/service support type referenced by `trait-contract.md`,
`service-contract.md`, or `store-contract.md` is owned by `axon-api`.

```rust
pub struct CapabilityBase {
    pub name: String,
    pub version: String,
    pub owner_crate: String,
    pub health: HealthStatus,
    pub features: Vec<String>,
    pub limits: MetadataMap,
}

pub type SourceResolverCapability = CapabilityBase;
pub type SourceRouterCapability = CapabilityBase;
pub type SourceAdapterCapability = CapabilityBase;
pub type SourceScopeCapability = CapabilityBase;
pub type SourceEnricherCapability = CapabilityBase;
pub type DocumentPreparerCapability = CapabilityBase;
pub type ChunkProfileCapability = CapabilityBase;
pub type ParserCapability = CapabilityBase;
pub type RetrievalCapability = CapabilityBase;
pub type LedgerStoreCapability = CapabilityBase;
pub type GraphStoreCapability = CapabilityBase;
pub type MemoryStoreCapability = CapabilityBase;
pub type JobStoreCapability = CapabilityBase;
pub type WatchStoreCapability = CapabilityBase;
pub type ArtifactStoreCapability = CapabilityBase;
pub type ConfigStoreCapability = CapabilityBase;
pub type DocumentCacheCapability = CapabilityBase;

pub struct ValidatedOptions {
    pub adapter: AdapterRef,
    pub scope: SourceScope,
    pub options: AdapterOptions,
    pub warnings: Vec<SourceWarning>,
}

pub struct AskContext {
    pub question: String,
    pub documents: Vec<DocumentId>,
    pub chunks: Vec<ChunkId>,
    pub graph_refs: Vec<GraphRef>,
    pub token_budget: u32,
    pub metadata: MetadataMap,
}

// CAS discipline: optimistic concurrency keys off `expected_previous_generation`
// (the generation id the caller believes is currently published), not a
// status enum or job_id. `None` means "no generation published yet".
pub struct PublishGenerationRequest {
    pub source_id: SourceId,
    pub generation: SourceGenerationId,
    pub expected_previous_generation: Option<SourceGenerationId>,
}

pub struct PublishPlan {
    pub source_id: SourceId,
    pub generation: SourceGenerationId,
    pub previous_generation: Option<SourceGenerationId>,
    pub ready: bool,
    pub estimated_document_count: u64,
    pub estimated_chunk_count: u64,
    pub cleanup_debt_preview: Vec<CleanupDebtId>,
    pub warnings: Vec<SourceWarning>,
}

pub struct JobCreateRequest {
    pub job_id: JobId,
    pub kind: JobKind,
    pub priority: JobPriority,
    pub request: serde_json::Value,
    pub auth_snapshot: AuthSnapshot,
    pub config_snapshot_id: ConfigSnapshotId,
}

pub struct AuthSnapshot {
    pub caller_id: Option<String>,
    pub transport: TransportKind,
    pub scopes: Vec<String>,
    pub visibility_ceiling: Visibility,
    pub requested_at: Timestamp,
    pub policy_version: String,
}
```

## Source Lifecycle DTOs

```rust
pub struct SourceRequest {
    pub source: String,
    pub intent: SourceIntent,
    pub embed: bool,
    pub refresh: SourceRefreshPolicy,
    pub watch: SourceWatchPolicy,
    pub execution: ExecutionPolicy,
    pub output: OutputPolicy,
    pub limits: SourceLimits,
    pub options: AdapterOptions,
    pub scope: Option<SourceScope>,
    pub collection: Option<String>,
    pub adapter: Option<String>,
    pub authority_hint: Option<AuthorityHint>,
    pub metadata: MetadataMap,
    pub idempotency_key: Option<String>,
}

pub struct ResolvedSource {
    pub requested_uri: String,
    pub canonical_uri: String,
    pub source_id: SourceId,
    pub source_kind: SourceKind,
    pub display_name: String,
    pub candidate_adapters: Vec<AdapterCandidate>,
    pub default_scope: SourceScope,
    pub available_scopes: Vec<SourceScope>,
    pub authority: AuthorityLevel,
    pub confidence: f32,
    pub reason: String,
    pub warnings: Vec<SourceWarning>,
}

pub struct RoutePlan {
    pub source: ResolvedSource,
    pub adapter: AdapterRef,
    pub scope: SourceScope,
    pub provider_requirements: Vec<ProviderRequirement>,
    pub credential_requirements: Vec<CredentialRequirement>,
    pub execution_affinity: ExecutionAffinity,
    pub safety_class: SafetyClass,
    pub option_schema_id: String,
    pub validated_options: AdapterOptions,
    pub chunking_hints: Vec<ChunkHint>,
    pub parser_hints: Vec<ParserHint>,
    pub graph_fact_kinds: Vec<String>,
    pub watch_supported: bool,
    pub refresh_supported: bool,
}

pub struct SourcePlan {
    pub job_id: JobId,
    pub request: SourceRequest,
    pub route: RoutePlan,
    pub stage_plan: Vec<JobStagePlan>,
    pub limits: EffectiveLimits,
    pub config_snapshot_id: ConfigSnapshotId,
    pub provider_reservations: Vec<ProviderReservationRequest>,
}

pub struct SourceResult {
    pub job_id: JobId,
    pub source_id: SourceId,
    pub canonical_uri: String,
    pub source_kind: SourceKind,
    pub adapter: AdapterRef,
    pub scope: SourceScope,
    pub status: LifecycleStatus,
    pub ledger: LedgerSummary,
    pub graph: GraphWriteSummary,
    pub counts: SourceCounts,
    pub warnings: Vec<SourceWarning>,
    pub inline: Option<InlineSourceResult>,
    pub job: Option<JobDescriptor>,
    pub watch: Option<WatchResult>,
    pub artifacts: Vec<ArtifactRef>,
    pub errors: Vec<SourceError>,
}
```

## Manifest and Acquisition DTOs

```rust
pub struct ManifestItem {
    pub source_id: SourceId,
    pub source_item_key: SourceItemKey,
    pub canonical_uri: String,
    pub item_kind: ItemKind,
    pub content_kind: Option<ContentKind>,
    pub display_path: Option<String>,
    pub parent_key: Option<SourceItemKey>,
    pub size_bytes: Option<u64>,
    pub content_hash: Option<String>,
    pub mtime: Option<Timestamp>,
    pub version: Option<String>,
    pub fetch_plan: Option<FetchPlan>,
    pub metadata: MetadataMap,
    pub graph_hints: Vec<GraphCandidate>,
}

pub struct SourceManifest {
    pub source_id: SourceId,
    pub generation: SourceGenerationId,
    pub adapter: AdapterRef,
    pub scope: SourceScope,
    pub items: Vec<ManifestItem>,
    pub created_at: Timestamp,
    pub metadata: MetadataMap,
}

pub struct AcquiredSourceItem {
    pub manifest_item: ManifestItem,
    pub fetch_status: LifecycleStatus,
    pub content_ref: ContentRef,
    pub raw_artifact_id: Option<ArtifactId>,
    pub headers: RedactedHeaders,
    pub fetched_at: Timestamp,
    pub metadata: MetadataMap,
}
```

## Parse and Graph DTOs

```rust
pub struct SourceParseFacts {
    pub document_id: DocumentId,
    pub source_item_key: SourceItemKey,
    pub fact_kind: String,
    pub name: String,
    pub value: serde_json::Value,
    pub range: Option<SourceRange>,
    pub confidence: f32,
    pub metadata: MetadataMap,
}

pub struct GraphCandidate {
    pub candidate_id: String,
    pub job_id: JobId,
    pub source_id: SourceId,
    pub source_item_key: SourceItemKey,
    pub item_canonical_uri: String,
    pub document_id: Option<DocumentId>,
    pub producer: GraphCandidateProducer,
    pub nodes: Vec<GraphNodeCandidate>,
    pub edges: Vec<GraphEdgeCandidate>,
    pub evidence: Vec<GraphEvidence>,
    pub confidence: f32,
    pub metadata: MetadataMap,
}

pub struct GraphCandidateProducer {
    pub adapter: String,
    pub parser: Option<String>,
    pub version: String,
}

pub struct GraphNodeCandidate {
    pub node_kind: String,
    pub stable_key: String,
    pub label: String,
    pub properties: MetadataMap,
}

pub struct GraphEdgeCandidate {
    pub edge_kind: String,
    pub from_stable_key: String,
    pub to_stable_key: String,
    pub evidence_ids: Vec<String>,
    pub properties: MetadataMap,
}
```

`sources/source-graph.md` is the canonical graph kind registry. Generated graph
schemas must use its node and edge kind names exactly. `GraphCandidate` is the
only parser/adapter candidate DTO; alternate single-node/single-edge or
`kind`-only shorthand shapes are invalid.

## Document DTOs

```rust
pub struct SourceDocument {
    pub document_id: DocumentId,
    pub source_id: SourceId,
    pub source_item_key: SourceItemKey,
    pub canonical_uri: String,
    pub content_kind: ContentKind,
    pub content: ContentRef,
    pub metadata: MetadataMap,
    pub title: Option<String>,
    pub language: Option<String>,
    pub path: Option<String>,
    pub mime_type: Option<String>,
    pub structured_payload: Option<serde_json::Value>,
    pub artifact_id: Option<ArtifactId>,
    pub chunk_hints: Vec<ChunkHint>,
    pub parser_hints: Vec<ParserHint>,
}

pub struct PreparedDocument {
    pub document_id: DocumentId,
    pub source_id: SourceId,
    pub source_item_key: SourceItemKey,
    pub generation: SourceGenerationId,
    pub chunks: Vec<PreparedChunk>,
    pub metadata: MetadataMap,
    pub cleanup_keys: Vec<CleanupKey>,
    pub graph_refs: Vec<GraphRef>,
}

pub struct PreparedChunk {
    pub chunk_id: ChunkId,
    pub document_id: DocumentId,
    pub chunk_index: u32,
    pub chunk_text: String,
    pub chunk_hash: String,
    pub chunk_locator: ChunkLocator,
    pub source_range: SourceRange,
    pub content_kind: ContentKind,
    pub metadata: MetadataMap,
}

pub struct DocumentStatus {
    pub document_id: DocumentId,
    pub source_id: SourceId,
    pub source_item_key: SourceItemKey,
    pub generation: SourceGenerationId,
    pub status: DocumentLifecycleStatus,
    pub updated_at: Timestamp,
    pub chunk_count: u32,
    pub vector_point_count: u32,
    pub error: Option<SourceError>,
    pub cleanup_status: Option<LifecycleStatus>,
}
```

## Embedding and Vector DTOs

```rust
pub struct EmbeddingBatch {
    pub batch_id: BatchId,
    pub job_id: JobId,
    pub provider_id: ProviderId,
    pub model: String,
    pub items: Vec<EmbeddingInput>,
    pub instruction: Option<String>,
    pub priority: JobPriority,
    pub metadata: MetadataMap,
}

pub struct EmbeddingResult {
    pub batch_id: BatchId,
    pub model: String,
    pub dimensions: u32,
    pub vectors: Vec<EmbeddingVector>,
    pub usage: ProviderUsage,
    pub warnings: Vec<SourceWarning>,
}

pub struct VectorPointBatch {
    pub batch_id: BatchId,
    pub collection: String,
    pub points: Vec<VectorPoint>,
    pub model: String,
    pub dimensions: u32,
    pub sparse_vectors: Option<Vec<SparseVector>>,
    pub payload_indexes: Vec<PayloadIndexSpec>,
}

pub struct EmbeddingInput {
    pub chunk_id: ChunkId,
    pub text: String,
    pub content_kind: ContentKind,
    pub metadata: MetadataMap,
}

pub struct EmbeddingVector {
    pub chunk_id: ChunkId,
    pub values: Vec<f32>,
}

pub struct SparseVector {
    pub chunk_id: ChunkId,
    pub indices: Vec<u32>,
    pub values: Vec<f32>,
}

pub struct VectorPoint {
    pub point_id: String,
    pub chunk_id: ChunkId,
    pub vector: Vec<f32>,
    pub sparse_vector: Option<SparseVector>,
    pub payload: MetadataMap,
}

pub struct PayloadIndexSpec {
    pub field_name: String,
    pub field_schema: PayloadFieldSchema,
    pub required_for_filters: bool,
}

pub struct ProviderUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub requests: u64,
    pub duration_ms: u64,
}
```

## State and Cleanup DTOs

```rust
pub struct SourceGeneration {
    pub source_id: SourceId,
    pub generation: SourceGenerationId,
    pub status: LifecycleStatus,
    pub created_at: Timestamp,
    pub published_at: Option<Timestamp>,
    pub item_counts: ItemCounts,
    pub document_counts: DocumentCounts,
    pub cleanup_debt: Vec<CleanupDebtId>,
    pub previous_generation: Option<SourceGenerationId>,
}

pub struct CleanupDebt {
    pub debt_id: CleanupDebtId,
    pub job_id: JobId,
    pub source_id: SourceId,
    pub generation: Option<SourceGenerationId>,
    pub kind: CleanupDebtKind,
    pub selector: CleanupSelector,
    pub status: LifecycleStatus,
    pub created_at: Timestamp,
    pub attempts: u32,
    pub last_error: Option<SourceError>,
    pub next_retry_at: Option<Timestamp>,
    pub completed_at: Option<Timestamp>,
}
```

## Completion Checklist

- every DTO has serde round-trip tests
- every DTO has a JSON schema snapshot when exposed externally
- every request DTO rejects unknown fields unless explicitly documented
- every DTO with IDs uses typed IDs in code
- every DTO with metadata follows `metadata-payload.md`
- every DTO exposed by REST/MCP/CLI is owned by `axon-api`
