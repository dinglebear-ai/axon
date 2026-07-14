use axon_adapters::{SourceAdapter, web};
use axon_api::source::*;
use axon_document::{DocumentPreparer, PrepareSourceDocumentRequest};
use axon_embedding::batch::EmbeddingBatchBuilder;
use axon_embedding::provider::EmbeddingProvider;
use axon_ledger::store::LedgerStore;
use axon_vectors::store::VectorStore;
use uuid::Uuid;

use super::WebSourceIndexInput;
use super::artifacts::{
    WebArtifactIndex, cleanup_artifacts_after_error, store_clean_outputs, store_warc_artifact,
};
use super::reuse;
use super::run::{WebAdapterRun, timestamp};
use super::vectorize_helpers::{
    changed_diff_batches, document_status, payload_index, prepared_document_batches,
    sanitize_web_payload_metadata, take_vertical_parse_artifacts, vector_point_batch_for_documents,
};

const WEB_CHANGED_DOCUMENT_BATCH_SIZE: usize = 64;
const WEB_CHANGED_CHUNK_BATCH_SIZE: usize = 512;

#[derive(Debug, Clone, Default)]
pub(super) struct VectorizeResult {
    pub(super) documents_prepared: u64,
    pub(super) chunks_prepared: u64,
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

#[derive(Debug, Clone, Copy, Default)]
struct VectorizeStats {
    documents_prepared: u64,
    chunks_prepared: u64,
}

#[derive(Debug, Clone, Default)]
struct VectorizeResultWithStats {
    stats: VectorizeStats,
    document_statuses: Vec<DocumentStatus>,
    graph_candidates: Vec<GraphCandidate>,
    warnings: Vec<SourceWarning>,
}

pub(super) struct NormalizedWebDocuments {
    pub(super) documents: Vec<SourceDocument>,
    pub(super) warnings: Vec<SourceWarning>,
    pub(super) reused_item_keys: Vec<SourceItemKey>,
    pub(super) artifacts: Vec<ArtifactRef>,
    pub(super) inline: Option<InlineSourceResult>,
    pub(super) artifact_index: WebArtifactIndex,
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
        let normalized = normalize_changed_documents(input, run, &batch_diff).await?;
        result.warnings.extend(normalized.warnings);
        result.reused_item_keys.extend(normalized.reused_item_keys);
        result.artifacts.extend(normalized.artifacts);
        result.artifact_index.merge(normalized.artifact_index);
        if result.inline.is_none() {
            result.inline = normalized.inline;
        }
        let prepared = match prepare_source_documents(normalized.documents, generation) {
            Ok(prepared) => prepared,
            Err(err) => {
                return Err(cleanup_artifacts_after_error(
                    input.artifact_store.as_ref(),
                    &result.artifacts,
                    err,
                )
                .await);
            }
        };
        for prepared_batch in prepared_document_batches(prepared, WEB_CHANGED_CHUNK_BATCH_SIZE) {
            let batch_result = match vectorize_documents(
                input,
                ledger,
                embedding_provider,
                vector_store,
                collection.clone(),
                prepared_batch,
            )
            .await
            {
                Ok(batch_result) => batch_result,
                Err(err) => {
                    return Err(cleanup_artifacts_after_error(
                        input.artifact_store.as_ref(),
                        &result.artifacts,
                        err,
                    )
                    .await);
                }
            };
            result.documents_prepared += batch_result.stats.documents_prepared;
            result.chunks_prepared += batch_result.stats.chunks_prepared;
            result
                .document_statuses
                .extend(batch_result.document_statuses);
            result
                .graph_candidates
                .extend(batch_result.graph_candidates);
            result.warnings.extend(batch_result.warnings);
        }
    }
    Ok(result)
}

