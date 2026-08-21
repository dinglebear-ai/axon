//! Run + publish one already-created source generation.
//!
//! Split out of `executor.rs` to stay under the monolith line cap; owns the
//! streaming acquire/normalize/prepare/embed/publish loop
//! (`run_created_generation`) and the terminal ledger/vector-store publish
//! step (`publish_created_generation`).

use axon_api::source::*;

use super::generation_state::{GenerationAccumulator, GenerationStageProgress, ProcessedBatch};
use super::helpers::*;
use super::progress::{AcquisitionBatchProgress, ProgressCoordinator, stage_counts};
use super::{
    ACQUIRE_BATCH_SIZE, SOURCE_LEASE_TTL_SECONDS, SourceEventEmitter, SourcePipelineInput,
    artifact_candidates, metadata, publish, reuse, vectorize,
};
use crate::context::TargetLocalSourceRuntime;
use crate::reserved_call::ArtifactCleanupGuard;
use crate::source::output;
use crate::source::progress as source_progress;
use crate::source::result_map::IndexCounts;

mod candidate_delivery;
mod setup;

use candidate_delivery::{finish_candidate_delivery, stage_candidate_delivery};
use setup::ensure_generation_collection;

/// Acquire/normalize/prepare/embed/publish the diff's added+modified items in
/// bounded batches (`ACQUIRE_BATCH_SIZE`) rather than a single
/// `adapter.acquire(&plan, &diff)` call for the whole changed corpus.
///
/// The executor streams each changed generation in ~64-item diff batches
/// instead of materializing the entire fetched and normalized corpus before
/// prepare/embed/publish. This keeps large git repositories, session
/// directories, and web collections on one bounded-memory execution shape.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_created_generation(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    emitter: &SourceEventEmitter,
    lease: &LeaseGuard,
    mut manifest: SourceManifest,
    diff: SourceManifestDiff,
    generation: SourceGeneration,
    previous: Option<SourceSummary>,
    coordinator: &ProgressCoordinator,
) -> anyhow::Result<IndexCounts> {
    let collection = collection_spec(input.collection, runtime.embedding_dimensions);
    let mut artifact_cleanup = ArtifactCleanupGuard::new(
        runtime,
        generation.source_id.clone(),
        generation.generation.clone(),
    );
    ensure_generation_collection(runtime, input, &collection).await?;

    let archive_requested = input.adapter.wants_archive(&input.plan);
    let mut accumulated = GenerationAccumulator::default();
    let changed_total = diff.added.len().saturating_add(diff.modified.len()) as u64;
    let mut stage = GenerationStageProgress::default();

    coordinator
        .report(
            emitter,
            PipelinePhase::Fetching,
            stage_counts(Some(changed_total), 0, Some(changed_total), 0, None, 0),
            "acquiring changed source items",
        )
        .await;

    let batch_count = usize::try_from(changed_total)
        .unwrap_or(usize::MAX)
        .div_ceil(ACQUIRE_BATCH_SIZE);
    for (batch_index, batch_diff) in batch_changed_diff(&diff, ACQUIRE_BATCH_SIZE).enumerate() {
        let is_final_batch = batch_index + 1 == batch_count;
        let batch_items = batch_diff
            .added
            .len()
            .saturating_add(batch_diff.modified.len()) as u64;
        let reporter = coordinator.acquisition_batch(
            changed_total,
            batch_items,
            stage.acquired_items,
            stage.acquired_documents,
        );
        let batch = process_changed_batch(
            runtime,
            input,
            emitter,
            &generation.generation,
            &collection,
            batch_diff,
            archive_requested,
            is_final_batch,
            &reporter,
            coordinator,
            &mut stage,
        )
        .await?;
        accumulated.absorb(&mut artifact_cleanup, batch);
    }

    let finalized = accumulated
        .finalize(runtime, input, &mut artifact_cleanup, &mut manifest, diff)
        .await?;

    coordinator
        .report(
            emitter,
            PipelinePhase::Publishing,
            stage_counts(Some(1), 0, None, 0, None, 0),
            "publishing source generation",
        )
        .await;
    let mut candidates = finalized.artifact_candidates;
    let candidate_generation = generation.generation.clone();
    let staged_delivery = stage_candidate_delivery(
        runtime,
        input.plan.job_id,
        input.plan.route.source.source_id.clone(),
        candidate_generation.clone(),
        &mut candidates,
    )
    .await?;
    let mut result = publish_created_generation(
        runtime,
        input,
        emitter,
        lease,
        manifest,
        finalized.diff,
        generation,
        previous,
        collection,
        finalized.vectorized,
        finalized.artifacts,
        finalized.inline,
    )
    .await;
    finish_candidate_delivery(
        runtime,
        input,
        coordinator,
        &mut artifact_cleanup,
        &candidate_generation,
        candidates,
        staged_delivery,
        &mut result,
    )
    .await?;
    result
}

