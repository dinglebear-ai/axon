use axon_api::source::*;
use axon_embedding::batch::EmbeddingBatchBuilder;
use axon_ledger::store::LedgerStore;
use uuid::Uuid;

use super::preparation::prepare_documents;
use super::progress::{PipelineProgress, ProgressCoordinator};
use super::vector_points::{VectorPointBuild, point_batch};
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

#[allow(clippy::too_many_arguments)]
pub(super) async fn prepare_embed_publish(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    documents: Vec<SourceDocument>,
    enrichment_graph: &std::collections::BTreeMap<SourceItemKey, Vec<GraphCandidate>>,
    generation: &SourceGenerationId,
    collection: CollectionSpec,
    emitter: &SourceEventEmitter,
    coordinator: &ProgressCoordinator,
    progress: &mut PipelineProgress,
    is_final_generation_batch: bool,
) -> anyhow::Result<VectorizeResult> {
    let mut output = VectorizeResult::default();
    let source_batch_count = documents.len().div_ceil(DOCUMENT_BATCH_SIZE);
    for (source_index, source_batch) in documents.chunks(DOCUMENT_BATCH_SIZE).enumerate() {
        let is_final_source_batch =
            is_final_generation_batch && source_index + 1 == source_batch_count;
        coordinator
            .report(
                emitter,
                PipelinePhase::Preparing,
                progress.preparing_counts(),
                "preparing source documents",
            )
            .await;
        let prepared = prepare_documents(
            source_batch,
            generation,
            enrichment_graph,
            runtime.document_prepare_concurrency,
        )
        .await?;
        let chunk_count = prepared
            .iter()
            .map(|document| document.chunks.len() as u64)
            .sum();
        let counts = progress.prepared(
            source_batch.len() as u64,
            chunk_count,
            is_final_source_batch,
        );
        coordinator
            .checkpoint(
                PipelinePhase::Preparing,
                counts,
                "prepared source documents",
            )
            .await;
        let batches = chunk_batches(prepared);
        let batch_count = batches.len();
        for (batch_index, batch) in batches.into_iter().enumerate() {
            let is_final_vector_batch = is_final_source_batch && batch_index + 1 == batch_count;
            if input.plan.request.embed {
                let batch_chunks = batch
                    .iter()
                    .map(|document| document.chunks.len() as u64)
                    .sum();
                let counts = progress.batched(batch_chunks);
                coordinator
                    .report(
                        emitter,
                        PipelinePhase::Batching,
                        counts,
                        "batching prepared chunks",
                    )
                    .await;
            }
            let result = vectorize_batch(
                runtime,
                input,
                batch,
                collection.clone(),
                emitter,
                coordinator,
                progress,
                is_final_vector_batch,
            )
            .await?;
            merge_vectorize_result(&mut output, result);
        }
    }
    write_document_statuses(runtime.ledger.as_ref(), &output.document_statuses).await?;
    Ok(output)
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

#[allow(clippy::too_many_arguments)]
async fn vectorize_batch(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    documents: Vec<PreparedDocument>,
    collection: CollectionSpec,
    emitter: &SourceEventEmitter,
    coordinator: &ProgressCoordinator,
    progress: &mut PipelineProgress,
    is_final_vector_batch: bool,
) -> anyhow::Result<VectorizeResult> {
    if !input.plan.request.embed {
        return Ok(statuses_only(documents, DocumentLifecycleStatus::Prepared));
    }
    let embeddings =
        embed_prepared_batch(runtime, input, &documents, emitter, coordinator, progress).await?;
    let VectorPointBuild {
        batch: point_batch,
        skipped_redaction,
        redaction_skips_by_source_item,
        points_by_document,
    } = point_batch(collection, &documents, &embeddings)?;
    let eligible_chunks = point_batch
        .points
        .iter()
        .map(|point| point.chunk_id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    coordinator
        .report(
            emitter,
            PipelinePhase::Vectorizing,
            progress.vectorized(point_batch.points.len() as u64, is_final_vector_batch),
            "built vector point batch",
        )
        .await;
    let write =
        upsert_vector_batch(runtime, input, point_batch, emitter, coordinator, progress).await?;
    let _ = points_by_document;
    Ok(vectorize_result(
        documents,
        embeddings.warnings,
        &eligible_chunks,
        write,
        skipped_redaction,
        &redaction_skips_by_source_item,
    ))
}

#[allow(clippy::too_many_arguments)]
async fn embed_prepared_batch(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    documents: &[PreparedDocument],
    emitter: &SourceEventEmitter,
    coordinator: &ProgressCoordinator,
    progress: &mut PipelineProgress,
) -> anyhow::Result<EmbeddingResult> {
    let counts = progress.embedding_counts();
    coordinator
        .report(
            emitter,
            PipelinePhase::Embedding,
            counts.clone(),
            "embedding prepared chunks",
        )
        .await;
    let embedding_batch = embedding_batch(runtime, input, documents)?;
    let embedding_operation = format!("embed:{}", embedding_batch.batch_id.0);
    let result = reserved_call::embed(
        runtime,
        ProviderCallContext::for_phase(
            input.plan.job_id,
            input.execution.attempt,
            PipelinePhase::Embedding,
            input.execution.priority,
            embedding_operation,
        )
        .with_counts(counts),
        embedding_batch,
    )
    .await?;
    coordinator
        .checkpoint(
            PipelinePhase::Embedding,
            progress.embedded(result.vectors.len() as u64),
            "embedded prepared chunks",
        )
        .await;
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
async fn upsert_vector_batch(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    batch: VectorPointBatch,
    emitter: &SourceEventEmitter,
    coordinator: &ProgressCoordinator,
    progress: &mut PipelineProgress,
) -> anyhow::Result<VectorStoreWriteResult> {
    let counts = progress.upserting_counts();
    coordinator
        .report(
            emitter,
            PipelinePhase::Upserting,
            counts.clone(),
            "upserting vector point batch",
        )
        .await;
    let expected_points = batch.points.len() as u64;
    let upsert_operation = format!("upsert:{}", batch.batch_id.0);
    let write = reserved_call::upsert(
        runtime,
        ProviderCallContext::for_phase(
            input.plan.job_id,
            input.execution.attempt,
            PipelinePhase::Upserting,
            input.execution.priority,
            upsert_operation,
        )
        .with_counts(counts),
        batch,
    )
    .await?;
    validate_upsert_counts(
        expected_points,
        write.points_attempted,
        write.points_written,
    )?;
    coordinator
        .checkpoint(
            PipelinePhase::Upserting,
            progress.upserted(write.points_written),
            "upserted vector point batch",
        )
        .await;
    Ok(write)
}

fn vectorize_result(
    documents: Vec<PreparedDocument>,
    embedding_warnings: Vec<SourceWarning>,
    eligible_chunks: &std::collections::BTreeSet<ChunkId>,
    write: VectorStoreWriteResult,
    skipped_redaction: u64,
    redaction_skips_by_source_item: &std::collections::BTreeMap<SourceItemKey, u64>,
) -> VectorizeResult {
    let point_counts = documents
        .iter()
        .map(|document| {
            let count = document
                .chunks
                .iter()
                .filter(|chunk| eligible_chunks.contains(&chunk.chunk_id))
                .count();
            (
                document.document_id.clone(),
                u32::try_from(count).unwrap_or(u32::MAX),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut result = statuses_only(documents, DocumentLifecycleStatus::Vectorized);
    result.points_written = write.points_written;
    result.warnings.extend(embedding_warnings);
    for (source_item_key, count) in redaction_skips_by_source_item {
        result.warnings.push(SourceWarning {
            code: "source.vectorize.redaction_skipped_chunks".to_string(),
            severity: Severity::Warning,
            message: format!(
                "skipped {} chunk(s) with secret-redaction-forbidden payload values \
                 (not indexed; reduced vector point count accordingly)",
                count
            ),
            source_item_key: Some(source_item_key.clone()),
            retryable: false,
        });
    }
    let attributed_skips = redaction_skips_by_source_item
        .values()
        .copied()
        .sum::<u64>();
    if skipped_redaction > attributed_skips {
        result.warnings.push(SourceWarning {
            code: "source.vectorize.redaction_skipped_chunks".to_string(),
            severity: Severity::Warning,
            message: format!(
                "skipped {} unattributed chunk(s) with secret-redaction-forbidden payload values \
                 (not indexed; reduced vector point count accordingly)",
                skipped_redaction - attributed_skips
            ),
            source_item_key: None,
            retryable: false,
        });
    }
    for status in &mut result.document_statuses {
        status.vector_point_count = point_counts.get(&status.document_id).copied().unwrap_or(0);
    }
    result
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
