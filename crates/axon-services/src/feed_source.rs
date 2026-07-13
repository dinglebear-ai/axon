mod feed_source_adapter;
mod feed_source_job;
mod feed_source_progress;
mod feed_source_publish;
mod feed_source_vectorize;

use std::path::PathBuf;

use axon_api::source::*;
use axon_embedding::provider::EmbeddingProvider;
use axon_embedding::reservation::ProviderReservationManager;
use axon_ledger::store::LedgerStore;
use axon_vectors::store::VectorStore;
use std::sync::Arc;

#[cfg(test)]
pub(super) use self::feed_source_adapter::feed_source_id;
use self::feed_source_adapter::{
    FeedAdapterRun, collection_spec, discover_manifest, resolve_adapter_run, source_summary,
    timestamp,
};
pub use self::feed_source_job::index_feed_source_with_job;
use self::feed_source_progress::{
    FeedSourceProgress, ensure_providers_ready, phase_for_api_error, record_progress,
    record_progress_error,
};
use self::feed_source_publish::{
    complete_generation, completed_source_summary, ensure_lease_before_publish,
    mark_completed_generation_failed, mark_vectors_for_completed_generation,
    publish_generation_and_rollback_vectors, rollback_new_generation_vectors,
};
use self::feed_source_vectorize::{
    VectorizeResult, publish_document_status, vectorize_changed_documents,
};

const FEED_ADAPTER_VERSION: &str = env!("CARGO_PKG_VERSION");
const FEED_LEASE_TTL_SECONDS: u64 = 30 * 60;

#[derive(Debug, Clone)]
pub struct FeedSourceIndexInput {
    /// Path to a prepared feed document on disk (raw feed bytes), fetched by
    /// the caller — mirrors the `git` adapter's `repo_root`. See
    /// `axon_adapters::feed::feed_path`.
    pub feed_path: PathBuf,
    pub collection: String,
    pub owner_id: String,
    pub job_id: JobId,
    pub embedding_provider_id: ProviderId,
    pub vector_provider_id: ProviderId,
    pub embedding_model: String,
    pub embedding_dimensions: u32,
    pub embedding_reservations: Option<Arc<ProviderReservationManager>>,
    pub vector_reservations: Option<Arc<ProviderReservationManager>>,
    pub auth_snapshot: Option<AuthSnapshot>,
    /// `SourceRequest.embed` (source-pipeline.md, Validation Checklist:
    /// "`embed=false` never writes vectors"). When `false`, feed entries are
    /// still discovered/normalized/prepared but no embedding provider or
    /// vector store call is made.
    pub embed: bool,
    /// `SourceRequest.limits.max_items` (source-pipeline.md limits contract).
    /// Natural mapping for feeds: caps the number of feed entries considered
    /// from the discovered manifest, applied before diffing so uncapped
    /// entries beyond the limit are neither diffed nor vectorized. `None`
    /// means uncapped.
    pub max_items: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FeedSourceIndexOutput {
    pub job_id: JobId,
    pub source_id: SourceId,
    pub generation: SourceGenerationId,
    pub documents_prepared: u64,
    pub chunks_prepared: u64,
    pub vector_points_written: u64,
    pub removed_entries: u64,
    /// Parser-produced graph candidates from every prepared document in this
    /// generation, carried up for the `graphing` stage
    /// (`source::graph::write_baseline_graph`) to write. Empty on the
    /// unchanged-refresh path, since it prepares no documents.
    pub graph_candidates: Vec<GraphCandidate>,
}

#[cfg(test)]
pub async fn index_feed_source(
    input: FeedSourceIndexInput,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
) -> anyhow::Result<FeedSourceIndexOutput> {
    index_feed_source_with_progress(input, ledger, embedding_provider, vector_store, None).await
}

async fn index_feed_source_with_progress(
    input: FeedSourceIndexInput,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
    progress: Option<&dyn FeedSourceProgress>,
) -> anyhow::Result<FeedSourceIndexOutput> {
    let run = resolve_adapter_run(&input).await?;

    let previous_source = ledger.get_source(run.source_id.clone()).await?;
    ledger.upsert_source(source_summary(&input, &run)).await?;
    let lease = ledger
        .acquire_lease(LeaseRequest {
            lease_key: format!("source:{}", run.source_id.0),
            owner_id: input.owner_id.clone(),
            ttl_seconds: FEED_LEASE_TTL_SECONDS,
            job_id: Some(input.job_id),
            metadata: MetadataMap::new(),
        })
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "feed source refresh already running for {}",
                run.source_id.0
            )
        })?;

    let result = index_feed_source_with_lease(
        &input,
        ledger,
        embedding_provider,
        vector_store,
        progress,
        previous_source,
        run,
        &lease,
    )
    .await;

    let release = ledger.release_lease(lease.lease_id, input.owner_id).await;
    match (result, release) {
        (Ok(output), Ok(())) => Ok(output),
        (Err(err), Ok(())) => Err(err),
        (Ok(_), Err(err)) => {
            Err(anyhow::Error::new(err).context("failed to release feed source lease"))
        }
        (Err(err), Err(release_err)) => Err(err.context(format!(
            "additionally failed to release feed source lease: {release_err}"
        ))),
    }
}

