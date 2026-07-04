use serde_json::Value;

pub const PHASE_1_REQUIRED_API_DEFS: &[&str] = &[
    "SuccessEnvelope",
    "ErrorEnvelope",
    "Page",
    "PollDescriptor",
    "JobDescriptor",
    "SourceRequest",
    "ResolvedSource",
    "RoutePlan",
    "SourcePlan",
    "SourceResult",
    "SourceSummary",
    "SourceItem",
    "SourceItemDetail",
    "SourceManifest",
    "ManifestItem",
    "SourceManifestDiff",
    "SourceGeneration",
    "SourceGenerationSummary",
    "SourceGenerationDetail",
    "CleanupDebt",
    "SourceDocument",
    "PreparedDocument",
    "PreparedChunk",
    "DocumentSummary",
    "DocumentDetail",
    "DocumentListRequest",
    "DocumentStatus",
    "ChunkSummary",
    "ChunkDetail",
    "ChunkListRequest",
    "ChunkGetRequest",
    "SourceParseFacts",
    "GraphCandidate",
    "GraphNode",
    "GraphEdge",
    "GraphEvidence",
    "DomainListRequest",
    "DomainSummary",
    "SourceEnrichment",
    "EmbeddingBatch",
    "EmbeddingResult",
    "VectorPointBatch",
    "VectorPoint",
    "PayloadIndexSpec",
    "CollectionSpec",
    "VectorConfig",
    "SparseVector",
    "SparseVectorConfig",
    "VectorStoreDeleteResult",
    "VectorSearchRequest",
    "VectorSearchResult",
    "VectorSearchMatch",
    "SearchRequest",
    "SearchResult",
    "JobSummary",
    "JobListRequest",
    "JobEventListRequest",
    "JobEventPage",
    "JobCleanupRequest",
    "JobCleanupResult",
    "JobRecoverRequest",
    "JobRecoverResult",
    "JobClearRequest",
    "JobClearResult",
    "WatchRequest",
    "WatchResult",
    "WatchDescriptor",
    "WatchUpdateRequest",
    "WatchListRequest",
    "WatchExecRequest",
    "WatchHistoryRequest",
    "WatchHistoryResult",
    "SourceProgressEvent",
    "TraceContext",
    "ArtifactRef",
    "ArtifactListRequest",
    "ArtifactResult",
    "UploadCreateRequest",
    "UploadResult",
    "PruneRequest",
    "PruneExecuteRequest",
    "PrunePlan",
    "PruneResult",
    "CollectionListRequest",
    "CollectionResult",
    "ProviderCapability",
    "HealthReport",
    "ApiError",
    "SourceError",
    "SourceWarning",
];

#[cfg_attr(not(test), allow(dead_code))]
pub const PHASE_1_DEFERRED_API_DEFS: &[(&str, &str, &str)] = &[
    (
        "QueryRequest",
        "phase-3b-security-error-memory.md",
        "needs request/action auth policy and retrieval filter bounds",
    ),
    (
        "QueryResult",
        "phase-3b-security-error-memory.md",
        "needs bounded result content policy",
    ),
    (
        "RetrievalRequest",
        "phase-3b-security-error-memory.md",
        "needs retrieval filter and content reference policy",
    ),
    (
        "RetrievalResult",
        "phase-3b-security-error-memory.md",
        "needs artifact-backed content policy",
    ),
    (
        "AskRequest",
        "phase-3b-security-error-memory.md",
        "needs bounded prompt and synthesis policy",
    ),
    (
        "AskResult",
        "phase-3b-security-error-memory.md",
        "needs artifact-backed answer/context policy",
    ),
    (
        "ChatRequest",
        "phase-3b-security-error-memory.md",
        "needs closed ChatRole and prompt/content policy",
    ),
    (
        "ChatResult",
        "phase-3b-security-error-memory.md",
        "needs tool-call and content redaction policy",
    ),
    (
        "EvaluationRequest",
        "phase-3b-security-error-memory.md",
        "needs closed evaluation input and auth policy",
    ),
    (
        "EvaluationResult",
        "phase-3b-security-error-memory.md",
        "needs closed EvaluationVerdict",
    ),
    (
        "SuggestRequest",
        "phase-9-source-families.md",
        "needs source-family discovery contract",
    ),
    (
        "SuggestResult",
        "phase-9-source-families.md",
        "needs source-family discovery contract",
    ),
    (
        "ResearchRequest",
        "phase-7-parser-metadata-graph.md",
        "needs bounded synthesis/source content policy",
    ),
    (
        "ResearchResult",
        "phase-7-parser-metadata-graph.md",
        "needs artifact-backed answer policy",
    ),
    (
        "SummarizeRequest",
        "phase-7-parser-metadata-graph.md",
        "needs closed SummaryFormat and content bounds",
    ),
    (
        "SummarizeResult",
        "phase-7-parser-metadata-graph.md",
        "needs artifact-backed summary policy",
    ),
    (
        "EndpointDiscoveryRequest",
        "phase-9-source-families.md",
        "needs source-family discovery contract",
    ),
    (
        "EndpointDiscoveryResult",
        "phase-9-source-families.md",
        "needs source-family discovery contract",
    ),
    (
        "BrandRequest",
        "phase-5a-surface-drift-generated-artifacts.md",
        "needs generated route policy",
    ),
    (
        "BrandResult",
        "phase-5a-surface-drift-generated-artifacts.md",
        "needs artifact/redaction policy",
    ),
    (
        "DiffRequest",
        "phase-5a-surface-drift-generated-artifacts.md",
        "needs closed DiffMode and generated route policy",
    ),
    (
        "DiffResult",
        "phase-5a-surface-drift-generated-artifacts.md",
        "needs artifact-backed diff policy",
    ),
    (
        "ScreenshotRequest",
        "phase-5a-surface-drift-generated-artifacts.md",
        "needs generated route policy",
    ),
    (
        "ScreenshotResult",
        "phase-5a-surface-drift-generated-artifacts.md",
        "needs artifact-only screenshot policy",
    ),
    (
        "ExtractRequest",
        "phase-7-parser-metadata-graph.md",
        "needs explicit extract policy and prompt bounds",
    ),
    (
        "ExtractResult",
        "phase-7-parser-metadata-graph.md",
        "needs structured output artifact/redaction policy",
    ),
    (
        "DedupeRequest",
        "phase-5b-reset-preflight.md",
        "needs current destructive-operation contract",
    ),
    (
        "DedupeResult",
        "phase-5b-reset-preflight.md",
        "needs current destructive-operation contract",
    ),
    (
        "PurgeRequest",
        "phase-5b-reset-preflight.md",
        "needs prune/reset cutover contract without legacy target/prefix",
    ),
    (
        "PurgeResult",
        "phase-5b-reset-preflight.md",
        "needs prune/reset cutover contract",
    ),
];

