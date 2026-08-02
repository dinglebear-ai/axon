use super::*;

pub(super) struct NoVectorGenerationRequest<'a> {
    pub(super) input: &'a WebSourceIndexInput,
    pub(super) ledger: &'a dyn LedgerStore,
    pub(super) run: WebAdapterRun,
    pub(super) lease: &'a LeaseGuard,
    pub(super) generation: SourceGeneration,
    pub(super) manifest: SourceManifest,
    pub(super) diff: SourceManifestDiff,
    pub(super) events: &'a crate::source::events::SourceEventEmitter,
    pub(super) coordinator: &'a progress::WebProgressCoordinator,
    pub(super) progress: &'a mut progress::WebPipelineProgress,
}

pub(super) async fn publish_prepared_generation_without_vectors(
    request: NoVectorGenerationRequest<'_>,
) -> anyhow::Result<WebSourceIndexOutput> {
    let NoVectorGenerationRequest {
        input,
        ledger,
        run,
        lease,
        generation,
        mut manifest,
        diff,
        events,
        coordinator,
        progress,
    } = request;
    let prepared = prepare_no_vector_generation(
        input,
        ledger,
        &run,
        &generation,
        &diff,
        events,
        coordinator,
        progress,
    )
    .await?;
    let effective_diff = apply_reused_item_keys(&diff, &prepared.reused_item_keys);
    ensure_no_vector_ready(
        input,
        ledger,
        lease,
        &generation,
        &mut manifest,
        &effective_diff,
        &prepared,
    )
    .await?;
    let published = complete_and_publish_no_vector(
        input,
        ledger,
        generation,
        &manifest,
        &effective_diff,
        &prepared,
    )
    .await?;
    record_no_vector_output(
        input,
        ledger,
        run,
        manifest,
        effective_diff,
        prepared,
        published,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn prepare_no_vector_generation(
    input: &WebSourceIndexInput,
    ledger: &dyn LedgerStore,
    run: &WebAdapterRun,
    generation: &SourceGeneration,
    diff: &SourceManifestDiff,
    events: &crate::source::events::SourceEventEmitter,
    coordinator: &progress::WebProgressCoordinator,
    progress: &mut progress::WebPipelineProgress,
) -> anyhow::Result<vectorize::VectorizeResult> {
    match prepare_changed_documents_without_vectors(
        input,
        run,
        diff,
        &generation.generation,
        ledger,
        events,
        coordinator,
        progress,
    )
    .await
    .map_err(|error| error.context("failed to prepare web source generation without vectors"))
    {
        Ok(prepared) => Ok(prepared),
        Err(error) => Err(fail_generation(ledger, generation.clone(), error).await),
    }
}

#[allow(clippy::too_many_arguments)]
async fn ensure_no_vector_ready(
    input: &WebSourceIndexInput,
    ledger: &dyn LedgerStore,
    lease: &LeaseGuard,
    generation: &SourceGeneration,
    manifest: &mut SourceManifest,
    diff: &SourceManifestDiff,
    prepared: &vectorize::VectorizeResult,
) -> anyhow::Result<()> {
    if let Err(error) = ensure_lease_before_publish(ledger, input, lease).await {
        return Err(cleanup_after_failure(
            input,
            prepared,
            fail_generation(ledger, generation.clone(), error).await,
        )
        .await);
    }
    if let Err(error) =
        record_artifacts_on_manifest(ledger, manifest, diff, &prepared.artifact_index).await
    {
        return Err(cleanup_after_failure(
            input,
            prepared,
            fail_generation(ledger, generation.clone(), error).await,
        )
        .await);
    }
    Ok(())
}

async fn complete_and_publish_no_vector(
    input: &WebSourceIndexInput,
    ledger: &dyn LedgerStore,
    generation: SourceGeneration,
    manifest: &SourceManifest,
    diff: &SourceManifestDiff,
    prepared: &vectorize::VectorizeResult,
) -> anyhow::Result<SourceGeneration> {
    let counts = GenerationDocumentCounts {
        discovered: manifest.items.len() as u64,
        prepared: prepared.documents_prepared,
        embedded: 0,
        published: prepared.documents_prepared,
        failed: 0,
    };
    let completed = match complete_generation(ledger, generation.clone(), diff, counts).await {
        Ok(completed) => completed,
        Err(error) => {
            let error = fail_generation(ledger, generation, error).await;
            return Err(cleanup_after_failure(input, prepared, error).await);
        }
    };
    match publish_generation_without_vectors(ledger, &completed).await {
        Ok(published) => Ok(published),
        Err(error) => Err(cleanup_after_failure(input, prepared, error).await),
    }
}

async fn record_no_vector_output(
    input: &WebSourceIndexInput,
    ledger: &dyn LedgerStore,
    run: WebAdapterRun,
    manifest: SourceManifest,
    effective_diff: SourceManifestDiff,
    prepared: vectorize::VectorizeResult,
    published: SourceGeneration,
) -> anyhow::Result<WebSourceIndexOutput> {
    for status in &prepared.document_statuses {
        ledger
            .update_document_status(published_status(status))
            .await?;
    }
    ledger
        .upsert_source(completed_source_summary(
            input,
            &run,
            manifest.items.len() as u64,
            &effective_diff,
            0,
        ))
        .await?;
    Ok(WebSourceIndexOutput {
        job_id: input.job_id,
        source_id: run.source_id,
        generation: published.generation,
        items_discovered: manifest.items.len() as u64,
        documents_prepared: prepared.documents_prepared,
        chunks_prepared: prepared.chunks_prepared,
        vector_points_written: 0,
        removed_pages: effective_diff.counts.removed,
        graph_candidates: prepared.graph_candidates,
        warnings: prepared.warnings,
        artifacts: prepared.artifacts,
        inline: prepared.inline,
    })
}

async fn cleanup_after_failure(
    input: &WebSourceIndexInput,
    prepared: &vectorize::VectorizeResult,
    error: anyhow::Error,
) -> anyhow::Error {
    cleanup_artifacts_after_error(input.artifact_store.as_ref(), &prepared.artifacts, error).await
}
