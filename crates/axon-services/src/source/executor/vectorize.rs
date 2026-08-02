use axon_api::source::*;
use axon_document::{DocumentPreparer, PrepareSourceDocumentRequest};
use axon_embedding::batch::EmbeddingBatchBuilder;
use axon_ledger::store::LedgerStore;
use axon_vectors::point::{VectorPointBatchBuildContext, VectorPointBatchBuilder};
use uuid::Uuid;

use super::{SourceEventEmitter, SourcePipelineInput, TargetLocalSourceRuntime, timestamp};
use crate::reserved_call::{self, ProviderCallContext};

const DOCUMENT_BATCH_SIZE: usize = 64;
const CHUNK_BATCH_SIZE: usize = 512;
const DOCUMENT_STATUS_BATCH_SIZE: usize = 64;

#[derive(Debug, Default)]
pub(super) struct VectorizeResult {
    pub(super) documents_prepared: u64,
    pub(super) chunks_prepared: u64,
    pub(super) points_written: u64,
    pub(super) document_statuses: Vec<DocumentStatus>,
    pub(super) graph_candidates: Vec<GraphCandidate>,
    pub(super) warnings: Vec<SourceWarning>,
}

struct VectorPointBuild {
    batch: VectorPointBatch,
    skipped_redaction: u64,
    points_by_document: std::collections::BTreeMap<DocumentId, u32>,
}

pub(super) async fn prepare_embed_publish(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    documents: Vec<SourceDocument>,
    enrichment_graph: &std::collections::BTreeMap<SourceItemKey, Vec<GraphCandidate>>,
    generation: &SourceGenerationId,
    collection: CollectionSpec,
    emitter: &SourceEventEmitter,
) -> anyhow::Result<VectorizeResult> {
    let mut output = VectorizeResult::default();
    for source_batch in documents.chunks(DOCUMENT_BATCH_SIZE) {
        let prepared = prepare_documents(source_batch, generation, enrichment_graph)?;
        for batch in chunk_batches(prepared) {
            let result =
                vectorize_batch(runtime, input, batch, collection.clone(), emitter).await?;
            merge_vectorize_result(&mut output, result);
        }
    }
    write_document_statuses(runtime.ledger.as_ref(), &output.document_statuses).await?;
    Ok(output)
}

fn prepare_documents(
    documents: &[SourceDocument],
    generation: &SourceGenerationId,
    enrichment_graph: &std::collections::BTreeMap<SourceItemKey, Vec<GraphCandidate>>,
) -> anyhow::Result<Vec<PreparedDocument>> {
    let preparer = DocumentPreparer::default();
    documents
        .iter()
        .cloned()
        .map(|document| {
            let item_key = document.source_item_key.0.clone();
            let graph_candidates = enrichment_graph
                .get(&document.source_item_key)
                .cloned()
                .unwrap_or_default();
            let prepared = preparer
                .prepare(PrepareSourceDocumentRequest {
                    document,
                    generation: generation.clone(),
                    profile: None,
                    parse_facts: Vec::new(),
                    graph_candidates,
                    warnings: Vec::new(),
                    errors: Vec::new(),
                })
                .map_err(|error| anyhow::anyhow!("failed to prepare {item_key}: {error}"))?
                .document;
            Ok(prepared)
        })
        .collect()
}

