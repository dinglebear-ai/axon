use axon_adapters::{SourceAdapter, web::WebSourceAdapter};
use axon_api::source::*;
use axon_document::{DocumentPreparer, PrepareSourceDocumentRequest};
use axon_embedding::batch::EmbeddingBatchBuilder;
use axon_embedding::provider::EmbeddingProvider;
use axon_ledger::store::LedgerStore;
use axon_vectors::point::{VectorPointBatchBuildContext, VectorPointBatchBuilder};
use axon_vectors::store::VectorStore;
use uuid::Uuid;

use super::{WebAdapterRun, WebSourceIndexInput, timestamp};

const WEB_CHANGED_DOCUMENT_BATCH_SIZE: usize = 64;
const WEB_CHANGED_CHUNK_BATCH_SIZE: usize = 512;

#[derive(Debug, Clone, Default)]
pub(super) struct VectorizeResult {
    pub(super) documents_prepared: u64,
    pub(super) chunks_prepared: u64,
    pub(super) document_statuses: Vec<DocumentStatus>,
}

#[derive(Debug, Clone, Copy, Default)]
struct VectorizeStats {
    documents_prepared: u64,
    chunks_prepared: u64,
}

#[derive(Debug, Clone, Default)]
struct VectorizeResultWithStats {
    stats: VectorizeStats,
    document_statuses: Vec<DocumentStatus>,
}

pub(super) async fn vectorize_changed_documents(
    input: &WebSourceIndexInput,
    run: &WebAdapterRun,
    diff: &SourceManifestDiff,
    generation: &SourceGenerationId,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
    collection: CollectionSpec,
) -> anyhow::Result<VectorizeResult> {
    let mut result = VectorizeResult::default();
    for batch_diff in changed_diff_batches(diff, WEB_CHANGED_DOCUMENT_BATCH_SIZE) {
        let source_documents = normalize_changed_documents(run, &batch_diff).await?;
        let prepared = prepare_source_documents(source_documents, generation)?;
        for prepared_batch in prepared_document_batches(prepared, WEB_CHANGED_CHUNK_BATCH_SIZE) {
            let batch_result = vectorize_documents(
                input,
                ledger,
                embedding_provider,
                vector_store,
                collection.clone(),
                prepared_batch,
            )
            .await?;
            result.documents_prepared += batch_result.stats.documents_prepared;
            result.chunks_prepared += batch_result.stats.chunks_prepared;
            result
                .document_statuses
                .extend(batch_result.document_statuses);
        }
    }
    Ok(result)
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
        sparse: None,
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

async fn normalize_changed_documents(
    run: &WebAdapterRun,
    diff: &SourceManifestDiff,
) -> anyhow::Result<Vec<SourceDocument>> {
    let adapter = WebSourceAdapter::new();
    let acquisition = adapter.acquire(&run.plan, diff).await?;
    Ok(adapter.normalize(&run.plan, acquisition).await?.data)
}

fn prepare_source_documents(
    source_documents: Vec<SourceDocument>,
    generation: &SourceGenerationId,
) -> anyhow::Result<Vec<PreparedDocument>> {
    let preparer = DocumentPreparer::default();
    let mut documents = Vec::with_capacity(source_documents.len());
    for document in source_documents {
        let item_key = document.source_item_key.0.clone();
        let mut prepared = preparer
            .prepare(PrepareSourceDocumentRequest {
                document,
                generation: generation.clone(),
                profile: None,
                parse_facts: Vec::new(),
                graph_candidates: Vec::new(),
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

fn sanitize_web_payload_metadata(document: &mut PreparedDocument) {
    sanitize_metadata(&mut document.metadata);
    for chunk in &mut document.chunks {
        sanitize_metadata(&mut chunk.metadata);
        if let Some(title) = chunk.title.as_deref().or(document
            .metadata
            .get("web_title")
            .and_then(|value| value.as_str()))
        {
            chunk
                .metadata
                .insert("web_title".to_string(), serde_json::json!(title));
        }
    }
}

fn sanitize_metadata(metadata: &mut MetadataMap) {
    for field in [
        "normalization_version",
        "web_url",
        "web_seed_url",
        "web_origin",
        "web_path",
        "web_normalized_url",
        "web_fetch_method",
        "structured_payload_omitted",
    ] {
        metadata.remove(field);
    }
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

async fn vectorize_documents(
    input: &WebSourceIndexInput,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
    collection: CollectionSpec,
    documents: Vec<PreparedDocument>,
) -> anyhow::Result<VectorizeResultWithStats> {
    if documents.is_empty() {
        return Ok(VectorizeResultWithStats::default());
    }
    let batch = embedding_batch_for_documents(input, &documents)?;
    let embeddings = embedding_provider.embed(batch).await?;
    let point_batch = vector_point_batch_for_documents(collection, &documents, &embeddings)?;
    let expected_points = point_batch.points.len() as u64;
    let write = vector_store.upsert(point_batch).await?;
    if write.points_attempted != write.points_written || write.points_written != expected_points {
        return Err(anyhow::anyhow!(
            "upsert wrote {} of {} attempted points; expected {expected_points}",
            write.points_written,
            write.points_attempted
        ));
    }
    let mut result = VectorizeResultWithStats::default();
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

fn vector_point_batch_for_documents(
    collection: CollectionSpec,
    documents: &[PreparedDocument],
    embeddings: &EmbeddingResult,
) -> anyhow::Result<VectorPointBatch> {
    let vectors_by_chunk = embeddings
        .vectors
        .iter()
        .cloned()
        .map(|vector| (vector.chunk_id.clone(), vector))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut points = Vec::new();
    for document in documents {
        let document_embeddings =
            embedding_result_for_document(embeddings, document, &vectors_by_chunk)?;
        let batch = VectorPointBatchBuilder::new(
            collection.clone(),
            document.clone(),
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
) -> anyhow::Result<EmbeddingResult> {
    let mut vectors = Vec::with_capacity(document.chunks.len());
    for chunk in &document.chunks {
        let vector = vectors_by_chunk
            .get(&chunk.chunk_id)
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "embedding result missing vector for web chunk {}",
                    chunk.chunk_id.0
                )
            })?;
        vectors.push(vector);
    }
    Ok(EmbeddingResult {
        batch_id: embeddings.batch_id.clone(),
        job_id: embeddings.job_id,
        provider_id: embeddings.provider_id.clone(),
        model: embeddings.model.clone(),
        dimensions: embeddings.dimensions,
        vectors,
        usage: embeddings.usage.clone(),
        warnings: embeddings.warnings.clone(),
    })
}

fn payload_index(field_name: &str) -> PayloadIndexSpec {
    PayloadIndexSpec {
        field_name: field_name.to_string(),
        field_schema: PayloadFieldSchema::Keyword,
        required_for_filters: true,
    }
}

fn document_status(
    document: &PreparedDocument,
    vector_point_count: u64,
    status: DocumentLifecycleStatus,
) -> DocumentStatus {
    DocumentStatus {
        document_id: document.document_id.clone(),
        source_id: document.source_id.clone(),
        source_item_key: document.source_item_key.clone(),
        generation: document.generation.clone(),
        status,
        updated_at: timestamp(),
        chunk_count: document.chunks.len() as u32,
        vector_point_count: vector_point_count.min(u32::MAX as u64) as u32,
        error: None,
        cleanup_status: None,
    }
}