async fn index_feed_source_with_lease(
    input: &FeedSourceIndexInput,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
    progress: Option<&dyn FeedSourceProgress>,
    previous_source: Option<SourceSummary>,
    run: FeedAdapterRun,
    lease: &LeaseGuard,
) -> anyhow::Result<FeedSourceIndexOutput> {
    let mut manifest = discover_manifest(&run).await?;
    apply_max_items_limit(&mut manifest, input.max_items);
    record_progress(progress, PipelinePhase::Discovering, None).await?;
    let diff = ledger.diff_manifest(manifest.clone()).await?;
    record_progress(progress, PipelinePhase::Diffing, None).await?;
    if let Some(output) =
        unchanged_refresh_output(input, ledger, previous_source, &run, &manifest, &diff).await?
    {
        return Ok(output);
    }

    if let Err(err) = ensure_providers_ready(embedding_provider, vector_store).await {
        record_progress_error(progress, phase_for_api_error(&err), &err).await?;
        return Err(anyhow::Error::new(err));
    }
    let source_id = run.source_id.clone();
    let generation = ledger.create_generation(source_id.clone()).await?;
    manifest.generation = generation.generation.clone();
    ledger.put_manifest(manifest.clone()).await?;
    record_progress(progress, PipelinePhase::Preparing, None).await?;
    let collection = collection_spec(&input.collection, input.embedding_dimensions);
    vector_store.ensure_collection(collection.clone()).await?;
    let vectorized = vectorize_generation_or_fail(
        input,
        &run,
        &diff,
        &source_id,
        &generation,
        ledger,
        embedding_provider,
        vector_store,
        progress,
        &collection,
    )
    .await?;
    ensure_lease_before_publish_or_rollback_vectors(
        ledger,
        input,
        lease,
        vector_store,
        &collection,
        &source_id,
        &generation,
    )
    .await?;
    let completed =
        complete_generation(ledger, generation, &diff, manifest.items.len() as u64).await?;
    let publish_stats = match mark_vectors_for_completed_generation(
        vector_store,
        &collection,
        &source_id,
        &completed,
        &diff,
        progress,
    )
    .await
    {
        Ok(stats) => stats,
        Err(err) => {
            if let Err(fail_err) = mark_completed_generation_failed(ledger, completed.clone()).await
            {
                return Err(err.context(format!(
                    "also failed to mark source generation failed: {fail_err}"
                )));
            }
            return Err(err);
        }
    };
    let published =
        publish_generation_and_rollback_vectors(ledger, vector_store, &collection, &completed)
            .await?;
    for status in &vectorized.document_statuses {
        ledger
            .update_document_status(publish_document_status(status))
            .await?;
    }
    ledger
        .upsert_source(completed_source_summary(
            input,
            &run,
            manifest.items.len() as u64,
            &diff,
            &publish_stats,
        ))
        .await?;
    record_progress(progress, PipelinePhase::Publishing, None).await?;
    record_progress(progress, PipelinePhase::Cleaning, None).await?;

    Ok(FeedSourceIndexOutput {
        job_id: input.job_id,
        source_id,
        generation: published.generation,
        documents_prepared: vectorized.stats.documents_prepared,
        chunks_prepared: vectorized.stats.chunks_prepared,
        vector_points_written: publish_stats.total_points_written(),
        removed_entries: diff.counts.removed,
        graph_candidates: vectorized.graph_candidates,
    })
}

async fn unchanged_refresh_output(
    input: &FeedSourceIndexInput,
    ledger: &dyn LedgerStore,
    previous_source: Option<SourceSummary>,
    run: &FeedAdapterRun,
    manifest: &SourceManifest,
    diff: &SourceManifestDiff,
) -> anyhow::Result<Option<FeedSourceIndexOutput>> {
    if manifest_diff_has_changes(diff) {
        return Ok(None);
    }
    let Some(committed_generation) = diff.previous_generation.clone() else {
        return Ok(None);
    };
    ledger
        .upsert_source(unchanged_source_summary(
            input,
            run,
            previous_source,
            manifest.items.len() as u64,
        ))
        .await?;
    Ok(Some(FeedSourceIndexOutput {
        job_id: input.job_id,
        source_id: run.source_id.clone(),
        generation: committed_generation,
        documents_prepared: 0,
        chunks_prepared: 0,
        vector_points_written: 0,
        removed_entries: 0,
        graph_candidates: Vec::new(),
    }))
}

