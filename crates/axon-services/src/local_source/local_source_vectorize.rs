use axon_api::source::*;
use axon_document::{DocumentPreparer, PrepareSourceDocumentRequest};
use axon_embedding::batch::EmbeddingBatchBuilder;
use axon_embedding::provider::EmbeddingProvider;
use axon_embedding::reservation::{ProviderReservation, ProviderReservationContext};
use axon_ledger::store::LedgerStore;
use axon_vectors::point::{VectorPointBatchBuildContext, VectorPointBatchBuilder};
use axon_vectors::store::VectorStore;
use uuid::Uuid;

use super::local_source_adapter::{LocalAdapterRun, normalize_changed_documents, timestamp};
use super::local_source_progress::{
    LocalSourceProgress, progress_error_context, record_progress_with_reservations,
};
use super::{LocalSourceIndexInput, LocalSourceSelectionPolicy};

const LOCAL_CHANGED_DOCUMENT_BATCH_SIZE: usize = 64;
const LOCAL_CHANGED_CHUNK_BATCH_SIZE: usize = 512;

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct VectorizeStats {
    pub(super) documents_prepared: u64,
    pub(super) chunks_prepared: u64,
    pub(super) points_written: u64,
}

#[derive(Debug, Clone, Default)]
pub(super) struct VectorizeResult {
    pub(super) stats: VectorizeStats,
    pub(super) document_statuses: Vec<DocumentStatus>,
}

impl VectorizeStats {
    fn add(&mut self, other: VectorizeStats) {
        self.documents_prepared += other.documents_prepared;
        self.chunks_prepared += other.chunks_prepared;
        self.points_written += other.points_written;
    }
}

pub(super) async fn vectorize_changed_documents(
    input: &LocalSourceIndexInput,
    run: &LocalAdapterRun,
    diff: &SourceManifestDiff,
    generation: &SourceGenerationId,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
    progress: Option<&dyn LocalSourceProgress>,
    collection: CollectionSpec,
) -> anyhow::Result<VectorizeResult> {
    let mut result = VectorizeResult::default();
    for batch_diff in changed_diff_batches(diff, LOCAL_CHANGED_DOCUMENT_BATCH_SIZE) {
        let documents = prepare_changed_documents(run, &batch_diff, generation).await?;
        for prepared_batch in prepared_document_batches(documents, LOCAL_CHANGED_CHUNK_BATCH_SIZE) {
            let batch_result = vectorize_documents(
                input,
                ledger,
                embedding_provider,
                vector_store,
                progress,
                collection.clone(),
                prepared_batch,
            )
            .await?;
            result.stats.add(batch_result.stats);
            result
                .document_statuses
                .extend(batch_result.document_statuses);
        }
    }
    Ok(result)
}

async fn prepare_changed_documents(
    run: &LocalAdapterRun,
    diff: &SourceManifestDiff,
    generation: &SourceGenerationId,
) -> anyhow::Result<Vec<PreparedDocument>> {
    let source_documents = normalize_changed_documents(run, diff).await?;
    let mut documents = Vec::with_capacity(source_documents.len());
    let preparer = DocumentPreparer::default();
    for document in source_documents {
        let item_key = document.source_item_key.0.clone();
        let prepared = preparer
            .prepare(PrepareSourceDocumentRequest {
                document,
                generation: generation.clone(),
                profile: None,
                parse_facts: Vec::new(),
                graph_candidates: Vec::new(),
                warnings: Vec::new(),
                errors: Vec::new(),
            })
            .map_err(|err| anyhow::anyhow!("failed to prepare {item_key}: {err}"))?
            .document;
        documents.push(prepared);
    }
    Ok(documents)
}

fn changed_diff_batches(diff: &SourceManifestDiff, batch_size: usize) -> Vec<SourceManifestDiff> {
    let batch_size = batch_size.max(1);
    let mut batches = Vec::new();
    let mut current = empty_diff_like(diff);
    for item in &diff.added {
        current.added.push(item.clone());
        if changed_batch_len(&current) == batch_size {
            push_changed_batch(&mut batches, &mut current, diff);
        }
    }
    for item in &diff.modified {
        current.modified.push(item.clone());
        if changed_batch_len(&current) == batch_size {
            push_changed_batch(&mut batches, &mut current, diff);
        }
    }
    if changed_batch_len(&current) > 0 {
        push_changed_batch(&mut batches, &mut current, diff);
    }
    batches
}

