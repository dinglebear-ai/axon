use axon_api::source::*;
use axon_embedding::provider::EmbeddingProvider;
use axon_ledger::store::LedgerStore;
use axon_vectors::store::VectorStore;

use super::artifacts::{cleanup_artifacts_after_error, record_artifacts_on_manifest};
use super::publish::{
    GenerationDocumentCounts, complete_generation, completed_source_summary,
    ensure_lease_before_publish, fail_generation, fail_generation_and_rollback_vectors,
    mark_vectors_for_completed_generation, publish_generation_and_rollback_vectors,
};
use super::run::{WebAdapterRun, apply_reused_item_keys};
use super::vectorize::{
    VectorizeResult, collection_spec, published_status, vectorize_changed_documents,
};
use super::{WebSourceIndexInput, WebSourceIndexOutput};

pub(super) struct VectorGenerationRequest<'a> {
    pub(super) input: &'a WebSourceIndexInput,
    pub(super) ledger: &'a dyn LedgerStore,
    pub(super) embedding_provider: &'a dyn EmbeddingProvider,
    pub(super) vector_store: &'a dyn VectorStore,
    pub(super) run: WebAdapterRun,
    pub(super) lease: &'a LeaseGuard,
    pub(super) generation: SourceGeneration,
    pub(super) manifest: SourceManifest,
    pub(super) diff: SourceManifestDiff,
}

pub(super) async fn publish_vector_generation(
    request: VectorGenerationRequest<'_>,
) -> anyhow::Result<WebSourceIndexOutput> {
    let VectorGenerationRequest {
        input,
        ledger,
        embedding_provider,
        vector_store,
        run,
        lease,
        generation,
        mut manifest,
        diff,
    } = request;
    let collection = collection_spec(input);
    let vectorized = vectorize_or_rollback(VectorizeWebGeneration {
        input,
        ledger,
        embedding_provider,
        vector_store,
        run: &run,
        generation: &generation,
        diff: &diff,
        collection: &collection,
    })
    .await?;
    let effective_diff = apply_reused_item_keys(&diff, &vectorized.reused_item_keys);
    ensure_vector_publish_ready(VectorPublishReady {
        input,
        ledger,
        vector_store,
        collection: &collection,
        lease,
        generation: &generation,
        manifest: &mut manifest,
        diff: &effective_diff,
        vectorized: &vectorized,
    })
    .await?;
    let completed = complete_generation_or_rollback(CompleteVectorGeneration {
        ledger,
        vector_store,
        collection: &collection,
        generation,
        manifest: &manifest,
        diff: &effective_diff,
        vectorized: &vectorized,
        artifact_store: input.artifact_store.as_ref(),
    })
    .await?;
    let (published, points_written) = publish_completed_vectors(PublishCompletedVectors {
        input,
        ledger,
        vector_store,
        collection: &collection,
        run: &run,
        completed,
        diff: &effective_diff,
        vectorized: &vectorized,
    })
    .await?;
    record_published_vector_generation(PublishedVectorRecord {
        input,
        ledger,
        run: &run,
        manifest: &manifest,
        diff: &effective_diff,
        vectorized,
        published,
        points_written,
    })
    .await
}

struct VectorizeWebGeneration<'a> {
    input: &'a WebSourceIndexInput,
    ledger: &'a dyn LedgerStore,
    embedding_provider: &'a dyn EmbeddingProvider,
    vector_store: &'a dyn VectorStore,
    run: &'a WebAdapterRun,
    generation: &'a SourceGeneration,
    diff: &'a SourceManifestDiff,
    collection: &'a CollectionSpec,
}

async fn vectorize_or_rollback(
    request: VectorizeWebGeneration<'_>,
) -> anyhow::Result<VectorizeResult> {
    let VectorizeWebGeneration {
        input,
        ledger,
        embedding_provider,
        vector_store,
        run,
        generation,
        diff,
        collection,
    } = request;
    if let Err(err) = vector_store.ensure_collection(collection.clone()).await {
        return Err(fail_generation(ledger, generation.clone(), anyhow::Error::new(err)).await);
    }
    match vectorize_changed_documents(
        input,
        run,
        diff,
        &generation.generation,
        ledger,
        embedding_provider,
        vector_store,
        collection.clone(),
    )
    .await
    .map_err(|err| err.context("failed to vectorize web source generation"))
    {
        Ok(vectorized) => Ok(vectorized),
        Err(err) => Err(fail_generation_and_rollback_vectors(
            ledger,
            vector_store,
            collection,
            generation.clone(),
            err,
        )
        .await),
    }
}

struct VectorPublishReady<'a> {
    input: &'a WebSourceIndexInput,
    ledger: &'a dyn LedgerStore,
    vector_store: &'a dyn VectorStore,
    collection: &'a CollectionSpec,
    lease: &'a LeaseGuard,
    generation: &'a SourceGeneration,
    manifest: &'a mut SourceManifest,
    diff: &'a SourceManifestDiff,
    vectorized: &'a VectorizeResult,
}