#[cfg_attr(not(test), allow(dead_code))]
pub const PHASE_1_REQUEST_SCOPE_ENTRIES: &[(&str, &str, &str)] = &[
    ("SourceRequest", "source.submit", "write"),
    ("ArtifactListRequest", "artifact.list", "read"),
    ("UploadCreateRequest", "upload.create", "write"),
    ("PruneRequest", "prune.plan", "admin"),
    ("PruneExecuteRequest", "prune.execute", "admin"),
    ("CollectionListRequest", "collection.list", "read"),
    ("SearchRequest", "search.run", "read"),
    ("WatchRequest", "watch.create", "write"),
];

#[cfg_attr(not(test), allow(dead_code))]
pub fn request_scope_for(dto: &str, action: &str) -> Option<&'static str> {
    PHASE_1_REQUEST_SCOPE_ENTRIES
        .iter()
        .find(|(entry_dto, entry_action, _)| *entry_dto == dto && *entry_action == action)
        .map(|(_, _, scope)| *scope)
}

pub fn api_source_schema_defs() -> Vec<(&'static str, Value)> {
    let mut defs = source_lifecycle_defs();
    defs.extend(source_document_defs());
    defs.extend(source_job_defs());
    defs.extend(source_status_defs());
    defs.extend(source_operation_defs());
    defs.extend(source_provider_defs());
    defs
}

pub fn api_vector_schema_defs() -> Vec<(&'static str, Value)> {
    vec![
        schema_def::<axon_api::source::EmbeddingBatch>("EmbeddingBatch"),
        schema_def::<axon_api::source::EmbeddingInput>("EmbeddingInput"),
        schema_def::<axon_api::source::EmbeddingResult>("EmbeddingResult"),
        schema_def::<axon_api::source::EmbeddingVector>("EmbeddingVector"),
        schema_def::<axon_api::source::ProviderUsage>("ProviderUsage"),
        schema_def::<axon_api::source::VectorPointBatch>("VectorPointBatch"),
        schema_def::<axon_api::source::VectorPoint>("VectorPoint"),
        schema_def::<axon_api::source::SparseVector>("SparseVector"),
        schema_def::<axon_api::source::PayloadIndexSpec>("PayloadIndexSpec"),
        schema_def::<axon_api::source::CollectionSpec>("CollectionSpec"),
        schema_def::<axon_api::source::VectorConfig>("VectorConfig"),
        schema_def::<axon_api::source::SparseVectorConfig>("SparseVectorConfig"),
        schema_def::<axon_api::source::VectorDeleteSelector>("VectorDeleteSelector"),
        schema_def::<axon_api::source::VectorStoreDeleteResult>("VectorStoreDeleteResult"),
        schema_def::<axon_api::source::VectorSearchRequest>("VectorSearchRequest"),
        schema_def::<axon_api::source::VectorSearchResult>("VectorSearchResult"),
        schema_def::<axon_api::source::VectorSearchMatch>("VectorSearchMatch"),
        schema_def::<axon_api::source::PayloadFieldSchema>("PayloadFieldSchema"),
        schema_def::<axon_api::source::VectorDistance>("VectorDistance"),
        schema_def::<axon_api::source::SparseVectorModifier>("SparseVectorModifier"),
    ]
}