#[allow(clippy::too_many_arguments)]
async fn process_changed_batch(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    emitter: &SourceEventEmitter,
    generation: &SourceGenerationId,
    collection: &CollectionSpec,
    batch_diff: SourceManifestDiff,
    archive_requested: bool,
    is_final_batch: bool,
    reporter: &AcquisitionBatchProgress<'_>,
    coordinator: &ProgressCoordinator,
    stage: &mut GenerationStageProgress,
) -> anyhow::Result<ProcessedBatch> {
    let batch_items = batch_diff
        .added
        .len()
        .saturating_add(batch_diff.modified.len()) as u64;
    let acquisition = input
        .adapter
        .acquire_with_progress(&input.plan, &batch_diff, Some(reporter))
        .await?;
    let acquired_documents = acquisition.fetched_items.len() as u64;
    reporter.complete(acquired_documents).await;
    stage.acquired_items = stage.acquired_items.saturating_add(batch_items);
    stage.acquired_documents = stage.acquired_documents.saturating_add(acquired_documents);
    source_progress::acquired(emitter, &acquisition).await;

    let resolved = reuse::resolve_acquisition(runtime, input, &batch_diff, acquisition).await?;
    let refreshed_manifest_items = resolved.acquisition.manifest.items.clone();
    let acquisition_artifacts = resolved.acquisition.artifacts.clone();
    let archive_items = if archive_requested {
        resolved.acquisition.fetched_items.clone()
    } else {
        Vec::new()
    };

    let mut enrichments = enrich_changed_items(
        runtime,
        input,
        emitter,
        coordinator,
        stage,
        &resolved.acquisition.fetched_items,
        is_final_batch,
    )
    .await?;

    let total = is_final_batch.then_some(stage.acquired_documents);
    coordinator
        .report(
            emitter,
            PipelinePhase::Normalizing,
            stage_counts(
                total,
                stage.normalized_documents,
                total,
                stage.normalized_documents,
                None,
                0,
            ),
            "normalizing source documents",
        )
        .await;
    let normalized =
        reuse::normalize_acquisition(runtime, input, &batch_diff, resolved.acquisition).await?;
    source_progress::normalized(emitter, generation, &normalized.header).await;
    let mut warnings = normalized.header.warnings.clone();
    let mut documents = normalized.data;
    stage.normalized_documents = stage
        .normalized_documents
        .saturating_add(documents.len() as u64);
    stage.pipeline.add_documents(documents.len() as u64);
    if is_final_batch {
        stage.pipeline.finish_documents();
    }
    coordinator
        .checkpoint(
            PipelinePhase::Normalizing,
            stage_counts(
                total,
                stage.normalized_documents,
                total,
                stage.normalized_documents,
                None,
                0,
            ),
            "normalized source documents",
        )
        .await;

    let (candidate_collection, clean_output) =
        finalize_normalized_batch(runtime, input, generation, &mut documents, &enrichments).await?;
    warnings.extend(candidate_collection.warnings);
    let enrichment_graph = take_enrichment_graph_candidates(&mut enrichments);
    let vectorized = vectorize::prepare_embed_publish(
        runtime,
        input,
        documents,
        &enrichment_graph,
        generation,
        collection.clone(),
        emitter,
        coordinator,
        &mut stage.pipeline,
        is_final_batch,
    )
    .await?;

    let enrichment_artifacts = collect_enrichment_outputs(enrichments, &mut warnings);
    Ok(ProcessedBatch {
        vectorized,
        acquisition_artifacts,
        enrichment_artifacts,
        clean_output,
        archive_items,
        artifact_candidates: candidate_collection.candidates,
        warnings,
        reused_item_keys: resolved.reused_item_keys,
        refreshed_manifest_items,
    })
}

async fn finalize_normalized_batch(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    generation: &SourceGenerationId,
    documents: &mut [SourceDocument],
    enrichments: &std::collections::BTreeMap<SourceItemKey, SourceEnrichment>,
) -> anyhow::Result<(
    artifact_candidates::CandidateCollection,
    output::SourceOutput,
)> {
    apply_enrichments(documents, enrichments);
    let candidates =
        artifact_candidates::collect_changed_candidates(input, generation, documents, enrichments)
            .await;
    let clean_output = output::store_clean_outputs(runtime, &input.plan, documents).await?;
    Ok((candidates, clean_output))
}

fn collect_enrichment_outputs(
    enrichments: std::collections::BTreeMap<SourceItemKey, SourceEnrichment>,
    warnings: &mut Vec<SourceWarning>,
) -> Vec<ArtifactRef> {
    let mut artifacts = Vec::new();
    for enrichment in enrichments.into_values() {
        warnings.extend(enrichment.warnings);
        artifacts.extend(enrichment.artifacts);
    }
    artifacts
}

