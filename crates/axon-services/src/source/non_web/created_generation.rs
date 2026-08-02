//! Run + publish one already-created source generation.
//!
//! Split out of `non_web.rs` to stay under the monolith line cap; owns the
//! streaming acquire/normalize/prepare/embed/publish loop
//! (`run_created_generation`) and the terminal ledger/vector-store publish
//! step (`publish_created_generation`).

use axon_api::source::*;

use super::helpers::*;
use super::progress::{PipelineProgress, ProgressCoordinator, stage_counts};
use super::{
    ACQUIRE_BATCH_SIZE, NonWebPipelineInput, SOURCE_LEASE_TTL_SECONDS, SourceEventEmitter,
    metadata, publish, vectorize,
};
use crate::context::TargetLocalSourceRuntime;
use crate::source::progress as source_progress;
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
#[derive(Default)]
struct GenerationRunState {
    vectorized: vectorize::VectorizeResult,
    pipeline: PipelineProgress,
    artifacts: Vec<ArtifactRef>,
    warnings: Vec<SourceWarning>,
    acquired_items: u64,
    acquired_documents: u64,
    enriched_items: u64,
    normalized_documents: u64,
}

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
    coordinator: &ProgressCoordinator,
) -> anyhow::Result<IndexCounts> {
    let collection = collection_spec(input.collection, runtime.embedding_dimensions);
    if input.plan.request.embed {
        runtime
            .vector_store
            .ensure_collection(collection.clone())
            .await?;
    }
    let changed_total = diff.added.len().saturating_add(diff.modified.len()) as u64;
    coordinator
        .report(
            emitter,
            PipelinePhase::Fetching,
            stage_counts(Some(changed_total), 0, Some(changed_total), 0, None, 0),
            "acquiring changed source items",
        )
        .await;
    let mut state = GenerationRunState::default();
    let batches = batch_changed_diff(&diff, ACQUIRE_BATCH_SIZE);
    let batch_count = batches.len();
    for (index, batch_diff) in batches.into_iter().enumerate() {
        process_generation_batch(
            runtime,
            input,
            emitter,
            &generation.generation,
            &collection,
            coordinator,
            changed_total,
            index + 1 == batch_count,
            batch_diff,
            &mut state,
        )
        .await?;
    }
    state.vectorized.warnings.splice(0..0, state.warnings);
    coordinator
        .report(
            emitter,
            PipelinePhase::Publishing,
            stage_counts(Some(1), 0, None, 0, None, 0),
            "publishing source generation",
        )
        .await;
    let result = publish_created_generation(
        runtime,
        input,
        emitter,
        lease,
        manifest,
        diff,
        generation,
        previous,
        collection,
        state.vectorized,
        state.artifacts,
    )
    .await;
    if result.is_ok() {
        coordinator
            .checkpoint(
                PipelinePhase::Publishing,
                stage_counts(Some(1), 1, None, 0, None, 0),
                "published source generation",
            )
            .await;
    }
    result
}

#[allow(clippy::too_many_arguments)]
async fn process_generation_batch(
    runtime: &TargetLocalSourceRuntime,
    input: &NonWebPipelineInput<'_>,
    emitter: &SourceEventEmitter,
    generation: &SourceGenerationId,
    collection: &CollectionSpec,
    coordinator: &ProgressCoordinator,
    changed_total: u64,
    is_final_batch: bool,
    batch_diff: SourceManifestDiff,
    state: &mut GenerationRunState,
) -> anyhow::Result<()> {
    let acquisition = acquire_generation_batch(
        input,
        emitter,
        coordinator,
        changed_total,
        batch_diff,
        state,
    )
    .await?;
    let enrichments = enrich_generation_batch(
        runtime,
        input,
        emitter,
        coordinator,
        &acquisition,
        is_final_batch,
        state,
    )
    .await?;
    let mut documents = normalize_generation_batch(
        input,
        emitter,
        generation,
        coordinator,
        acquisition,
        is_final_batch,
        state,
    )
    .await?;
    apply_enrichments(&mut documents, &enrichments);
    let enrichment_graph = enrichment_graph_candidates(&enrichments);
    let batch_result = vectorize::prepare_embed_publish(
        runtime,
        input,
        documents,
        &enrichment_graph,
        generation,
        collection.clone(),
        emitter,
        coordinator,
        &mut state.pipeline,
        is_final_batch,
    )
    .await?;
    collect_enrichment_output(&enrichments, state);
    vectorize::merge_vectorize_result(&mut state.vectorized, batch_result);
    Ok(())
}