fn chunk_batches(documents: Vec<PreparedDocument>) -> Vec<Vec<PreparedDocument>> {
    let mut batches = Vec::new();
    let mut current = Vec::new();
    let mut chunks = 0;
    for document in documents.into_iter().flat_map(split_oversized_document) {
        let count = document.chunks.len().max(1);
        if !current.is_empty() && chunks + count > CHUNK_BATCH_SIZE {
            batches.push(std::mem::take(&mut current));
            chunks = 0;
        }
        chunks += count;
        current.push(document);
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

fn split_oversized_document(document: PreparedDocument) -> Vec<PreparedDocument> {
    if document.chunks.len() <= CHUNK_BATCH_SIZE {
        return vec![document];
    }
    let mut windows = Vec::new();
    for (index, chunks) in document.chunks.chunks(CHUNK_BATCH_SIZE).enumerate() {
        let mut window = document.clone();
        window.chunks = chunks.to_vec();
        if index > 0 {
            window.graph_candidates.clear();
            window.warnings.clear();
        }
        windows.push(window);
    }
    windows
}

pub(super) fn merge_vectorize_result(output: &mut VectorizeResult, result: VectorizeResult) {
    output.chunks_prepared = output
        .chunks_prepared
        .saturating_add(result.chunks_prepared);
    output.points_written = output.points_written.saturating_add(result.points_written);
    output.graph_candidates.extend(result.graph_candidates);
    output.warnings.extend(result.warnings);
    for status in result.document_statuses {
        if let Some(existing) = output
            .document_statuses
            .iter_mut()
            .find(|existing| existing.document_id == status.document_id)
        {
            existing.chunk_count = existing.chunk_count.saturating_add(status.chunk_count);
            existing.vector_point_count = existing
                .vector_point_count
                .saturating_add(status.vector_point_count);
            existing.updated_at = status.updated_at;
        } else {
            output.documents_prepared = output.documents_prepared.saturating_add(1);
            output.document_statuses.push(status);
        }
    }
}

async fn vectorize_batch(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    documents: Vec<PreparedDocument>,
    collection: CollectionSpec,
    emitter: &SourceEventEmitter,
) -> anyhow::Result<VectorizeResult> {
    if !input.plan.request.embed {
        return Ok(statuses_only(documents, DocumentLifecycleStatus::Prepared));
    }
    super::record_running_phase(
        runtime,
        input,
        emitter,
        PipelinePhase::Embedding,
        "embedding prepared document batch",
    )
    .await?;
    let embedding_batch = embedding_batch(runtime, input, &documents)?;
    let embedding_operation = format!("embed:{}", embedding_batch.batch_id.0);
    let embeddings = reserved_call::embed(
        runtime,
        ProviderCallContext::for_phase(
            input.plan.job_id,
            input.execution.attempt,
            PipelinePhase::Embedding,
            input.execution.priority,
            embedding_operation,
        ),
        embedding_batch,
    )
    .await?;

    super::record_running_phase(
        runtime,
        input,
        emitter,
        PipelinePhase::Upserting,
        "upserting vector point batch",
    )
    .await?;
    let VectorPointBuild {
        batch: point_batch,
        skipped_redaction,
        points_by_document,
    } = point_batch(collection, &documents, &embeddings)?;
    let expected_points = point_batch.points.len() as u64;
    let upsert_operation = format!("upsert:{}", point_batch.batch_id.0);
    let write = reserved_call::upsert(
        runtime,
        ProviderCallContext::for_phase(
            input.plan.job_id,
            input.execution.attempt,
            PipelinePhase::Upserting,
            input.execution.priority,
            upsert_operation,
        ),
        point_batch,
    )
    .await?;
    validate_upsert_counts(
        expected_points,
        write.points_attempted,
        write.points_written,
    )?;

    let mut result = statuses_only(documents, DocumentLifecycleStatus::Vectorized);
    result.points_written = write.points_written;
    result.warnings.extend(embeddings.warnings);
    if skipped_redaction > 0 {
        result.warnings.push(SourceWarning {
            code: "source.vectorize.redaction_skipped_chunks".to_string(),
            severity: Severity::Warning,
            message: format!(
                "skipped {} chunk(s) with secret-redaction-forbidden payload values \
                 (not indexed; reduced vector point count accordingly)",
                skipped_redaction
            ),
            source_item_key: None,
            retryable: false,
        });
    }
    apply_vector_point_counts(&mut result.document_statuses, &points_by_document);
    Ok(result)
}

fn statuses_only(
    documents: Vec<PreparedDocument>,
    lifecycle: DocumentLifecycleStatus,
) -> VectorizeResult {
    let mut result = VectorizeResult::default();
    for document in documents {
        result.documents_prepared += 1;
        result.chunks_prepared += document.chunks.len() as u64;
        result
            .graph_candidates
            .extend(document.graph_candidates.clone());
        result.warnings.extend(document.warnings.clone());
        let status = DocumentStatus {
            document_id: document.document_id,
            source_id: document.source_id,
            source_item_key: document.source_item_key,
            generation: Some(document.generation),
            status: lifecycle,
            updated_at: timestamp(),
            chunk_count: u32::try_from(document.chunks.len()).unwrap_or(u32::MAX),
            vector_point_count: 0,
            error: None,
            cleanup_status: None,
        };
        result.document_statuses.push(status);
    }
    result
}

pub(super) async fn write_document_statuses(
    ledger: &dyn LedgerStore,
    statuses: &[DocumentStatus],
) -> anyhow::Result<()> {
    for batch in statuses.chunks(DOCUMENT_STATUS_BATCH_SIZE) {
        ledger.update_document_statuses(batch.to_vec()).await?;
    }
    Ok(())
}

fn embedding_batch(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    documents: &[PreparedDocument],
) -> anyhow::Result<EmbeddingBatch> {
    let batch_id = BatchId::new(Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        documents
            .iter()
            .flat_map(|document| document.chunks.iter())
            .map(|chunk| chunk.chunk_id.0.as_str())
            .collect::<Vec<_>>()
            .join(":")
            .as_bytes(),
    ));
    let mut builder = EmbeddingBatchBuilder::new(
        batch_id,
        input.plan.job_id,
        runtime.embedding_provider_id.clone(),
        runtime.embedding_model.clone(),
    )
    .priority(input.execution.priority);
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

fn point_batch(
    collection: CollectionSpec,
    documents: &[PreparedDocument],
    embeddings: &EmbeddingResult,
) -> anyhow::Result<VectorPointBuild> {
    let by_chunk = embeddings
        .vectors
        .iter()
        .cloned()
        .map(|vector| (vector.chunk_id.clone(), vector))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut points = Vec::new();
    let mut skipped_redaction = 0u64;
    let mut points_by_document = std::collections::BTreeMap::new();
    for document in documents {
        let document_embeddings = EmbeddingResult {
            batch_id: embeddings.batch_id.clone(),
            job_id: embeddings.job_id,
            provider_id: embeddings.provider_id.clone(),
            model: embeddings.model.clone(),
            dimensions: embeddings.dimensions,
            vectors: document
                .chunks
                .iter()
                .filter_map(|chunk| by_chunk.get(&chunk.chunk_id).cloned())
                .collect(),
            usage: embeddings.usage.clone(),
            warnings: embeddings.warnings.clone(),
        };
        let (batch, document_skipped) = VectorPointBatchBuilder::new(
            collection.clone(),
            document.clone(),
            document_embeddings,
            VectorPointBatchBuildContext {
                embedded_at: timestamp(),
            },
        )
        .build_with_skipped_count()?;
        let document_point_count = u32::try_from(batch.points.len()).unwrap_or(u32::MAX);
        points_by_document.insert(document.document_id.clone(), document_point_count);
        points.extend(batch.points);
        skipped_redaction += document_skipped;
    }
    Ok(VectorPointBuild {
        batch: VectorPointBatch {
            batch_id: embeddings.batch_id.clone(),
            collection: collection.collection,
            points,
            model: embeddings.model.clone(),
            dimensions: embeddings.dimensions,
            sparse_vectors: None,
            payload_indexes: collection.payload_indexes,
        },
        skipped_redaction,
        points_by_document,
    })
}

fn apply_vector_point_counts(
    statuses: &mut [DocumentStatus],
    points_by_document: &std::collections::BTreeMap<DocumentId, u32>,
) {
    for status in statuses {
        status.vector_point_count = points_by_document
            .get(&status.document_id)
            .copied()
            .unwrap_or(0);
    }
}

fn validate_upsert_counts(
    expected_points: u64,
    points_attempted: u64,
    points_written: u64,
) -> anyhow::Result<()> {
    if points_attempted == expected_points && points_written == expected_points {
        return Ok(());
    }
    anyhow::bail!(
        "vector upsert short write: expected {expected_points} point(s), attempted {points_attempted}, wrote {points_written}"
    )
}

#[cfg(test)]
#[path = "vectorize_tests.rs"]
mod tests;