pub fn api_dto_names() -> &'static [&'static str] {
    PHASE_1_REQUIRED_API_DEFS
}

fn source_lifecycle_defs() -> Vec<(&'static str, Value)> {
    vec![
        schema_def::<axon_api::source::SourceRequest>("SourceRequest"),
        schema_def::<axon_api::source::SourceResult>("SourceResult"),
        schema_def::<axon_api::source::ResolvedSource>("ResolvedSource"),
        schema_def::<axon_api::source::RoutePlan>("RoutePlan"),
        schema_def::<axon_api::source::SourcePlan>("SourcePlan"),
        schema_def::<axon_api::source::SourceGeneration>("SourceGeneration"),
        schema_def::<axon_api::source::SourceGenerationSummary>("SourceGenerationSummary"),
        schema_def::<axon_api::source::SourceGenerationDetail>("SourceGenerationDetail"),
        schema_def::<axon_api::source::PublishGenerationRequest>("PublishGenerationRequest"),
        schema_def::<axon_api::source::CleanupDebt>("CleanupDebt"),
        schema_def::<axon_api::source::LeaseRequest>("LeaseRequest"),
        schema_def::<axon_api::source::LeaseGuard>("LeaseGuard"),
        schema_def::<axon_api::source::CleanupSelector>("CleanupSelector"),
        schema_def::<axon_api::source::DocumentStatus>("DocumentStatus"),
    ]
}

fn source_document_defs() -> Vec<(&'static str, Value)> {
    vec![
        schema_def::<axon_api::source::SourceDocument>("SourceDocument"),
        schema_def::<axon_api::source::PreparedDocument>("PreparedDocument"),
        schema_def::<axon_api::source::PreparedChunk>("PreparedChunk"),
        schema_def::<axon_api::source::ChunkLocator>("ChunkLocator"),
        schema_def::<axon_api::source::SourceParseFacts>("SourceParseFacts"),
        schema_def::<axon_api::source::GraphCandidate>("GraphCandidate"),
        schema_def::<axon_api::source::GraphNode>("GraphNode"),
        schema_def::<axon_api::source::GraphEdge>("GraphEdge"),
        schema_def::<axon_api::source::GraphEvidence>("GraphEvidence"),
        schema_def::<axon_api::source::SourceManifest>("SourceManifest"),
        schema_def::<axon_api::source::ManifestItem>("ManifestItem"),
        schema_def::<axon_api::source::SourceManifestDiff>("SourceManifestDiff"),
        schema_def::<axon_api::source::SourceEnrichment>("SourceEnrichment"),
    ]
}

fn source_job_defs() -> Vec<(&'static str, Value)> {
    vec![
        schema_def::<axon_api::source::JobCreateRequest>("JobCreateRequest"),
        schema_def::<axon_api::source::JobDescriptor>("JobDescriptor"),
        schema_def::<axon_api::source::JobSummary>("JobSummary"),
        schema_def::<axon_api::source::JobListRequest>("JobListRequest"),
        schema_def::<axon_api::source::SourceJobStatus>("SourceJobStatus"),
        schema_def::<axon_api::source::JobAttemptSnapshot>("JobAttemptSnapshot"),
        schema_def::<axon_api::source::JobStageSnapshot>("JobStageSnapshot"),
        schema_def::<axon_api::source::JobStatusUpdate>("JobStatusUpdate"),
        schema_def::<axon_api::source::JobEvent>("JobEvent"),
        schema_def::<axon_api::source::JobEventListRequest>("JobEventListRequest"),
        schema_def::<axon_api::source::JobEventPage>("JobEventPage"),
        schema_def::<axon_api::source::JobHeartbeat>("JobHeartbeat"),
        schema_def::<axon_api::source::JobCancelRequest>("JobCancelRequest"),
        schema_def::<axon_api::source::JobCancelResult>("JobCancelResult"),
        schema_def::<axon_api::source::JobRetryRequest>("JobRetryRequest"),
        schema_def::<axon_api::source::JobRetryResult>("JobRetryResult"),
        schema_def::<axon_api::source::JobRecoveryRequest>("JobRecoveryRequest"),
        schema_def::<axon_api::source::JobRecoveryResult>("JobRecoveryResult"),
        schema_def::<axon_api::source::JobRecoveryRequest>("JobRecoverRequest"),
        schema_def::<axon_api::source::JobRecoveryResult>("JobRecoverResult"),
        schema_def::<axon_api::source::JobCleanupRequest>("JobCleanupRequest"),
        schema_def::<axon_api::source::JobCleanupResult>("JobCleanupResult"),
        schema_def::<axon_api::source::JobClearRequest>("JobClearRequest"),
        schema_def::<axon_api::source::JobClearResult>("JobClearResult"),
        schema_def::<axon_api::source::JobArtifactListRequest>("JobArtifactListRequest"),
        schema_def::<axon_api::source::JobArtifactListResult>("JobArtifactListResult"),
    ]
}

