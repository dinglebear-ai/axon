use super::*;

pub(crate) async fn vectorize_changed_documents(
    input: &WebSourceIndexInput,
    run: &WebAdapterRun,
    diff: &SourceManifestDiff,
    generation: &SourceGenerationId,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
    collection: CollectionSpec,
    events: &SourceEventEmitter,
    coordinator: &WebProgressCoordinator,
    progress: &mut WebPipelineProgress,
) -> anyhow::Result<VectorizeResult> {
    let mut result = VectorizeResult::default();
    let batches = changed_diff_batches(diff, WEB_CHANGED_DOCUMENT_BATCH_SIZE);
    let batch_count = batches.len();
    for (index, batch_diff) in batches.into_iter().enumerate() {
        let batch_result = process_changed_batch(
            input,
            run,
            &batch_diff,
            generation,
            ledger,
            embedding_provider,
            vector_store,
            collection.clone(),
            events,
            coordinator,
            progress,
            index + 1 == batch_count,
        )
        .await?;
        merge_vectorize_result(&mut result, batch_result);
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
async fn process_changed_batch(
    input: &WebSourceIndexInput,
    run: &WebAdapterRun,
    diff: &SourceManifestDiff,
    generation: &SourceGenerationId,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
    collection: CollectionSpec,
    events: &SourceEventEmitter,
    coordinator: &WebProgressCoordinator,
    progress: &mut WebPipelineProgress,
    final_changed_batch: bool,
) -> anyhow::Result<VectorizeResult> {
    let normalized = normalize_changed_documents(
        input,
        run,
        diff,
        events,
        coordinator,
        progress,
        final_changed_batch,
    )
    .await?;
    let (documents, mut result) = split_normalized(normalized);
    coordinator
        .report(
            events,
            PipelinePhase::Preparing,
            progress.preparing_counts(),
            "preparing web source documents",
        )
        .await;
    let prepared = prepare_or_cleanup(input, generation, &result.artifacts, documents).await?;
    let document_count = prepared.len() as u64;
    let chunk_count = prepared
        .iter()
        .map(|document| document.chunks.len() as u64)
        .sum();
    coordinator
        .checkpoint(
            events,
            PipelinePhase::Preparing,
            progress.prepared(document_count, chunk_count, final_changed_batch),
            "prepared web source documents",
        )
        .await;
    let vectorized = vectorize_prepared_batches(
        input,
        ledger,
        embedding_provider,
        vector_store,
        collection,
        prepared,
        events,
        coordinator,
        progress,
        final_changed_batch,
        &result.artifacts,
    )
    .await?;
    merge_vectorize_result(&mut result, vectorized);
    Ok(result)
}

fn split_normalized(normalized: NormalizedWebDocuments) -> (Vec<SourceDocument>, VectorizeResult) {
    let documents = normalized.documents;
    let result = VectorizeResult {
        reused_item_keys: normalized.reused_item_keys,
        warnings: normalized.warnings,
        artifacts: normalized.artifacts,
        inline: normalized.inline,
        artifact_index: normalized.artifact_index,
        ..VectorizeResult::default()
    };
    (documents, result)
}

async fn prepare_or_cleanup(
    input: &WebSourceIndexInput,
    generation: &SourceGenerationId,
    artifacts: &[ArtifactRef],
    documents: Vec<SourceDocument>,
) -> anyhow::Result<Vec<PreparedDocument>> {
    match prepare_source_documents(documents, generation) {
        Ok(prepared) => Ok(prepared),
        Err(error) => {
            Err(
                cleanup_artifacts_after_error(input.artifact_store.as_ref(), artifacts, error)
                    .await,
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn vectorize_prepared_batches(
    input: &WebSourceIndexInput,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
    collection: CollectionSpec,
    prepared: Vec<PreparedDocument>,
    events: &SourceEventEmitter,
    coordinator: &WebProgressCoordinator,
    progress: &mut WebPipelineProgress,
    final_changed_batch: bool,
    artifacts: &[ArtifactRef],
) -> anyhow::Result<VectorizeResult> {
    let mut result = VectorizeResult::default();
    let batches = prepared_document_batches(prepared, WEB_CHANGED_CHUNK_BATCH_SIZE);
    let batch_count = batches.len();
    for (index, batch) in batches.into_iter().enumerate() {
        let batch_chunks = batch
            .iter()
            .map(|document| document.chunks.len() as u64)
            .sum();
        coordinator
            .report(
                events,
                PipelinePhase::Batching,
                progress.batched(batch_chunks),
                "batching web source chunks",
            )
            .await;
        let batch_result = match super::provider::vectorize_documents(
            input,
            ledger,
            embedding_provider,
            vector_store,
            collection.clone(),
            batch,
            events,
            coordinator,
            progress,
            final_changed_batch && index + 1 == batch_count,
        )
        .await
        {
            Ok(result) => result,
            Err(error) => {
                return Err(cleanup_artifacts_after_error(
                    input.artifact_store.as_ref(),
                    artifacts,
                    error,
                )
                .await);
            }
        };
        merge_vectorize_result(&mut result, batch_result);
    }
    Ok(result)
}

pub(crate) async fn prepare_changed_documents_without_vectors(
    input: &WebSourceIndexInput,
    run: &WebAdapterRun,
    diff: &SourceManifestDiff,
    generation: &SourceGenerationId,
    ledger: &dyn LedgerStore,
    events: &SourceEventEmitter,
    coordinator: &WebProgressCoordinator,
    progress: &mut WebPipelineProgress,
) -> anyhow::Result<VectorizeResult> {
    let mut result = VectorizeResult::default();
    let batches = changed_diff_batches(diff, WEB_CHANGED_DOCUMENT_BATCH_SIZE);
    let batch_count = batches.len();
    for (index, batch_diff) in batches.into_iter().enumerate() {
        let batch_result = prepare_changed_batch_without_vectors(
            input,
            run,
            &batch_diff,
            generation,
            ledger,
            events,
            coordinator,
            progress,
            index + 1 == batch_count,
        )
        .await?;
        merge_vectorize_result(&mut result, batch_result);
    }
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
async fn prepare_changed_batch_without_vectors(
    input: &WebSourceIndexInput,
    run: &WebAdapterRun,
    diff: &SourceManifestDiff,
    generation: &SourceGenerationId,
    ledger: &dyn LedgerStore,
    events: &SourceEventEmitter,
    coordinator: &WebProgressCoordinator,
    progress: &mut WebPipelineProgress,
    final_batch: bool,
) -> anyhow::Result<VectorizeResult> {
    let normalized =
        normalize_changed_documents(input, run, diff, events, coordinator, progress, final_batch)
            .await?;
    let (documents, mut result) = split_normalized(normalized);
    coordinator
        .report(
            events,
            PipelinePhase::Preparing,
            progress.preparing_counts(),
            "preparing web source documents",
        )
        .await;
    let prepared = prepare_or_cleanup(input, generation, &result.artifacts, documents).await?;
    let document_count = prepared.len() as u64;
    let chunk_count = prepared
        .iter()
        .map(|document| document.chunks.len() as u64)
        .sum();
    coordinator
        .checkpoint(
            events,
            PipelinePhase::Preparing,
            progress.prepared(document_count, chunk_count, final_batch),
            "prepared web source documents",
        )
        .await;
    record_prepared_statuses(ledger, prepared, &mut result).await?;
    Ok(result)
}

async fn record_prepared_statuses(
    ledger: &dyn LedgerStore,
    documents: Vec<PreparedDocument>,
    result: &mut VectorizeResult,
) -> anyhow::Result<()> {
    for document in documents {
        result.documents_prepared += 1;
        result.chunks_prepared += document.chunks.len() as u64;
        result
            .graph_candidates
            .extend(document.graph_candidates.clone());
        let status = document_status(&document, 0, DocumentLifecycleStatus::Prepared, timestamp());
        ledger.update_document_status(status.clone()).await?;
        result.document_statuses.push(status);
    }
    Ok(())
}

fn merge_vectorize_result(output: &mut VectorizeResult, mut batch: VectorizeResult) {
    output.documents_prepared += batch.documents_prepared;
    output.chunks_prepared += batch.chunks_prepared;
    output.points_attempted += batch.points_attempted;
    output
        .document_statuses
        .append(&mut batch.document_statuses);
    output.reused_item_keys.append(&mut batch.reused_item_keys);
    output.graph_candidates.append(&mut batch.graph_candidates);
    output.warnings.append(&mut batch.warnings);
    output.artifacts.append(&mut batch.artifacts);
    output.artifact_index.merge(batch.artifact_index);
    if output.inline.is_none() {
        output.inline = batch.inline;
    }
}