async fn ensure_lease_before_publish_or_rollback_vectors(
    ledger: &dyn LedgerStore,
    input: &FeedSourceIndexInput,
    lease: &LeaseGuard,
    vector_store: &dyn VectorStore,
    collection: &CollectionSpec,
    source_id: &SourceId,
    generation: &SourceGeneration,
) -> anyhow::Result<()> {
    if let Err(err) = ensure_lease_before_publish(ledger, input, lease, generation.clone()).await {
        if let Err(rollback_err) =
            rollback_new_generation_vectors(vector_store, collection, source_id, generation).await
        {
            return Err(err.context(format!(
                "also failed to rollback feed source generation vectors: {rollback_err}"
            )));
        }
        return Err(err);
    }
    Ok(())
}

async fn vectorize_generation_or_fail(
    input: &FeedSourceIndexInput,
    run: &FeedAdapterRun,
    diff: &SourceManifestDiff,
    source_id: &SourceId,
    generation: &SourceGeneration,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
    progress: Option<&dyn FeedSourceProgress>,
    collection: &CollectionSpec,
) -> anyhow::Result<VectorizeResult> {
    match vectorize_changed_documents(
        input,
        run,
        diff,
        &generation.generation,
        ledger,
        embedding_provider,
        vector_store,
        progress,
        collection.clone(),
    )
    .await
    {
        Ok(vectorized) => Ok(vectorized),
        Err(err) => {
            fail_generation_after_vectorize_error(
                ledger,
                vector_store,
                collection,
                source_id,
                generation,
                err,
            )
            .await
        }
    }
}

async fn fail_generation_after_vectorize_error(
    ledger: &dyn LedgerStore,
    vector_store: &dyn VectorStore,
    collection: &CollectionSpec,
    source_id: &SourceId,
    generation: &SourceGeneration,
    err: anyhow::Error,
) -> anyhow::Result<VectorizeResult> {
    if let Err(rollback_err) =
        rollback_new_generation_vectors(vector_store, collection, source_id, generation).await
    {
        if let Err(fail_err) = mark_completed_generation_failed(ledger, generation.clone()).await {
            return Err(err.context(format!(
                "also failed to rollback partially written vectors: {rollback_err}; also failed to mark source generation failed: {fail_err}"
            )));
        }
        return Err(err.context(format!(
            "also failed to rollback partially written vectors: {rollback_err}"
        )));
    }
    if let Err(fail_err) = mark_completed_generation_failed(ledger, generation.clone()).await {
        return Err(err.context(format!(
            "also failed to mark source generation failed: {fail_err}"
        )));
    }
    Err(err)
}

fn unchanged_source_summary(
    input: &FeedSourceIndexInput,
    run: &FeedAdapterRun,
    previous: Option<SourceSummary>,
    item_count: u64,
) -> SourceSummary {
    if let Some(mut summary) = previous {
        summary.status = LifecycleStatus::Completed;
        summary.counts.items_total = item_count;
        summary.counts.items_changed = 0;
        summary.updated_at = timestamp();
        return summary;
    }

    let mut summary = source_summary(input, run);
    summary.status = LifecycleStatus::Completed;
    summary.counts.items_total = item_count;
    summary.updated_at = timestamp();
    summary
}

/// Cap the discovered feed manifest to `max_items` entries
/// (`SourceRequest.limits.max_items`). Applied before diffing so entries
/// beyond the cap are never diffed, prepared, or vectorized. `None` leaves
/// the manifest untouched.
fn apply_max_items_limit(manifest: &mut SourceManifest, max_items: Option<u64>) {
    if let Some(max_items) = max_items {
        let max_items = usize::try_from(max_items).unwrap_or(usize::MAX);
        manifest.items.truncate(max_items);
    }
}

fn manifest_diff_has_changes(diff: &SourceManifestDiff) -> bool {
    diff.counts.added > 0
        || diff.counts.modified > 0
        || diff.counts.removed > 0
        || diff.counts.skipped > 0
        || diff.counts.failed > 0
}

#[cfg(test)]
#[path = "feed_source_failure_tests.rs"]
mod failure_tests;
#[cfg(test)]
#[path = "feed_source_refresh_tests.rs"]
mod refresh_tests;
#[cfg(test)]
#[path = "feed_source_tests.rs"]
mod tests;