async fn enrich_changed_items(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    emitter: &SourceEventEmitter,
    coordinator: &ProgressCoordinator,
    stage: &mut GenerationStageProgress,
    items: &[AcquiredSourceItem],
    is_final_batch: bool,
) -> anyhow::Result<std::collections::BTreeMap<SourceItemKey, SourceEnrichment>> {
    let total = is_final_batch.then_some(stage.acquired_documents);
    coordinator
        .report(
            emitter,
            PipelinePhase::Enriching,
            stage_counts(total, stage.enriched_items, None, 0, None, 0),
            "enriching acquired source items",
        )
        .await;
    let enrichments = enrich(runtime.enricher.clone(), &input.plan, items).await?;
    stage.enriched_items = stage.enriched_items.saturating_add(items.len() as u64);
    coordinator
        .checkpoint(
            PipelinePhase::Enriching,
            stage_counts(total, stage.enriched_items, None, 0, None, 0),
            "enriched acquired source items",
        )
        .await;
    Ok(enrichments)
}

#[allow(clippy::too_many_arguments)]
async fn publish_created_generation(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    emitter: &SourceEventEmitter,
    lease: &LeaseGuard,
    manifest: SourceManifest,
    diff: SourceManifestDiff,
    generation: SourceGeneration,
    previous: Option<SourceSummary>,
    collection: CollectionSpec,
    vectorized: vectorize::VectorizeResult,
    artifacts: Vec<ArtifactRef>,
    inline: Option<InlineSourceResult>,
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
        vectorized, artifacts, inline,
    )
    .await;
    let release = runtime
        .ledger
        .release_lease(finalizer.lease_id, input.owner_id.to_string())
        .await;
    match (result, release) {
        (Ok(mut counts), Ok(())) => {
            if !counts.warnings.is_empty() {
                super::persist_degraded_summary(runtime, &mut counts).await;
            }
            Ok(counts)
        }
        (Err(err), Ok(())) => Err(err),
        (Ok(mut counts), Err(err)) => {
            counts.warnings.push(post_publish_warning(
                "source.publish.finalizer_release_deferred",
                format!(
                    "generation {} was published, but releasing the publication finalizer failed: {err}",
                    counts.generation.0
                ),
            ));
            super::persist_degraded_summary(runtime, &mut counts).await;
            Ok(counts)
        }
        (Err(err), Err(release_err)) => Err(err.context(format!(
            "source publication finalizer release also failed: {release_err}"
        ))),
    }
}

#[allow(clippy::too_many_arguments)]
async fn publish_created_generation_under_finalizer(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    emitter: &SourceEventEmitter,
    lease: &LeaseGuard,
    manifest: SourceManifest,
    diff: SourceManifestDiff,
    generation: SourceGeneration,
    previous: Option<SourceSummary>,
    collection: CollectionSpec,
    mut vectorized: vectorize::VectorizeResult,
    artifacts: Vec<ArtifactRef>,
    inline: Option<InlineSourceResult>,
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
    let publish_outcome = publish::publish(
        runtime,
        input,
        &collection,
        &generation,
        &diff,
        input.plan.request.embed,
        vectorized.points_written,
    )
    .await?;
    vectorized.warnings.extend(publish_outcome.warnings);
    let published = publish_outcome.generation;
    if let Err(error) = runtime
        .ledger
        .publish_document_statuses(
            manifest.source_id.clone(),
            published.generation.clone(),
            timestamp(),
        )
        .await
    {
        vectorized.warnings.push(post_publish_warning(
            "source.publish.document_status_deferred",
            format!(
                "generation {} was published, but persisting published document statuses failed: {error}",
                published.generation.0
            ),
        ));
    }
    let counts = terminal_source_counts(previous.as_ref(), &manifest, &diff, &vectorized);
    if let Err(error) = runtime
        .ledger
        .upsert_source(metadata::source_summary(
            input,
            super::successful_status(&vectorized.warnings),
            counts,
            previous.as_ref(),
        ))
        .await
    {
        vectorized.warnings.push(post_publish_warning(
            "source.publish.summary_deferred",
            format!(
                "generation {} was published, but persisting the source summary failed: {error}",
                published.generation.0
            ),
        ));
    }
    source_progress::published(
        emitter,
        &published.generation,
        manifest.items.len() as u64,
        &vectorized.warnings,
        vectorized.documents_prepared,
        vectorized.chunks_prepared,
    )
    .await;
    let items_discovered = manifest.items.len() as u64;
    let source_id = manifest.source_id.clone();
    Ok(IndexCounts {
        job_id: input.plan.job_id,
        source_id,
        generation: published.generation,
        items_discovered,
        documents_prepared: vectorized.documents_prepared,
        chunks_prepared: vectorized.chunks_prepared,
        vector_points_written: vectorized.points_written,
        removed: diff.counts.removed,
        published_manifest: Some(manifest),
        graph_candidates: vectorized.graph_candidates,
        warnings: vectorized.warnings,
        artifacts,
        inline,
    })
}

fn post_publish_warning(code: &str, message: String) -> SourceWarning {
    SourceWarning {
        code: code.to_string(),
        severity: Severity::Warning,
        message,
        source_item_key: None,
        retryable: true,
    }
}
