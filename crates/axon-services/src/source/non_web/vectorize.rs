use axon_api::source::*;
use axon_document::{DocumentPreparer, PrepareSourceDocumentRequest};
use axon_embedding::batch::EmbeddingBatchBuilder;
use axon_embedding::reservation::{ProviderReservation, ProviderReservationContext};
use axon_ledger::store::LedgerStore;
use axon_vectors::point::{VectorPointBatchBuildContext, VectorPointBatchBuilder};
use futures_util::future::try_join_all;
use uuid::Uuid;

use super::{NonWebPipelineInput, SourceEventEmitter, TargetLocalSourceRuntime, timestamp};

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

pub(super) async fn prepare_embed_publish(
    runtime: &TargetLocalSourceRuntime,
    input: &NonWebPipelineInput<'_>,
    documents: Vec<SourceDocument>,
    enrichment_graph: &std::collections::BTreeMap<SourceItemKey, Vec<GraphCandidate>>,
    sanitize_session_chunks: bool,
    generation: &SourceGenerationId,
    collection: CollectionSpec,
    emitter: &SourceEventEmitter,
) -> anyhow::Result<VectorizeResult> {
    let mut output = VectorizeResult::default();
    for source_batch in documents.chunks(DOCUMENT_BATCH_SIZE) {
        let prepared = prepare_documents(
            source_batch,
            generation,
            enrichment_graph,
            sanitize_session_chunks,
        )?;
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
    sanitize_session_chunks: bool,
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
            let mut prepared = preparer
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
            if sanitize_session_chunks {
                sanitize_session_chunk_metadata(&mut prepared);
            }
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

fn merge_vectorize_result(output: &mut VectorizeResult, result: VectorizeResult) {
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
    input: &NonWebPipelineInput<'_>,
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
    let embedding_reservation = reserve_embedding(runtime, input).await?;
    record_reservation(
        runtime,
        input,
        PipelinePhase::Embedding,
        &embedding_reservation,
    )
    .await?;
    let embedding_batch = embedding_batch(runtime, input, &documents)?;
    let embeddings = runtime.embedding_provider.embed(embedding_batch).await?;
    drop(embedding_reservation);

    super::record_running_phase(
        runtime,
        input,
        emitter,
        PipelinePhase::Upserting,
        "upserting vector point batch",
    )
    .await?;
    let vector_reservation = reserve_vector(runtime, input).await?;
    record_reservation(
        runtime,
        input,
        PipelinePhase::Upserting,
        &vector_reservation,
    )
    .await?;
    let point_batch = point_batch(collection, &documents, &embeddings)?;
    let write = runtime.vector_store.upsert(point_batch).await?;
    drop(vector_reservation);

    let mut result = statuses_only(documents, DocumentLifecycleStatus::Vectorized);
    result.points_written = write.points_written;
    result.warnings.extend(embeddings.warnings);
    for status in &mut result.document_statuses {
        status.vector_point_count = status.chunk_count;
    }
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
        try_join_all(
            batch
                .iter()
                .cloned()
                .map(|status| ledger.update_document_status(status)),
        )
        .await?;
    }
    Ok(())
}

fn embedding_batch(
    runtime: &TargetLocalSourceRuntime,
    input: &NonWebPipelineInput<'_>,
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
) -> anyhow::Result<VectorPointBatch> {
    let by_chunk = embeddings
        .vectors
        .iter()
        .cloned()
        .map(|vector| (vector.chunk_id.clone(), vector))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut points = Vec::new();
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
        points.extend(
            VectorPointBatchBuilder::new(
                collection.clone(),
                document.clone(),
                document_embeddings,
                VectorPointBatchBuildContext {
                    embedded_at: timestamp(),
                },
            )
            .build()?
            .points,
        );
    }
    Ok(VectorPointBatch {
        batch_id: embeddings.batch_id.clone(),
        collection: collection.collection,
        points,
        model: embeddings.model.clone(),
        dimensions: embeddings.dimensions,
        sparse_vectors: None,
        payload_indexes: collection.payload_indexes,
    })
}

async fn reserve_embedding(
    runtime: &TargetLocalSourceRuntime,
    input: &NonWebPipelineInput<'_>,
) -> anyhow::Result<ProviderReservation> {
    Ok(runtime
        .embedding_reservations
        .reserve_with_context(ProviderReservationContext {
            job_id: input.plan.job_id,
            stage_id: None,
            provider_id: Some(runtime.embedding_provider_id.clone()),
            priority: input.execution.priority,
            units: 1,
            ttl_seconds: Some(300),
        })
        .await?)
}

async fn reserve_vector(
    runtime: &TargetLocalSourceRuntime,
    input: &NonWebPipelineInput<'_>,
) -> anyhow::Result<ProviderReservation> {
    Ok(runtime
        .vector_reservations
        .reserve_with_context(ProviderReservationContext {
            job_id: input.plan.job_id,
            stage_id: None,
            provider_id: Some(runtime.vector_provider_id.clone()),
            priority: input.execution.priority,
            units: 1,
            ttl_seconds: Some(300),
        })
        .await?)
}

async fn record_reservation(
    runtime: &TargetLocalSourceRuntime,
    input: &NonWebPipelineInput<'_>,
    phase: PipelinePhase,
    reservation: &ProviderReservation,
) -> anyhow::Result<()> {
    runtime
        .jobs
        .heartbeat(JobHeartbeat {
            job_id: input.plan.job_id,
            attempt: input.execution.attempt,
            worker_id: Some("source-pipeline".to_string()),
            phase,
            status: LifecycleStatus::Running,
            stage_id: None,
            heartbeat_at: timestamp(),
            sequence: 0,
            last_progress_at: Some(timestamp()),
            last_event_sequence: None,
            counts: None,
            provider_reservations: vec![reservation.snapshot()],
        })
        .await?;
    Ok(())
}

fn sanitize_session_chunk_metadata(document: &mut PreparedDocument) {
    const ALLOWED: &[&str] = &[
        "session_provider",
        "session_id",
        "session_turn_index",
        "session_tool_name",
        "session_skill_name",
    ];
    for chunk in &mut document.chunks {
        chunk.metadata.retain(|key, _| {
            axon_vectors::payload::VECTOR_SHARED_FIELDS.contains(&key.as_str())
                || ALLOWED.contains(&key.as_str())
        });
    }
}

#[cfg(test)]
#[path = "vectorize_tests.rs"]
mod tests;