fn changed_batch_len(batch: &SourceManifestDiff) -> usize {
    batch.added.len() + batch.modified.len()
}

fn push_changed_batch(
    batches: &mut Vec<SourceManifestDiff>,
    current: &mut SourceManifestDiff,
    diff: &SourceManifestDiff,
) {
    current.counts.added = current.added.len() as u64;
    current.counts.modified = current.modified.len() as u64;
    batches.push(std::mem::replace(current, empty_diff_like(diff)));
}

fn prepared_document_batches(
    documents: Vec<PreparedDocument>,
    max_chunks: usize,
) -> Vec<Vec<PreparedDocument>> {
    let max_chunks = max_chunks.max(1);
    let mut batches = Vec::new();
    let mut current = Vec::new();
    let mut current_chunks = 0_usize;
    for document in documents {
        let document_chunks = document.chunks.len().max(1);
        if !current.is_empty() && current_chunks + document_chunks > max_chunks {
            batches.push(std::mem::take(&mut current));
            current_chunks = 0;
        }
        current_chunks += document_chunks;
        current.push(document);
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

fn empty_diff_like(diff: &SourceManifestDiff) -> SourceManifestDiff {
    SourceManifestDiff {
        header: diff.header.clone(),
        source_id: diff.source_id.clone(),
        previous_generation: diff.previous_generation.clone(),
        next_generation: diff.next_generation.clone(),
        added: Vec::new(),
        modified: Vec::new(),
        removed: Vec::new(),
        unchanged: Vec::new(),
        skipped: Vec::new(),
        failed: Vec::new(),
        counts: DiffCounts {
            added: 0,
            modified: 0,
            removed: 0,
            unchanged: 0,
            skipped: 0,
            failed: 0,
        },
    }
}

async fn vectorize_documents(
    input: &LocalSourceIndexInput,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
    progress: Option<&dyn LocalSourceProgress>,
    collection: CollectionSpec,
    documents: Vec<PreparedDocument>,
) -> anyhow::Result<VectorizeResult> {
    let mut result = VectorizeResult::default();
    if documents.is_empty() {
        return Ok(result);
    }
    // `SourceRequest.embed = false` (source-pipeline.md Validation Checklist:
    // "`embed=false` never writes vectors"): documents are still discovered,
    // normalized, and prepared above this call, but neither the embedding
    // provider nor the vector store may be invoked. Counts still reflect
    // prepared documents/chunks; only `points_written` stays zero.
    if !input.embed {
        for document in documents {
            result.stats.chunks_prepared += document.chunks.len() as u64;
            result.stats.documents_prepared += 1;
            let status = document_status(&document, 0, DocumentLifecycleStatus::Prepared);
            ledger.update_document_status(status.clone()).await?;
            result.document_statuses.push(status);
        }
        return Ok(result);
    }
    let batch = embedding_batch_for_documents(input, &documents)?;
    let embedding_reservation = reserve_embedding(input).await?;
    record_progress_with_reservations(
        progress,
        PipelinePhase::Embedding,
        None,
        reservation_snapshots([embedding_reservation.as_ref()]),
    )
    .await?;
    let embeddings = match embedding_provider.embed(batch).await {
        Ok(embeddings) => embeddings,
        Err(err) => {
            let progress_context =
                progress_error_context(progress, PipelinePhase::Embedding, &err).await;
            let mut err = anyhow::Error::new(err);
            if let Some(context) = progress_context {
                err = err.context(context);
            }
            return Err(err);
        }
    };
    drop(embedding_reservation);
    let point_batch = vector_point_batch_for_documents(input, collection, &documents, &embeddings)?;
    let vector_reservation = reserve_vector(input).await?;
    record_progress_with_reservations(
        progress,
        PipelinePhase::Vectorizing,
        None,
        reservation_snapshots([vector_reservation.as_ref()]),
    )
    .await?;
    let write = match vector_store.upsert(point_batch).await {
        Ok(write) => write,
        Err(err) => {
            let progress_context =
                progress_error_context(progress, PipelinePhase::Vectorizing, &err).await;
            let mut err = anyhow::Error::new(err);
            if let Some(context) = progress_context {
                err = err.context(context);
            }
            return Err(err);
        }
    };
    drop(vector_reservation);
    result.stats.points_written += write.points_written;
    for document in documents {
        result.stats.chunks_prepared += document.chunks.len() as u64;
        result.stats.documents_prepared += 1;
        let status = document_status(
            &document,
            document.chunks.len() as u64,
            DocumentLifecycleStatus::Vectorized,
        );
        ledger.update_document_status(status.clone()).await?;
        result.document_statuses.push(status);
    }
    Ok(result)
}

async fn reserve_embedding(
    input: &LocalSourceIndexInput,
) -> anyhow::Result<Option<ProviderReservation>> {
    let Some(manager) = &input.embedding_reservations else {
        return Ok(None);
    };
    Ok(Some(
        manager
            .reserve_with_context(ProviderReservationContext {
                job_id: input.job_id,
                stage_id: None,
                provider_id: Some(input.embedding_provider_id.clone()),
                priority: JobPriority::Background,
                units: 1,
                ttl_seconds: Some(300),
            })
            .await?,
    ))
}

async fn reserve_vector(
    input: &LocalSourceIndexInput,
) -> anyhow::Result<Option<ProviderReservation>> {
    let Some(manager) = &input.vector_reservations else {
        return Ok(None);
    };
    Ok(Some(
        manager
            .reserve_with_context(ProviderReservationContext {
                job_id: input.job_id,
                stage_id: None,
                provider_id: Some(input.vector_provider_id.clone()),
                priority: JobPriority::Background,
                units: 1,
                ttl_seconds: Some(300),
            })
            .await?,
    ))
}

fn reservation_snapshots<'a>(
    reservations: impl IntoIterator<Item = Option<&'a ProviderReservation>>,
) -> Vec<ProviderReservationSnapshot> {
    reservations
        .into_iter()
        .flatten()
        .map(ProviderReservation::snapshot)
        .collect()
}

fn embedding_batch_for_documents(
    input: &LocalSourceIndexInput,
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

fn vector_point_batch_for_documents(
    input: &LocalSourceIndexInput,
    collection: CollectionSpec,
    documents: &[PreparedDocument],
    embeddings: &EmbeddingResult,
) -> anyhow::Result<VectorPointBatch> {
    let mut points = Vec::new();
    let vectors_by_chunk = embeddings
        .vectors
        .iter()
        .cloned()
        .map(|vector| (vector.chunk_id.clone(), vector))
        .collect::<std::collections::BTreeMap<_, _>>();
    for document in documents {
        let mut document = document.clone();
        if input.selection_policy == LocalSourceSelectionPolicy::CodeSearch {
            document
                .metadata
                .insert("visibility".to_string(), serde_json::json!("public"));
            for chunk in &mut document.chunks {
                chunk
                    .metadata
                    .insert("visibility".to_string(), serde_json::json!("public"));
            }
        }
        let document_embeddings =
            embedding_result_for_document(embeddings, &document, &vectors_by_chunk);
        let batch = VectorPointBatchBuilder::new(
            collection.clone(),
            document,
            document_embeddings,
            VectorPointBatchBuildContext {
                embedded_at: timestamp(),
            },
        )
        .build()?;
        points.extend(batch.points);
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

fn embedding_result_for_document(
    embeddings: &EmbeddingResult,
    document: &PreparedDocument,
    vectors_by_chunk: &std::collections::BTreeMap<ChunkId, EmbeddingVector>,
) -> EmbeddingResult {
    let vectors = document
        .chunks
        .iter()
        .filter_map(|chunk| vectors_by_chunk.get(&chunk.chunk_id).cloned())
        .collect();
    EmbeddingResult {
        batch_id: embeddings.batch_id.clone(),
        job_id: embeddings.job_id,
        provider_id: embeddings.provider_id.clone(),
        model: embeddings.model.clone(),
        dimensions: embeddings.dimensions,
        vectors,
        usage: embeddings.usage.clone(),
        warnings: embeddings.warnings.clone(),
    }
}

pub(super) fn publish_document_status(status: &DocumentStatus) -> DocumentStatus {
    DocumentStatus {
        status: DocumentLifecycleStatus::Published,
        updated_at: timestamp(),
        ..status.clone()
    }
}

fn document_status(
    document: &PreparedDocument,
    points_written: u64,
    status: DocumentLifecycleStatus,
) -> DocumentStatus {
    DocumentStatus {
        document_id: document.document_id.clone(),
        source_id: document.source_id.clone(),
        source_item_key: document.source_item_key.clone(),
        generation: Some(document.generation.clone()),
        status,
        updated_at: timestamp(),
        chunk_count: document.chunks.len() as u32,
        vector_point_count: u32::try_from(points_written).unwrap_or(u32::MAX),
        error: None,
        cleanup_status: None,
    }
}
