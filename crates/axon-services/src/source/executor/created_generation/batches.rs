use super::*;
use crate::source::executor::generation_work::PreparedBatchSideEffects;
use std::future::Future;

#[path = "batches/scheduled.rs"]
mod scheduled;

struct ChangedBatch {
    diff: SourceManifestDiff,
    is_final: bool,
}

struct AcquiredChangedBatch {
    batch: ChangedBatch,
    acquisition: SourceAcquisition,
    items: u64,
    documents: u64,
}

async fn process_and_acquire_next<P, A, Process, Acquire>(
    adapter: &dyn axon_adapters::SourceAdapter,
    process: Process,
    acquire: Acquire,
) -> (anyhow::Result<P>, Option<anyhow::Result<A>>)
where
    Process: Future<Output = anyhow::Result<P>>,
    Acquire: Future<Output = anyhow::Result<A>>,
{
    if adapter.supports_acquisition_prefetch() {
        let (processed, acquired) = tokio::join!(process, acquire);
        (processed, Some(acquired))
    } else {
        match process.await {
            Ok(processed) => (Ok(processed), Some(acquire.await)),
            Err(error) => (Err(error), None),
        }
    }
}

fn resolve_batch_step<P, A>(
    processed: anyhow::Result<P>,
    prefetched: Option<anyhow::Result<A>>,
    mut absorb: impl FnMut(P) -> anyhow::Result<()>,
) -> anyhow::Result<A> {
    match (processed, prefetched) {
        (Ok(processed), Some(prefetched)) => {
            // Current-batch accounting is durable even when the speculative
            // next acquisition fails. This preserves the completed work in
            // the eventual failure summary instead of silently dropping it.
            absorb(processed)?;
            prefetched
        }
        (Ok(processed), None) => {
            absorb(processed)?;
            anyhow::bail!("next acquisition was not attempted after successful batch processing")
        }
        (Err(primary), Some(Err(secondary))) => {
            tracing::error!(
                primary_error = %primary,
                secondary_error = %secondary,
                "source batch processing and overlapped acquisition both failed"
            );
            Err(primary.context(format!(
                "overlapped next-batch acquisition also failed: {secondary:#}"
            )))
        }
        (Err(primary), Some(Ok(_)) | None) => Err(primary),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn process_generation_batches(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    emitter: &SourceEventEmitter,
    generation: &SourceGenerationId,
    collection: &CollectionSpec,
    diff: &SourceManifestDiff,
    archive_requested: bool,
    changed_total: u64,
    coordinator: &ProgressCoordinator,
    stage: &mut GenerationStageProgress,
    accumulated: &mut GenerationAccumulator,
    artifact_cleanup: &mut ArtifactCleanupGuard,
) -> anyhow::Result<()> {
    if scheduled::enabled() {
        return scheduled::process(
            runtime,
            input,
            emitter,
            generation,
            collection,
            diff,
            archive_requested,
            changed_total,
            coordinator,
            stage,
            accumulated,
            artifact_cleanup,
        )
        .await;
    }
    let acquire_batch_size = acquire_batch_size();
    let first_batch_size = first_acquire_batch_size(acquire_batch_size);
    let changed = usize::try_from(changed_total).unwrap_or(usize::MAX);
    let batch_count = if changed <= first_batch_size {
        usize::from(changed > 0)
    } else {
        1 + (changed - first_batch_size).div_ceil(acquire_batch_size)
    };
    let mut batches = batch_changed_diff_ramped(diff, first_batch_size, acquire_batch_size)
        .enumerate()
        .map(|(index, diff)| ChangedBatch {
            diff,
            is_final: index + 1 == batch_count,
        });
    let Some(first) = batches.next() else {
        // Removal-only and failed-only diffs pass `manifest_has_changes` but
        // yield no added/modified acquisition batches. Skip acquisition and
        // fall through so finalization still publishes the removals and
        // retires the previous generation instead of failing the run
        // (2026-08-23 adversarial pipeline review, H1).
        return Ok(());
    };
    let mut acquired = acquire_changed_batch(
        input,
        first,
        changed_total,
        stage.acquired_items,
        stage.acquired_documents,
        coordinator,
        true,
    )
    .await?;
    loop {
        stage.acquired_items = stage.acquired_items.saturating_add(acquired.items);
        stage.acquired_documents = stage.acquired_documents.saturating_add(acquired.documents);
        let next = batches.next();
        if let Some(next_batch) = next {
            let next_acquisition = acquire_changed_batch(
                input,
                next_batch,
                changed_total,
                stage.acquired_items,
                stage.acquired_documents,
                coordinator,
                !input.adapter.supports_acquisition_prefetch(),
            );
            let (processed, prefetched) = process_and_acquire_next(
                input.adapter,
                process_acquired_batch(
                    runtime,
                    input,
                    emitter,
                    generation,
                    collection,
                    acquired,
                    archive_requested,
                    coordinator,
                    stage,
                    artifact_cleanup,
                ),
                next_acquisition,
            )
            .await;
            if let Some(Ok(prefetched)) = prefetched.as_ref() {
                artifact_cleanup.track(&prefetched.acquisition.artifacts);
            }
            acquired = resolve_batch_step(processed, prefetched, |processed| {
                let (side_effects, vectorized) = processed;
                accumulated.absorb_pretracked_side_effects(side_effects)?;
                accumulated.absorb_vectorized(vectorized);
                Ok(())
            })?;
            continue;
        }

        let processed = process_acquired_batch(
            runtime,
            input,
            emitter,
            generation,
            collection,
            acquired,
            archive_requested,
            coordinator,
            stage,
            artifact_cleanup,
        )
        .await?;
        let (side_effects, vectorized) = processed;
        accumulated.absorb_pretracked_side_effects(side_effects)?;
        accumulated.absorb_vectorized(vectorized);
        break;
    }
    Ok(())
}

#[cfg(test)]
#[path = "batches_tests.rs"]
mod tests;

async fn acquire_changed_batch(
    input: &SourcePipelineInput<'_>,
    batch: ChangedBatch,
    changed_total: u64,
    acquired_items: u64,
    acquired_documents: u64,
    coordinator: &ProgressCoordinator,
    publish_fetching_phase: bool,
) -> anyhow::Result<AcquiredChangedBatch> {
    let items = batch
        .diff
        .added
        .len()
        .saturating_add(batch.diff.modified.len()) as u64;
    let reporter = coordinator.acquisition_batch(
        changed_total,
        items,
        acquired_items,
        acquired_documents,
        publish_fetching_phase,
    );
    let acquisition = input
        .adapter
        .acquire_with_progress(&input.plan, &batch.diff, Some(&reporter))
        .await?;
    let documents = acquisition.fetched_items.len() as u64;
    reporter.complete(documents).await;
    Ok(AcquiredChangedBatch {
        batch,
        acquisition,
        items,
        documents,
    })
}

struct PreparedAcquiredComponents {
    documents: Vec<SourceDocument>,
    enrichment_graph: std::collections::BTreeMap<SourceItemKey, Vec<GraphCandidate>>,
    side_effects: PreparedBatchSideEffects,
    is_final: bool,
}

#[allow(clippy::too_many_arguments)]
async fn prepare_acquired_components(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    emitter: &SourceEventEmitter,
    generation: &SourceGenerationId,
    acquired: AcquiredChangedBatch,
    archive_requested: bool,
    coordinator: &ProgressCoordinator,
    stage: &mut GenerationStageProgress,
    artifact_cleanup: &mut ArtifactCleanupGuard,
) -> anyhow::Result<PreparedAcquiredComponents> {
    let AcquiredChangedBatch {
        batch, acquisition, ..
    } = acquired;
    let ChangedBatch {
        diff: batch_diff,
        is_final: is_final_batch,
    } = batch;
    source_progress::acquired(emitter, &acquisition).await;

    let resolved = reuse::resolve_acquisition(runtime, input, &batch_diff, acquisition).await?;
    // Track in-batch artifacts with the cleanup guard as they are produced,
    // not only after batch success in `GenerationAccumulator::absorb` — a
    // later in-batch failure would otherwise orphan them (2026-08-23
    // adversarial pipeline review, low: in-batch artifact tracking).
    artifact_cleanup.track(&resolved.acquisition.artifacts);
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
    for enrichment in enrichments.values() {
        artifact_cleanup.track(&enrichment.artifacts);
    }

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
    artifact_cleanup.track(&clean_output.artifacts);
    warnings.extend(candidate_collection.warnings);
    let enrichment_graph = take_enrichment_graph_candidates(&mut enrichments);
    let enrichment_artifacts = collect_enrichment_outputs(enrichments, &mut warnings);
    Ok(PreparedAcquiredComponents {
        documents,
        enrichment_graph,
        side_effects: PreparedBatchSideEffects {
            acquisition_artifacts,
            enrichment_artifacts,
            clean_output,
            archive_items,
            artifact_candidates: candidate_collection.candidates,
            warnings,
            reused_item_keys: resolved.reused_item_keys,
            refreshed_manifest_items,
        },
        is_final: is_final_batch,
    })
}

#[allow(clippy::too_many_arguments)]
async fn process_acquired_batch(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    emitter: &SourceEventEmitter,
    generation: &SourceGenerationId,
    collection: &CollectionSpec,
    acquired: AcquiredChangedBatch,
    archive_requested: bool,
    coordinator: &ProgressCoordinator,
    stage: &mut GenerationStageProgress,
    artifact_cleanup: &mut ArtifactCleanupGuard,
) -> anyhow::Result<(PreparedBatchSideEffects, vectorize::VectorizeResult)> {
    let components = prepare_acquired_components(
        runtime,
        input,
        emitter,
        generation,
        acquired,
        archive_requested,
        coordinator,
        stage,
        artifact_cleanup,
    )
    .await?;
    let vectorized = vectorize::prepare_embed_publish(
        runtime,
        input,
        components.documents,
        &components.enrichment_graph,
        generation,
        collection.clone(),
        emitter,
        coordinator,
        &mut stage.pipeline,
        components.is_final,
    )
    .await?;
    Ok((components.side_effects, vectorized))
}
