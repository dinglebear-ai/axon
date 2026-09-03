use std::collections::HashMap;

use axon_api::source::*;
use axon_embedding::batch::EmbeddingBatchBuilder;
use axon_ledger::store::LedgerStore;
use uuid::Uuid;

use super::preparation::prepare_documents;
use super::progress::{PipelineProgress, ProgressCoordinator};
use super::vector_points::point_batch;
use super::{SourceEventEmitter, SourcePipelineInput, TargetLocalSourceRuntime, timestamp};
use crate::reserved_call::{self, ProviderCallContext};

pub(super) mod batching;
mod pipeline;
mod prepared_pool;

use batching::chunk_batches;
use pipeline::{embed_and_build_batch, publish_and_build_next, publish_built_batch};
pub(super) use prepared_pool::PreparedPoolVectorizer;

// Match the acquisition wave so the next web fetch overlaps this batch's
// prepare/embed/upsert work.
const DOCUMENT_BATCH_SIZE: usize = 16;
const DOCUMENT_STATUS_BATCH_SIZE: usize = 64;

fn env_batch_size(name: &str, default: usize, maximum: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
        .clamp(1, maximum)
}

#[derive(Debug, Default)]
pub(super) struct VectorizeResult {
    pub(super) documents_prepared: u64,
    pub(super) chunks_prepared: u64,
    pub(super) points_written: u64,
    pub(super) document_statuses: Vec<DocumentStatus>,
    document_status_positions: HashMap<DocumentId, usize>,
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
    let document_batch_size = env_batch_size("AXON_DOCUMENT_BATCH_SIZE", DOCUMENT_BATCH_SIZE, 1024);
    let source_batch_count = documents.len().div_ceil(document_batch_size);
    let mut documents = documents.into_iter();
    for source_index in 0..source_batch_count {
        let source_batch = documents
            .by_ref()
            .take(document_batch_size)
            .collect::<Vec<_>>();
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
        let prepared = reserved_call::parse_operation(
            runtime,
            ProviderCallContext::for_phase(
                input.plan.job_id,
                input.execution.attempt,
                PipelinePhase::Parsing,
                input.execution.priority,
                format!("parse:{}:{source_index}", generation.0),
            ),
            {
                let document_preparer = runtime.document_preparer.clone();
                move || {
                    prepare_documents(
                        source_batch,
                        generation,
                        enrichment_graph,
                        document_preparer,
                        runtime.document_prepare_concurrency,
                    )
                }
            },
        )
        .await?;
        let chunk_count = prepared
            .iter()
            .map(|document| document.chunks.len() as u64)
            .sum();
        let counts = progress.prepared(prepared.len() as u64, chunk_count, is_final_source_batch);
        coordinator
            .checkpoint(
                PipelinePhase::Preparing,
                counts,
                "prepared source documents",
            )
            .await;
        let batches = chunk_batches(prepared, runtime.embed_pool_max_inputs);
        if !input.plan.request.embed {
            for batch in batches {
                merge_vectorize_result(
                    &mut output,
                    statuses_only(batch, DocumentLifecycleStatus::Prepared),
                );
            }
            continue;
        }
        let batch_count = batches.len();
        let mut batches = batches.into_iter().enumerate();
        let Some((first_index, first_batch)) = batches.next() else {
            continue;
        };
        report_batching(input, &first_batch, emitter, coordinator, progress).await;
        let mut ready = embed_and_build_batch(
            runtime,
            input,
            first_batch,
            collection.clone(),
            emitter,
            coordinator,
            progress,
            is_final_vector_batch(is_final_source_batch, first_index, batch_count),
        )
        .await?;
        for (batch_index, batch) in batches {
            let final_vector_batch =
                is_final_vector_batch(is_final_source_batch, batch_index, batch_count);
            report_batching(input, &batch, emitter, coordinator, progress).await;
            // The current batch's write accounting is absorbed into `output`
            // inside `publish_and_build_next`, before an overlapped embedding
            // failure can surface.
            ready = publish_and_build_next(
                runtime,
                input,
                ready,
                batch,
                collection.clone(),
                emitter,
                coordinator,
                progress,
                &mut output,
                final_vector_batch,
            )
            .await?;
        }
        let result =
            publish_built_batch(runtime, input, ready, emitter, coordinator, progress).await?;
        merge_vectorize_result(&mut output, result);
    }
    write_document_statuses(runtime.ledger.as_ref(), &output.document_statuses).await?;
    Ok(output)
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn prepare_generation_documents(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    documents: Vec<SourceDocument>,
    enrichment_graph: &std::collections::BTreeMap<SourceItemKey, Vec<GraphCandidate>>,
    generation: &SourceGenerationId,
    emitter: &SourceEventEmitter,
    coordinator: &ProgressCoordinator,
    progress: &mut PipelineProgress,
    is_final_generation_batch: bool,
) -> anyhow::Result<Vec<PreparedDocument>> {
    coordinator
        .report(
            emitter,
            PipelinePhase::Preparing,
            progress.preparing_counts(),
            "preparing source documents",
        )
        .await;
    let prepared = reserved_call::parse_operation(
        runtime,
        ProviderCallContext::for_phase(
            input.plan.job_id,
            input.execution.attempt,
            PipelinePhase::Parsing,
            input.execution.priority,
            format!("parse:{}:scheduled", generation.0),
        ),
        {
            let document_preparer = runtime.document_preparer.clone();
            let generation = generation.clone();
            let enrichment_graph = enrichment_graph.clone();
            let concurrency = runtime.document_prepare_concurrency;
            move || async move {
                prepare_documents(
                    documents,
                    &generation,
                    &enrichment_graph,
                    document_preparer,
                    concurrency,
                )
                .await
            }
        },
    )
    .await?;
    let chunk_count = prepared
        .iter()
        .map(|document| document.chunks.len() as u64)
        .sum();
    let counts = progress.prepared(
        prepared.len() as u64,
        chunk_count,
        is_final_generation_batch,
    );
    coordinator
        .checkpoint(
            PipelinePhase::Preparing,
            counts,
            "prepared source documents",
        )
        .await;
    Ok(prepared)
}

fn is_final_vector_batch(
    is_final_source_batch: bool,
    batch_index: usize,
    batch_count: usize,
) -> bool {
    is_final_source_batch && batch_index + 1 == batch_count
}

async fn report_batching(
    input: &SourcePipelineInput<'_>,
    batch: &[PreparedDocument],
    emitter: &SourceEventEmitter,
    coordinator: &ProgressCoordinator,
    progress: &mut PipelineProgress,
) {
    if !input.plan.request.embed {
        return;
    }
    let chunks = batch
        .iter()
        .map(|document| document.chunks.len() as u64)
        .sum();
    coordinator
        .report(
            emitter,
            PipelinePhase::Batching,
            progress.batched(chunks),
            "batching prepared chunks",
        )
        .await;
}

pub(super) fn merge_vectorize_result(output: &mut VectorizeResult, result: VectorizeResult) {
    output.chunks_prepared = output
        .chunks_prepared
        .saturating_add(result.chunks_prepared);
    output.points_written = output.points_written.saturating_add(result.points_written);
    output.graph_candidates.extend(result.graph_candidates);
    output.warnings.extend(result.warnings);
    for status in result.document_statuses {
        if let Some(&position) = output.document_status_positions.get(&status.document_id) {
            let existing = &mut output.document_statuses[position];
            existing.chunk_count = existing.chunk_count.saturating_add(status.chunk_count);
            existing.vector_point_count = existing
                .vector_point_count
                .saturating_add(status.vector_point_count);
            existing.updated_at = status.updated_at;
        } else {
            output.documents_prepared = output.documents_prepared.saturating_add(1);
            let position = output.document_statuses.len();
            output
                .document_status_positions
                .insert(status.document_id.clone(), position);
            output.document_statuses.push(status);
        }
    }
}

fn vectorize_result(
    documents: Vec<PreparedDocument>,
    embedding_warnings: Vec<SourceWarning>,
    points_by_document: &std::collections::BTreeMap<DocumentId, u32>,
    write: VectorStoreWriteResult,
    skipped_redaction: u64,
    redaction_skips_by_source_item: &std::collections::BTreeMap<SourceItemKey, u64>,
) -> VectorizeResult {
    let mut result = statuses_only(documents, DocumentLifecycleStatus::Vectorized);
    result.points_written = write.points_written;
    result.warnings.extend(embedding_warnings);
    for (source_item_key, count) in redaction_skips_by_source_item {
        result.warnings.push(SourceWarning {
            code: "source.vectorize.redaction_skipped_chunks".to_string(),
            severity: Severity::Warning,
            message: format!(
                "skipped {count} chunk(s) with secret-redaction-forbidden payload values \
                 (not indexed; reduced vector point count accordingly)"
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
        status.vector_point_count = points_by_document
            .get(&status.document_id)
            .copied()
            .unwrap_or(0);
    }
    result
}

fn statuses_only(
    documents: Vec<PreparedDocument>,
    lifecycle: DocumentLifecycleStatus,
) -> VectorizeResult {
    let mut result = VectorizeResult::default();
    for document in documents {
        let chunk_count = document.chunks.len();
        result.documents_prepared += 1;
        result.chunks_prepared += chunk_count as u64;
        result.graph_candidates.extend(document.graph_candidates);
        result.warnings.extend(document.warnings);
        let status = DocumentStatus {
            document_id: document.document_id,
            source_id: document.source_id,
            source_item_key: document.source_item_key,
            generation: Some(document.generation),
            status: lifecycle,
            updated_at: timestamp(),
            chunk_count: u32::try_from(chunk_count).unwrap_or(u32::MAX),
            vector_point_count: 0,
            error: None,
            cleanup_status: None,
        };
        let position = result.document_statuses.len();
        result
            .document_status_positions
            .insert(status.document_id.clone(), position);
        result.document_statuses.push(status);
    }
    result
}

pub(super) async fn write_document_statuses(
    ledger: &dyn LedgerStore,
    statuses: &[DocumentStatus],
) -> anyhow::Result<()> {
    let status_batch_size = env_batch_size(
        "AXON_DOCUMENT_STATUS_BATCH_SIZE",
        DOCUMENT_STATUS_BATCH_SIZE,
        4096,
    );
    for batch in statuses.chunks(status_batch_size) {
        ledger.update_document_statuses(batch.to_vec()).await?;
    }
    Ok(())
}

fn embedding_batch(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    documents: &[PreparedDocument],
) -> anyhow::Result<EmbeddingBatch> {
    let mut batch_key = String::new();
    for chunk in documents.iter().flat_map(|document| document.chunks.iter()) {
        if !batch_key.is_empty() {
            batch_key.push(':');
        }
        batch_key.push_str(&chunk.chunk_id.0);
    }
    let batch_id = BatchId::new(Uuid::new_v5(&Uuid::NAMESPACE_URL, batch_key.as_bytes()));
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
                // Vector payload construction still owns the PreparedChunk metadata;
                // TEI consumes only chunk id/text. Avoid duplicating the payload map
                // across every embedding input in this 512-chunk hot path.
                metadata: MetadataMap::new(),
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
