use axon_api::source::*;
use axon_document::{DocumentPreparer, PrepareSourceDocumentRequest};
use axon_embedding::batch::EmbeddingBatchBuilder;
use axon_embedding::provider::EmbeddingProvider;
use axon_embedding::reservation::ProviderReservationContext;
use axon_ledger::store::LedgerStore;
use axon_vectors::store::VectorStore;
use uuid::Uuid;

use crate::source::events::SourceEventEmitter;

use super::WebSourceIndexInput;
use super::artifacts::{WebArtifactIndex, cleanup_artifacts_after_error};
use super::normalize::{NormalizedWebDocuments, normalize_changed_documents};
use super::progress::{WebPipelineProgress, WebProgressCoordinator};
use super::run::{WebAdapterRun, timestamp};
use super::vectorize_helpers::{
    VectorPointBuild, changed_diff_batches, document_status, payload_index,
    prepared_document_batches, sanitize_web_payload_metadata, take_vertical_parse_artifacts,
    vector_point_batch_for_documents, vectorized_document_status,
};

#[path = "vectorize_pipeline.rs"]
mod pipeline;
#[path = "vectorize_provider.rs"]
mod provider;

pub(super) use pipeline::{prepare_changed_documents_without_vectors, vectorize_changed_documents};

const WEB_CHANGED_DOCUMENT_BATCH_SIZE: usize = 64;
const WEB_CHANGED_CHUNK_BATCH_SIZE: usize = 512;

#[derive(Debug, Clone, Default)]
pub(super) struct VectorizeResult {
    pub(super) documents_prepared: u64,
    pub(super) chunks_prepared: u64,
    /// Publish-stage count: vector points actually upserted (post-skip). Fed
    /// to the publish invariant instead of `chunks_prepared`.
    pub(super) points_attempted: u64,
    pub(super) document_statuses: Vec<DocumentStatus>,
    pub(super) reused_item_keys: Vec<SourceItemKey>,
    /// Parser-produced graph candidates carried by each prepared document
    /// (populated by `DocumentPreparer`'s self-parse when the caller supplies
    /// no pre-computed facts). Collected here so the graphing stage
    /// (`source::graph::write_baseline_graph`) can write them instead of
    /// silently dropping them after vectorization.
    pub(super) graph_candidates: Vec<GraphCandidate>,
    pub(super) warnings: Vec<SourceWarning>,
    pub(super) artifacts: Vec<ArtifactRef>,
    pub(super) inline: Option<InlineSourceResult>,
    pub(super) artifact_index: WebArtifactIndex,
}

pub(super) fn collection_spec(input: &WebSourceIndexInput) -> CollectionSpec {
    let mut metadata = MetadataMap::new();
    metadata.insert(
        "vector_provider_id".to_string(),
        serde_json::json!(input.vector_provider_id.0.clone()),
    );
    CollectionSpec {
        collection: input.collection.clone(),
        dense: VectorConfig {
            name: "dense".to_string(),
            dimensions: input.embedding_dimensions,
            distance: VectorDistance::Cosine,
        },
        payload_indexes: vec![
            payload_index("source_id"),
            payload_index("source_generation"),
            payload_index("source_item_key"),
            payload_index("document_id"),
            payload_index("chunk_id"),
        ],
        sparse: Some(SparseVectorConfig {
            name: "bm42".to_string(),
            modifier: SparseVectorModifier::Idf,
        }),
        aliases: Vec::new(),
        distance: Some(VectorDistance::Cosine),
        metadata,
    }
}

pub(super) fn published_status(status: &DocumentStatus) -> DocumentStatus {
    DocumentStatus {
        status: DocumentLifecycleStatus::Published,
        updated_at: timestamp(),
        ..status.clone()
    }
}

fn prepare_source_documents(
    source_documents: Vec<SourceDocument>,
    generation: &SourceGenerationId,
) -> anyhow::Result<Vec<PreparedDocument>> {
    let preparer = DocumentPreparer::default();
    let mut documents = Vec::with_capacity(source_documents.len());
    for mut document in source_documents {
        let item_key = document.source_item_key.0.clone();
        let (parse_facts, graph_candidates) = take_vertical_parse_artifacts(&mut document);
        let mut prepared = preparer
            .prepare(PrepareSourceDocumentRequest {
                document,
                generation: generation.clone(),
                profile: None,
                parse_facts,
                graph_candidates,
                warnings: Vec::new(),
                errors: Vec::new(),
            })
            .map_err(|err| anyhow::anyhow!("failed to prepare web item {item_key}: {err}"))?
            .document;
        sanitize_web_payload_metadata(&mut prepared);
        documents.push(prepared);
    }
    Ok(documents)
}

fn embedding_batch_for_documents(
    input: &WebSourceIndexInput,
    documents: &[PreparedDocument],
) -> anyhow::Result<EmbeddingBatch> {
    let batch_id = BatchId::new(Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        documents
            .iter()
            .map(|document| document.document_id.0.as_str())
            .collect::<Vec<_>>()
            .join(":")
            .as_bytes(),
    ));
    let mut builder = EmbeddingBatchBuilder::new(
        batch_id,
        input.job_id,
        input.embedding_provider_id.clone(),
        input.embedding_model.clone(),
    )
    .priority(JobPriority::Background);
    for document in documents {
        for chunk in &document.chunks {
            builder = builder.push_input(EmbeddingInput {
                chunk_id: chunk.chunk_id.clone(),
                text: chunk
                    .embedding_text
                    .clone()
                    .unwrap_or_else(|| chunk.content.clone()),
                content_kind: chunk.content_kind,
                metadata: chunk.metadata.clone(),
            });
        }
    }
    Ok(builder.build()?)
}

#[cfg(test)]
#[path = "vectorize_tests.rs"]
mod tests;