async fn ensure_vector_publish_ready(request: VectorPublishReady<'_>) -> anyhow::Result<()> {
    let VectorPublishReady {
        input,
        ledger,
        vector_store,
        collection,
        lease,
        generation,
        manifest,
        diff,
        vectorized,
    } = request;
    if let Err(err) = ensure_lease_before_publish(ledger, input, lease).await {
        let err = fail_generation_and_rollback_vectors(
            ledger,
            vector_store,
            collection,
            generation.clone(),
            err,
        )
        .await;
        return Err(cleanup_artifacts_after_error(
            input.artifact_store.as_ref(),
            &vectorized.artifacts,
            err,
        )
        .await);
    }
    if let Err(err) =
        record_artifacts_on_manifest(ledger, manifest, diff, &vectorized.artifact_index).await
    {
        let err = fail_generation_and_rollback_vectors(
            ledger,
            vector_store,
            collection,
            generation.clone(),
            err,
        )
        .await;
        return Err(cleanup_artifacts_after_error(
            input.artifact_store.as_ref(),
            &vectorized.artifacts,
            err,
        )
        .await);
    }
    Ok(())
}

struct PublishCompletedVectors<'a> {
    input: &'a WebSourceIndexInput,
    ledger: &'a dyn LedgerStore,
    vector_store: &'a dyn VectorStore,
    collection: &'a CollectionSpec,
    run: &'a WebAdapterRun,
    completed: SourceGeneration,
    diff: &'a SourceManifestDiff,
    vectorized: &'a VectorizeResult,
}

async fn publish_completed_vectors(
    request: PublishCompletedVectors<'_>,
) -> anyhow::Result<(SourceGeneration, u64)> {
    let PublishCompletedVectors {
        input,
        ledger,
        vector_store,
        collection,
        run,
        completed,
        diff,
        vectorized,
    } = request;
    let publish_stats = match mark_vectors_for_completed_generation(
        vector_store,
        collection,
        &run.source_id,
        &completed,
        diff,
        vectorized.points_attempted,
    )
    .await
    {
        Ok(stats) => stats,
        Err(err) => {
            let err = fail_generation_and_rollback_vectors(
                ledger,
                vector_store,
                collection,
                completed.clone(),
                err,
            )
            .await;
            return Err(cleanup_artifacts_after_error(
                input.artifact_store.as_ref(),
                &vectorized.artifacts,
                err,
            )
            .await);
        }
    };
    let published =
        match publish_generation_and_rollback_vectors(ledger, vector_store, collection, &completed)
            .await
        {
            Ok(published) => published,
            Err(err) => {
                return Err(cleanup_artifacts_after_error(
                    input.artifact_store.as_ref(),
                    &vectorized.artifacts,
                    err,
                )
                .await);
            }
        };
    Ok((published, publish_stats.total_points_written()))
}

struct CompleteVectorGeneration<'a> {
    ledger: &'a dyn LedgerStore,
    vector_store: &'a dyn VectorStore,
    collection: &'a CollectionSpec,
    generation: SourceGeneration,
    manifest: &'a SourceManifest,
    diff: &'a SourceManifestDiff,
    vectorized: &'a VectorizeResult,
    artifact_store: &'a dyn axon_core::boundary::ArtifactStore,
}

async fn complete_generation_or_rollback(
    request: CompleteVectorGeneration<'_>,
) -> anyhow::Result<SourceGeneration> {
    let CompleteVectorGeneration {
        ledger,
        vector_store,
        collection,
        generation,
        manifest,
        diff,
        vectorized,
        artifact_store,
    } = request;
    match complete_generation(
        ledger,
        generation.clone(),
        diff,
        GenerationDocumentCounts {
            discovered: manifest.items.len() as u64,
            prepared: vectorized.documents_prepared,
            embedded: vectorized.documents_prepared,
            published: vectorized.documents_prepared,
            failed: 0,
        },
    )
    .await
    {
        Ok(completed) => Ok(completed),
        Err(err) => {
            let err = fail_generation_and_rollback_vectors(
                ledger,
                vector_store,
                collection,
                generation,
                err,
            )
            .await;
            Err(cleanup_artifacts_after_error(artifact_store, &vectorized.artifacts, err).await)
        }
    }
}

struct PublishedVectorRecord<'a> {
    input: &'a WebSourceIndexInput,
    ledger: &'a dyn LedgerStore,
    run: &'a WebAdapterRun,
    manifest: &'a SourceManifest,
    diff: &'a SourceManifestDiff,
    vectorized: VectorizeResult,
    published: SourceGeneration,
    points_written: u64,
}

async fn record_published_vector_generation(
    record: PublishedVectorRecord<'_>,
) -> anyhow::Result<WebSourceIndexOutput> {
    let PublishedVectorRecord {
        input,
        ledger,
        run,
        manifest,
        diff,
        vectorized,
        published,
        points_written,
    } = record;
    for status in &vectorized.document_statuses {
        ledger
            .update_document_status(published_status(status))
            .await?;
    }
    ledger
        .upsert_source(completed_source_summary(
            input,
            run,
            manifest.items.len() as u64,
            diff,
            points_written,
        ))
        .await?;
    Ok(WebSourceIndexOutput {
        job_id: input.job_id,
        source_id: run.source_id.clone(),
        generation: published.generation,
        documents_prepared: vectorized.documents_prepared,
        chunks_prepared: vectorized.chunks_prepared,
        vector_points_written: points_written,
        removed_pages: diff.counts.removed,
        graph_candidates: vectorized.graph_candidates,
        warnings: vectorized.warnings,
        artifacts: vectorized.artifacts,
        inline: vectorized.inline,
    })
}