fn source_status_defs() -> Vec<(&'static str, Value)> {
    vec![
        schema_def::<axon_api::source::SuccessEnvelope<axon_api::source::SourceResult>>(
            "SuccessEnvelope",
        ),
        schema_def::<axon_api::source::ErrorEnvelope>("ErrorEnvelope"),
        schema_def::<axon_api::source::Page<axon_api::source::SourceSummary>>("Page"),
        schema_def::<axon_api::source::PollDescriptor>("PollDescriptor"),
        schema_def::<axon_api::source::WatchRequest>("WatchRequest"),
        schema_def::<axon_api::source::WatchResult>("WatchResult"),
        schema_def::<axon_api::source::WatchDescriptor>("WatchDescriptor"),
        schema_def::<axon_api::source::WatchUpdateRequest>("WatchUpdateRequest"),
        schema_def::<axon_api::source::WatchListRequest>("WatchListRequest"),
        schema_def::<axon_api::source::WatchExecRequest>("WatchExecRequest"),
        schema_def::<axon_api::source::WatchHistoryRequest>("WatchHistoryRequest"),
        schema_def::<axon_api::source::WatchHistoryResult>("WatchHistoryResult"),
        schema_def::<axon_api::source::SourceProgressEvent>("SourceProgressEvent"),
        schema_def::<axon_api::source::TraceContext>("TraceContext"),
        schema_def::<axon_api::source::SourceError>("SourceError"),
        schema_def::<axon_api::source::SourceWarning>("SourceWarning"),
        schema_def::<axon_api::source::SourceSummary>("SourceSummary"),
        schema_def::<axon_api::source::SourceItem>("SourceItem"),
        schema_def::<axon_api::source::SourceItemDetail>("SourceItemDetail"),
        schema_def::<axon_api::source::DocumentSummary>("DocumentSummary"),
        schema_def::<axon_api::source::DocumentDetail>("DocumentDetail"),
        schema_def::<axon_api::source::DocumentListRequest>("DocumentListRequest"),
        schema_def::<axon_api::source::ChunkSummary>("ChunkSummary"),
        schema_def::<axon_api::source::ChunkDetail>("ChunkDetail"),
        schema_def::<axon_api::source::ChunkListRequest>("ChunkListRequest"),
        schema_def::<axon_api::source::ChunkGetRequest>("ChunkGetRequest"),
        schema_def::<axon_api::source::DomainListRequest>("DomainListRequest"),
        schema_def::<axon_api::source::DomainSummary>("DomainSummary"),
    ]
}

fn source_operation_defs() -> Vec<(&'static str, Value)> {
    vec![
        schema_def::<axon_api::source::ArtifactRef>("ArtifactRef"),
        schema_def::<axon_api::source::ArtifactListRequest>("ArtifactListRequest"),
        schema_def::<axon_api::source::ArtifactResult>("ArtifactResult"),
        schema_def::<axon_api::source::UploadCreateRequest>("UploadCreateRequest"),
        schema_def::<axon_api::source::UploadResult>("UploadResult"),
        schema_def::<axon_api::source::PruneRequest>("PruneRequest"),
        schema_def::<axon_api::source::PruneExecuteRequest>("PruneExecuteRequest"),
        schema_def::<axon_api::source::PrunePlan>("PrunePlan"),
        schema_def::<axon_api::source::PruneResult>("PruneResult"),
        schema_def::<axon_api::source::CollectionListRequest>("CollectionListRequest"),
        schema_def::<axon_api::source::CollectionResult>("CollectionResult"),
    ]
}

fn source_provider_defs() -> Vec<(&'static str, Value)> {
    vec![
        schema_def::<axon_api::source::SearchRequest>("SearchRequest"),
        schema_def::<axon_api::source::SearchResult>("SearchResult"),
        schema_def::<axon_api::source::ProviderCapability>("ProviderCapability"),
        schema_def::<axon_api::source::HealthReport>("HealthReport"),
    ]
}

fn schema_def<T: schemars::JsonSchema>(name: &'static str) -> (&'static str, Value) {
    (name, schemars::schema_for!(T).into())
}
