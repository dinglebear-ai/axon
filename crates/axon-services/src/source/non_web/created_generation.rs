//! Run + publish one already-created source generation.
//!
//! Split out of `non_web.rs` to stay under the monolith line cap; owns the
//! streaming acquire/normalize/prepare/embed/publish loop
//! (`run_created_generation`) and the terminal ledger/vector-store publish
//! step (`publish_created_generation`).

use axon_api::source::*;

use super::helpers::*;
use super::{
    ACQUIRE_BATCH_SIZE, NonWebPipelineInput, SourceEventEmitter, metadata, publish, vectorize,
};
use crate::context::TargetLocalSourceRuntime;
use crate::source::progress;
use crate::source::result_map::IndexCounts;

/// Acquire/normalize/prepare/embed/publish the diff's added+modified items in
/// bounded batches (`ACQUIRE_BATCH_SIZE`) rather than a single
/// `adapter.acquire(&plan, &diff)` call for the whole changed corpus.
///
/// Before this, `non_web` was the only one of the three parallel pipelines
/// that materialized an entire changed generation's fetched+normalized
/// documents in memory at once before handing anything to
/// prepare/embed/publish — `web_source`/`local_source` (now merged into this
/// runner) always streamed per ~64-item diff batch. For the largest corpora
/// this crate acquires (git repos, session directories), that was an
/// unbounded-memory / OOM risk unique to this path. Streaming here brings
/// every non-web family, plus local (`source/dispatch/local.rs`), onto the
/// same bounded-memory acquisition shape web already had.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_created_generation(
    runtime: &TargetLocalSourceRuntime,
    input: &NonWebPipelineInput<'_>,
    emitter: &SourceEventEmitter,
    lease: &LeaseGuard,
    manifest: SourceManifest,
    diff: SourceManifestDiff,
    generation: SourceGeneration,
    previous: Option<SourceSummary>,
) -> anyhow::Result<IndexCounts> {
    let collection = collection_spec(input.collection, runtime.embedding_dimensions);
    if input.plan.request.embed {
        runtime
            .vector_store
            .ensure_collection(collection.clone())
            .await?;
    }
    let mut vectorized = vectorize::VectorizeResult::default();
    let mut artifacts = Vec::new();
    let mut warnings = Vec::new();
    for batch_diff in batch_changed_diff(&diff, ACQUIRE_BATCH_SIZE) {
        record_running_phase(
            runtime,
            input,
            emitter,
            PipelinePhase::Fetching,
            "acquiring changed source items",
        )
        .await?;
        let acquisition = input.adapter.acquire(&input.plan, &batch_diff).await?;
        progress::acquired(emitter, &acquisition).await;
        artifacts.extend(acquisition.artifacts.clone());
        warnings.extend(acquisition.header.warnings.clone());
        let enrichments = enrich(
            runtime.enricher.clone(),
            &input.plan,
            &acquisition.fetched_items,
        )
        .await?;
        record_running_phase(
            runtime,
            input,
            emitter,
            PipelinePhase::Normalizing,
            "normalizing source documents",
        )
        .await?;
        let normalized = input.adapter.normalize(&input.plan, acquisition).await?;
        progress::normalized(emitter, &generation.generation, &normalized.header).await;
        warnings.extend(normalized.header.warnings.clone());
        let mut documents = normalized.data;
        apply_enrichments(&mut documents, &enrichments);
        let enrichment_graph = enrichment_graph_candidates(&enrichments);
        record_running_phase(
            runtime,
            input,
            emitter,
            PipelinePhase::Preparing,
            "preparing source documents",
        )
        .await?;
        let batch_result = vectorize::prepare_embed_publish(
            runtime,
            input,
            documents,
            &enrichment_graph,
            &generation.generation,
            collection.clone(),
            emitter,
        )
        .await?;
        for enrichment in enrichments.values() {
            warnings.extend(enrichment.warnings.clone());
            artifacts.extend(enrichment.artifacts.clone());
        }
        vectorize::merge_vectorize_result(&mut vectorized, batch_result);
    }
    vectorized.warnings.splice(0..0, warnings);

    publish_created_generation(
        runtime, input, emitter, lease, manifest, diff, generation, previous, collection,
        vectorized, artifacts,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn publish_created_generation(
    runtime: &TargetLocalSourceRuntime,
    input: &NonWebPipelineInput<'_>,
    emitter: &SourceEventEmitter,
    lease: &LeaseGuard,
    manifest: SourceManifest,
    diff: SourceManifestDiff,
    generation: SourceGeneration,
    previous: Option<SourceSummary>,
    collection: CollectionSpec,
    vectorized: vectorize::VectorizeResult,
    artifacts: Vec<ArtifactRef>,
) -> anyhow::Result<IndexCounts> {
    publish::ensure_lease(runtime.ledger.as_ref(), input, lease).await?;
    let generation = publish::complete_generation(
        runtime.ledger.as_ref(),
        generation,
        &diff,
        manifest.items.len() as u64,
        &vectorized,
    )
    .await?;
    let published = publish::publish(
        runtime.ledger.as_ref(),
        runtime.vector_store.as_ref(),
        &collection,
        &generation,
        &diff,
        input.plan.request.embed,
    )
    .await?;
    let published_statuses = vectorized
        .document_statuses
        .iter()
        .map(publish::published_status)
        .collect::<Vec<_>>();
    vectorize::write_document_statuses(runtime.ledger.as_ref(), &published_statuses).await?;
    let counts = terminal_source_counts(previous.as_ref(), &manifest, &diff, &vectorized);
    runtime
        .ledger
        .upsert_source(metadata::source_summary(
            input,
            LifecycleStatus::Completed,
            counts,
            previous.as_ref(),
        ))
        .await?;
    progress::published(
        emitter,
        &published.generation,
        manifest.items.len() as u64,
        &vectorized.warnings,
        vectorized.documents_prepared,
        vectorized.chunks_prepared,
    )
    .await;
    Ok(IndexCounts {
        job_id: input.plan.job_id,
        source_id: manifest.source_id,
        generation: published.generation,
        documents_prepared: vectorized.documents_prepared,
        chunks_prepared: vectorized.chunks_prepared,
        vector_points_written: vectorized.points_written,
        removed: diff.counts.removed,
        graph_candidates: vectorized.graph_candidates,
        warnings: vectorized.warnings,
        artifacts,
        inline: None,
    })
}
