use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) async fn vectorize_documents(
    input: &WebSourceIndexInput,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
    collection: CollectionSpec,
    documents: Vec<PreparedDocument>,
    events: &SourceEventEmitter,
    coordinator: &WebProgressCoordinator,
    progress: &mut WebPipelineProgress,
    final_vector_batch: bool,
) -> anyhow::Result<VectorizeResult> {
    if documents.is_empty() {
        return Ok(VectorizeResult::default());
    }
    let batch_chunks = documents
        .iter()
        .map(|document| document.chunks.len() as u64)
        .sum();
    let embeddings = embed_documents(
        input,
        embedding_provider,
        &documents,
        batch_chunks,
        events,
        coordinator,
        progress,
    )
    .await?;
    let built_points = vector_point_batch_for_documents(collection, &documents, &embeddings)?;
    let expected_points = built_points.batch.points.len() as u64;
    let skipped_redaction = built_points.skipped_redaction;
    let points_by_document = built_points.points_by_document;
    coordinator
        .report(
            events,
            PipelinePhase::Vectorizing,
            progress.vectorized(expected_points, final_vector_batch),
            "built web vector point batch",
        )
        .await;
    let write = upsert_points(
        input,
        vector_store,
        built_points.batch,
        expected_points,
        events,
        coordinator,
        progress,
    )
    .await?;
    build_vectorized_result(
        ledger,
        documents,
        skipped_redaction,
        points_by_document,
        write.points_attempted,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn embed_documents(
    input: &WebSourceIndexInput,
    embedding_provider: &dyn EmbeddingProvider,
    documents: &[PreparedDocument],
    batch_chunks: u64,
    events: &SourceEventEmitter,
    coordinator: &WebProgressCoordinator,
    progress: &mut WebPipelineProgress,
) -> anyhow::Result<EmbeddingResult> {
    let counts = progress.embedding_counts();
    coordinator
        .report(
            events,
            PipelinePhase::Embedding,
            counts.clone(),
            "embedding web source chunks",
        )
        .await;
    let reservation = input
        .embedding_reservations
        .reserve_with_context_wait(ProviderReservationContext {
            job_id: input.job_id,
            stage_id: None,
            provider_id: Some(input.embedding_provider_id.clone()),
            priority: JobPriority::Background,
            units: 1,
            ttl_seconds: Some(300),
        })
        .await?;
    coordinator
        .heartbeat(PipelinePhase::Embedding, counts, &reservation)
        .await;
    let embeddings = embedding_provider
        .embed(embedding_batch_for_documents(input, documents)?)
        .await?;
    drop(reservation);
    coordinator
        .checkpoint(
            events,
            PipelinePhase::Embedding,
            progress.embedded(batch_chunks),
            "embedded web source chunks",
        )
        .await;
    Ok(embeddings)
}

#[allow(clippy::too_many_arguments)]
async fn upsert_points(
    input: &WebSourceIndexInput,
    vector_store: &dyn VectorStore,
    batch: VectorPointBatch,
    expected_points: u64,
    events: &SourceEventEmitter,
    coordinator: &WebProgressCoordinator,
    progress: &mut WebPipelineProgress,
) -> anyhow::Result<VectorStoreWriteResult> {
    let counts = progress.upserting_counts();
    coordinator
        .report(
            events,
            PipelinePhase::Upserting,
            counts.clone(),
            "upserting web source vectors",
        )
        .await;
    let reservation = input
        .vector_reservations
        .reserve_with_context_wait(ProviderReservationContext {
            job_id: input.job_id,
            stage_id: None,
            provider_id: Some(input.vector_provider_id.clone()),
            priority: JobPriority::Background,
            units: 1,
            ttl_seconds: Some(300),
        })
        .await?;
    coordinator
        .heartbeat(PipelinePhase::Upserting, counts, &reservation)
        .await;
    let write = vector_store.upsert(batch).await?;
    drop(reservation);
    if write.points_attempted != write.points_written || write.points_written != expected_points {
        anyhow::bail!(
            "upsert wrote {} of {} attempted points; expected {expected_points}",
            write.points_written,
            write.points_attempted
        );
    }
    coordinator
        .checkpoint(
            events,
            PipelinePhase::Upserting,
            progress.upserted(write.points_written),
            "upserted web source vectors",
        )
        .await;
    Ok(write)
}

async fn build_vectorized_result(
    ledger: &dyn LedgerStore,
    documents: Vec<PreparedDocument>,
    skipped_redaction: u64,
    points_by_document: std::collections::BTreeMap<DocumentId, u64>,
    points_attempted: u64,
) -> anyhow::Result<VectorizeResult> {
    let mut result = VectorizeResult {
        points_attempted,
        ..VectorizeResult::default()
    };
    if skipped_redaction > 0 {
        result.warnings.push(SourceWarning {
            code: "web.vectorize.redaction_skipped_chunks".to_string(),
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
    for document in documents {
        result.chunks_prepared += document.chunks.len() as u64;
        result.documents_prepared += 1;
        result
            .graph_candidates
            .extend(document.graph_candidates.clone());
        result.warnings.extend(document.warnings.clone());
        let status = vectorized_document_status(&document, &points_by_document, timestamp())?;
        ledger.update_document_status(status.clone()).await?;
        result.document_statuses.push(status);
    }
    Ok(result)
}