pub(super) async fn prepare_changed_documents_without_vectors(
    input: &WebSourceIndexInput,
    run: &WebAdapterRun,
    diff: &SourceManifestDiff,
    generation: &SourceGenerationId,
    ledger: &dyn LedgerStore,
) -> anyhow::Result<VectorizeResult> {
    let mut result = VectorizeResult::default();
    for batch_diff in changed_diff_batches(diff, WEB_CHANGED_DOCUMENT_BATCH_SIZE) {
        let normalized = normalize_changed_documents(input, run, &batch_diff).await?;
        result.warnings.extend(normalized.warnings);
        result.reused_item_keys.extend(normalized.reused_item_keys);
        result.artifacts.extend(normalized.artifacts);
        result.artifact_index.merge(normalized.artifact_index);
        if result.inline.is_none() {
            result.inline = normalized.inline;
        }
        let prepared = match prepare_source_documents(normalized.documents, generation) {
            Ok(prepared) => prepared,
            Err(err) => {
                return Err(cleanup_artifacts_after_error(
                    input.artifact_store.as_ref(),
                    &result.artifacts,
                    err,
                )
                .await);
            }
        };
        for document in prepared {
            result.documents_prepared += 1;
            result.chunks_prepared += document.chunks.len() as u64;
            result
                .graph_candidates
                .extend(document.graph_candidates.clone());
            let status =
                document_status(&document, 0, DocumentLifecycleStatus::Prepared, timestamp());
            if let Err(err) = ledger.update_document_status(status.clone()).await {
                return Err(cleanup_artifacts_after_error(
                    input.artifact_store.as_ref(),
                    &result.artifacts,
                    anyhow::Error::new(err),
                )
                .await);
            }
            result.document_statuses.push(status);
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

pub(super) async fn normalize_changed_documents(
    input: &WebSourceIndexInput,
    run: &WebAdapterRun,
    diff: &SourceManifestDiff,
) -> anyhow::Result<NormalizedWebDocuments> {
    let adapter = web::WebSourceAdapter::new(
        std::sync::Arc::clone(&input.fetch_provider),
        std::sync::Arc::clone(&input.render_provider),
    );
    let mut acquisition = adapter.acquire(&run.plan, diff).await?;
    let mut warnings = acquisition.header.warnings.clone();
    let mut artifact_index = WebArtifactIndex::default();
    let mut artifacts = Vec::new();
    for artifact in store_warc_artifact(input, run, &acquisition.fetched_items).await? {
        artifact_index.push_generation(artifact.clone());
        artifacts.push(artifact);
    }
    let mut documents = Vec::new();
    let mut documents_to_cache = Vec::new();
    let mut fetched_items = Vec::new();
    let mut reused_item_keys = Vec::new();

    for item in std::mem::take(&mut acquisition.fetched_items) {
        let reuse_required = item
            .metadata
            .get("web_reuse_required")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if !reuse_required {
            fetched_items.push(item);
            continue;
        }

        if let Some(reused) = reuse::load_reused_web_document(
            input.document_cache.as_ref(),
            &run.source_id,
            diff.previous_generation.as_ref(),
            &item.manifest_item.source_item_key,
            &diff.next_generation,
        )
        .await?
        {
            reused_item_keys.push(item.manifest_item.source_item_key.clone());
            documents_to_cache.push(reused.document);
            continue;
        }

        warnings.push(SourceWarning {
            code: "web.reuse.cache_miss_refetch".to_string(),
            severity: Severity::Warning,
            message: format!(
                "conditional 304 for {} had no cached committed document; refetching before publish",
                item.manifest_item.canonical_uri
            ),
            source_item_key: Some(item.manifest_item.source_item_key.clone()),
            retryable: true,
        });
        fetched_items
            .push(refetch_without_conditional(input, run, diff, item.manifest_item).await?);
    }

    if !fetched_items.is_empty() {
        acquisition.fetched_items = fetched_items;
        let normalized = adapter.normalize(&run.plan, acquisition).await?.data;
        let clean_output = store_clean_outputs(input, &normalized).await?;
        artifacts.extend(clean_output.artifacts);
        artifact_index.merge(clean_output.artifact_index);
        let inline = clean_output.inline;
        documents_to_cache.extend(normalized.clone());
        documents.extend(normalized);
        reuse::cache_documents(
            input.document_cache.as_ref(),
            &run.source_id,
            &diff.next_generation,
            &documents_to_cache,
        )
        .await?;
        return Ok(NormalizedWebDocuments {
            documents,
            warnings,
            reused_item_keys,
            artifacts,
            inline,
            artifact_index,
        });
    }

    reuse::cache_documents(
        input.document_cache.as_ref(),
        &run.source_id,
        &diff.next_generation,
        &documents_to_cache,
    )
    .await?;
    Ok(NormalizedWebDocuments {
        documents,
        warnings,
        reused_item_keys,
        artifacts,
        inline: None,
        artifact_index,
    })
}

async fn refetch_without_conditional(
    input: &WebSourceIndexInput,
    run: &WebAdapterRun,
    diff: &SourceManifestDiff,
    manifest_item: ManifestItem,
) -> anyhow::Result<AcquiredSourceItem> {
    let mut plan = run.plan.clone();
    plan.route
        .validated_options
        .values
        .insert("etag_conditional".to_string(), serde_json::json!(false));
    let adapter = web::WebSourceAdapter::new(
        std::sync::Arc::clone(&input.fetch_provider),
        std::sync::Arc::clone(&input.render_provider),
    );
    let reacquired = adapter
        .acquire(
            &plan,
            &SourceManifestDiff {
                header: diff.header.clone(),
                source_id: diff.source_id.clone(),
                previous_generation: diff.previous_generation.clone(),
                next_generation: diff.next_generation.clone(),
                added: Vec::new(),
                modified: vec![manifest_item.clone()],
                removed: Vec::new(),
                unchanged: Vec::new(),
                skipped: Vec::new(),
                failed: Vec::new(),
                counts: DiffCounts {
                    added: 0,
                    modified: 1,
                    removed: 0,
                    unchanged: 0,
                    skipped: 0,
                    failed: 0,
                },
            },
        )
        .await?;
    let mut reacquired_items = reacquired.fetched_items.into_iter();
    let reacquired = match reacquired_items.next() {
        Some(item) => item,
        None => {
            if let Some(warning) = reacquired.header.warnings.iter().find(|warning| {
                warning.code == "web.fetch.invalid_304_without_validator"
                    || warning.message.contains("304 Not Modified")
            }) {
                anyhow::bail!(
                    "unconditional refetch for {} received another 304/reuse response: {}",
                    manifest_item.canonical_uri,
                    warning.message
                );
            }
            anyhow::bail!(
                "unconditional refetch for {} returned no document",
                manifest_item.canonical_uri
            );
        }
    };
    let reuse_required = reacquired
        .metadata
        .get("web_reuse_required")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if reuse_required
        || matches!(
            &reacquired.content_ref,
            ContentRef::External { uri, .. } if uri.starts_with("reuse://")
        )
    {
        anyhow::bail!(
            "unconditional refetch for {} returned 304/reuse instead of content",
            manifest_item.canonical_uri
        );
    }
    Ok(reacquired)
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
        result
            .graph_candidates
            .extend(document.graph_candidates.clone());
        result.warnings.extend(document.warnings.clone());
        let status = document_status(
            &document,
            document.chunks.len() as u64,
            DocumentLifecycleStatus::Vectorized,
            timestamp(),
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

#[cfg(test)]
#[path = "vectorize_tests.rs"]
mod tests;
