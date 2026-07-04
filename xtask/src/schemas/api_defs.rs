use serde_json::Value;

pub fn api_source_schema_defs() -> Vec<(&'static str, Value)> {
    let mut defs = source_lifecycle_defs();
    defs.extend(source_document_defs());
    defs.extend(source_job_defs());
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
    &[
        "SourceRequest",
        "SourceResult",
        "ResolvedSource",
        "SourceGeneration",
        "PreparedDocument",
        "PreparedChunk",
        "EmbeddingBatch",
        "EmbeddingInput",
        "EmbeddingResult",
        "VectorPointBatch",
        "VectorPoint",
        "PayloadIndexSpec",
        "CollectionSpec",
        "VectorDeleteSelector",
        "VectorSearchRequest",
        "VectorSearchResult",
        "VectorSearchMatch",
        "JobCreateRequest",
        "JobDescriptor",
        "JobSummary",
        "SourceJobStatus",
        "JobAttemptSnapshot",
        "JobStageSnapshot",
        "JobStatusUpdate",
        "JobEvent",
        "JobEventListRequest",
        "JobEventPage",
        "JobHeartbeat",
        "JobCancelRequest",
        "JobCancelResult",
        "JobRetryRequest",
        "JobRetryResult",
        "JobRecoveryRequest",
        "JobRecoveryResult",
        "JobCleanupRequest",
        "JobCleanupResult",
        "JobArtifactListRequest",
        "JobArtifactListResult",
    ]
}

fn source_lifecycle_defs() -> Vec<(&'static str, Value)> {
    vec![
        schema_def::<axon_api::source::SourceRequest>("SourceRequest"),
        schema_def::<axon_api::source::SourceResult>("SourceResult"),
        schema_def::<axon_api::source::ResolvedSource>("ResolvedSource"),
        schema_def::<axon_api::source::SourceGeneration>("SourceGeneration"),
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
        schema_def::<axon_api::source::GraphEvidence>("GraphEvidence"),
    ]
}

fn source_job_defs() -> Vec<(&'static str, Value)> {
    vec![
        schema_def::<axon_api::source::JobCreateRequest>("JobCreateRequest"),
        schema_def::<axon_api::source::JobDescriptor>("JobDescriptor"),
        schema_def::<axon_api::source::JobSummary>("JobSummary"),
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
        schema_def::<axon_api::source::JobCleanupRequest>("JobCleanupRequest"),
        schema_def::<axon_api::source::JobCleanupResult>("JobCleanupResult"),
        schema_def::<axon_api::source::JobArtifactListRequest>("JobArtifactListRequest"),
        schema_def::<axon_api::source::JobArtifactListResult>("JobArtifactListResult"),
    ]
}

fn schema_def<T: schemars::JsonSchema>(name: &'static str) -> (&'static str, Value) {
    (name, schemars::schema_for!(T).into())
}
