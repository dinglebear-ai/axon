//! Run + publish one already-created source generation.
//!
//! Split out of `executor.rs` to stay under the monolith line cap; owns the
//! streaming acquire/normalize/prepare/embed/publish loop
//! (`run_created_generation`) and the terminal ledger/vector-store publish
//! step (`publish_created_generation`).

use axon_api::source::*;

use super::helpers::*;
use super::{
    ACQUIRE_BATCH_SIZE, SOURCE_LEASE_TTL_SECONDS, SourceEventEmitter, SourcePipelineInput,
    metadata, publish, reuse, vectorize,
};
use crate::context::TargetLocalSourceRuntime;
use crate::reserved_call::{self, ArtifactCleanupGuard, ProviderCallContext};
use crate::source::output::{self, SourceOutput};
use crate::source::progress;
use crate::source::result_map::IndexCounts;

struct ProcessedBatch {
    vectorized: vectorize::VectorizeResult,
    acquisition_artifacts: Vec<ArtifactRef>,
    enrichment_artifacts: Vec<ArtifactRef>,
    clean_output: SourceOutput,
    archive_items: Vec<AcquiredSourceItem>,
    warnings: Vec<SourceWarning>,
    reused_item_keys: Vec<SourceItemKey>,
}

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
) -> anyhow::Result<IndexCounts> {
    let collection = collection_spec(input.collection, runtime.embedding_dimensions);
    let mut artifact_cleanup = ArtifactCleanupGuard::new(
        runtime,
        generation.source_id.clone(),
        generation.generation.clone(),
    );
    if input.plan.request.embed {
        reserved_call::ensure_collection(
            runtime,
            ProviderCallContext::for_phase(
                input.plan.job_id,
                input.execution.attempt,
                PipelinePhase::Upserting,
                input.execution.priority,
                format!("ensure-collection:{}", collection.collection),
            ),
            collection.clone(),
        )
        .await?;
    }
    let mut vectorized = vectorize::VectorizeResult::default();
    let mut artifacts = Vec::new();
    let mut output = SourceOutput::default();
    let mut archive_items = Vec::new();
    let archive_requested = input.adapter.wants_archive(&input.plan);
    let mut warnings = Vec::new();
    let mut reused_item_keys = Vec::new();
    for batch_diff in batch_changed_diff(&diff, ACQUIRE_BATCH_SIZE) {
        let batch = process_changed_batch(
            runtime,
            input,
            emitter,
            &generation.generation,
            &collection,
            batch_diff,
            archive_requested,
        )
        .await?;
        artifact_cleanup.track(&batch.acquisition_artifacts);
        artifact_cleanup.track(&batch.enrichment_artifacts);
        artifact_cleanup.track(&batch.clean_output.artifacts);
        artifacts.extend(batch.acquisition_artifacts);
        artifacts.extend(batch.enrichment_artifacts);
        archive_items.extend(batch.archive_items);
        warnings.extend(batch.warnings);
        reused_item_keys.extend(batch.reused_item_keys);
        output.merge(batch.clean_output);
        vectorize::merge_vectorize_result(&mut vectorized, batch.vectorized);
    }
    vectorized.warnings.splice(0..0, warnings);
    let archive_output =
        output::store_adapter_archive(runtime, input.adapter, &input.plan, &archive_items).await?;
    artifact_cleanup.track(&archive_output.artifacts);
    output.merge(archive_output);
    artifacts.extend(output.artifacts.clone());
    let diff = reuse::apply_reused_items(&diff, &reused_item_keys);
    output::record_artifacts_on_manifest(
        runtime.ledger.as_ref(),
        &mut manifest,
        &diff,
        &output.artifact_index,
    )
    .await?;

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
        vectorized,
        artifacts,
        output.inline,
    )
    .await;
    if result.is_ok() {
        artifact_cleanup.disarm();
    }
    result
}

async fn process_changed_batch(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    emitter: &SourceEventEmitter,
    generation: &SourceGenerationId,
    collection: &CollectionSpec,
    batch_diff: SourceManifestDiff,
    archive_requested: bool,
) -> anyhow::Result<ProcessedBatch> {
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
    let resolved = reuse::resolve_acquisition(runtime, input, &batch_diff, acquisition).await?;
    let acquisition_artifacts = resolved.acquisition.artifacts.clone();
    let archive_items = if archive_requested {
        resolved.acquisition.fetched_items.clone()
    } else {
        Vec::new()
    };
    let enrichments = enrich(
        runtime.enricher.clone(),
        &input.plan,
        &resolved.acquisition.fetched_items,
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
    let normalized =
        reuse::normalize_acquisition(runtime, input, &batch_diff, resolved.acquisition).await?;
    progress::normalized(emitter, generation, &normalized.header).await;
    let mut warnings = normalized.header.warnings.clone();
    let mut documents = normalized.data;
    apply_enrichments(&mut documents, &enrichments);
    let clean_output = output::store_clean_outputs(runtime, &input.plan, &documents).await?;
    let enrichment_graph = enrichment_graph_candidates(&enrichments);
    record_running_phase(
        runtime,
        input,
        emitter,
        PipelinePhase::Preparing,
        "preparing source documents",
    )
    .await?;
    let vectorized = vectorize::prepare_embed_publish(
        runtime,
        input,
        documents,
        &enrichment_graph,
        generation,
        collection.clone(),
        emitter,
    )
    .await?;
    let mut enrichment_artifacts = Vec::new();
    for enrichment in enrichments.values() {
        warnings.extend(enrichment.warnings.clone());
        enrichment_artifacts.extend(enrichment.artifacts.clone());
    }
    Ok(ProcessedBatch {
        vectorized,
        acquisition_artifacts,
        enrichment_artifacts,
        clean_output,
        archive_items,
        warnings,
        reused_item_keys: resolved.reused_item_keys,
    })
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
    let published_statuses = vectorized
        .document_statuses
        .iter()
        .map(publish::published_status)
        .collect::<Vec<_>>();
    if let Err(error) =
        vectorize::write_document_statuses(runtime.ledger.as_ref(), &published_statuses).await
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
        items_discovered: manifest.items.len() as u64,
        documents_prepared: vectorized.documents_prepared,
        chunks_prepared: vectorized.chunks_prepared,
        vector_points_written: vectorized.points_written,
        removed: diff.counts.removed,
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