async fn acquire_generation_batch(
    input: &NonWebPipelineInput<'_>,
    emitter: &SourceEventEmitter,
    coordinator: &ProgressCoordinator,
    changed_total: u64,
    batch_diff: SourceManifestDiff,
    state: &mut GenerationRunState,
) -> anyhow::Result<SourceAcquisition> {
    let batch_items = batch_diff
        .added
        .len()
        .saturating_add(batch_diff.modified.len()) as u64;
    let reporter = coordinator.acquisition_batch(
        changed_total,
        batch_items,
        state.acquired_items,
        state.acquired_documents,
    );
    let acquisition = input
        .adapter
        .acquire_with_progress(&input.plan, &batch_diff, Some(&reporter))
        .await?;
    let fetched = acquisition.fetched_items.len() as u64;
    reporter.complete(fetched).await;
    state.acquired_items = state.acquired_items.saturating_add(batch_items);
    state.acquired_documents = state.acquired_documents.saturating_add(fetched);
    state.artifacts.extend(acquisition.artifacts.clone());
    state.warnings.extend(acquisition.header.warnings.clone());
    source_progress::acquired(emitter, &acquisition).await;
    Ok(acquisition)
}

async fn enrich_generation_batch(
    runtime: &TargetLocalSourceRuntime,
    input: &NonWebPipelineInput<'_>,
    emitter: &SourceEventEmitter,
    coordinator: &ProgressCoordinator,
    acquisition: &SourceAcquisition,
    is_final_batch: bool,
    state: &mut GenerationRunState,
) -> anyhow::Result<std::collections::BTreeMap<SourceItemKey, SourceEnrichment>> {
    let total = is_final_batch.then_some(state.acquired_documents);
    coordinator
        .report(
            emitter,
            PipelinePhase::Enriching,
            stage_counts(total, state.enriched_items, None, 0, None, 0),
            "enriching acquired source items",
        )
        .await;
    let enrichments = enrich(
        runtime.enricher.clone(),
        &input.plan,
        &acquisition.fetched_items,
    )
    .await?;
    state.enriched_items = state
        .enriched_items
        .saturating_add(acquisition.fetched_items.len() as u64);
    coordinator
        .checkpoint(
            PipelinePhase::Enriching,
            stage_counts(total, state.enriched_items, None, 0, None, 0),
            "enriched acquired source items",
        )
        .await;
    Ok(enrichments)
}

async fn normalize_generation_batch(
    input: &NonWebPipelineInput<'_>,
    emitter: &SourceEventEmitter,
    generation: &SourceGenerationId,
    coordinator: &ProgressCoordinator,
    acquisition: SourceAcquisition,
    is_final_batch: bool,
    state: &mut GenerationRunState,
) -> anyhow::Result<Vec<SourceDocument>> {
    let total = is_final_batch.then_some(state.acquired_documents);
    let counts = stage_counts(
        total,
        state.normalized_documents,
        total,
        state.normalized_documents,
        None,
        0,
    );
    coordinator
        .report(
            emitter,
            PipelinePhase::Normalizing,
            counts,
            "normalizing source documents",
        )
        .await;
    let normalized = input.adapter.normalize(&input.plan, acquisition).await?;
    source_progress::normalized(emitter, generation, &normalized.header).await;
    state.warnings.extend(normalized.header.warnings.clone());
    state.normalized_documents = state
        .normalized_documents
        .saturating_add(normalized.data.len() as u64);
    state.pipeline.add_documents(normalized.data.len() as u64);
    if is_final_batch {
        state.pipeline.finish_documents();
    }
    coordinator
        .checkpoint(
            PipelinePhase::Normalizing,
            stage_counts(
                total,
                state.normalized_documents,
                total,
                state.normalized_documents,
                None,
                0,
            ),
            "normalized source documents",
        )
        .await;
    Ok(normalized.data)
}

fn collect_enrichment_output(
    enrichments: &std::collections::BTreeMap<SourceItemKey, SourceEnrichment>,
    state: &mut GenerationRunState,
) {
    for enrichment in enrichments.values() {
        state.warnings.extend(enrichment.warnings.clone());
        state.artifacts.extend(enrichment.artifacts.clone());
    }
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
    let finalizer = runtime
        .ledger
        .acquire_lease(LeaseRequest {
            lease_key: format!("publication:{}", generation.source_id.0),
            owner_id: input.owner_id.to_string(),
            ttl_seconds: SOURCE_LEASE_TTL_SECONDS,
            job_id: Some(input.plan.job_id),
            metadata: MetadataMap::new(),
        })
        .await?
        .ok_or_else(|| anyhow::anyhow!("source publication finalizer is already leased"))?;
    let result = publish_created_generation_under_finalizer(
        runtime, input, emitter, lease, manifest, diff, generation, previous, collection,
        vectorized, artifacts,
    )
    .await;
    let release = runtime
        .ledger
        .release_lease(finalizer.lease_id, input.owner_id.to_string())
        .await;
    match (result, release) {
        (Ok(counts), Ok(())) => Ok(counts),
        (Err(err), Ok(())) => Err(err),
        (Ok(_), Err(err)) => Err(err.into()),
        (Err(err), Err(release_err)) => Err(err.context(format!(
            "source publication finalizer release also failed: {release_err}"
        ))),
    }
}

#[allow(clippy::too_many_arguments)]
async fn publish_created_generation_under_finalizer(
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
    source_progress::published(
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
