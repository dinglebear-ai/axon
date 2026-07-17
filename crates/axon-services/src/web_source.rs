mod artifacts;
mod job_execution;
mod publish;
mod reuse;
mod run;
mod vector_publish;
mod vectorize;
mod vectorize_helpers;
mod web_source_job;

pub(crate) use self::job_execution::WebSourceJobExecution;
pub(crate) use self::reuse::InProcessWebDocumentCache;
pub(crate) use self::web_source_job::index_web_source_with_execution;
pub use self::web_source_job::index_web_source_with_job;
pub(crate) use self::web_source_job::job_create_request as web_source_job_create_request;

use std::sync::Arc;

use axon_adapters::boundary::{FetchProvider, RenderProvider};
use axon_adapters::{SourceAdapter, web::WebSourceAdapter};
use axon_api::source::*;
use axon_core::boundary::ArtifactStore;
use axon_core::boundary::DocumentCache;
use axon_embedding::provider::EmbeddingProvider;
use axon_jobs::boundary::JobStore;
use axon_ledger::store::LedgerStore;
use axon_vectors::store::VectorStore;

use self::artifacts::{cleanup_artifacts_after_error, record_artifacts_on_manifest};
use self::publish::{
    GenerationDocumentCounts, complete_generation, completed_source_summary,
    ensure_lease_before_publish, fail_generation, publish_generation_without_vectors,
};
use self::run::apply_reused_item_keys;
use self::run::{
    WebAdapterRun, overlay_previous_web_etags, resolve_web_run, source_summary,
    unchanged_refresh_output,
};
use self::vector_publish::{VectorGenerationRequest, publish_vector_generation};
use self::vectorize::{prepare_changed_documents_without_vectors, published_status};

pub(super) const WEB_LEASE_TTL_SECONDS: u64 = 30 * 60;

#[cfg(test)]
#[path = "source_web_304_reuse_tests.rs"]
mod source_web_304_reuse_tests;
#[cfg(test)]
#[path = "source_web_artifacts_tests.rs"]
mod source_web_artifacts_tests;
#[cfg(test)]
#[path = "source_web_events_tests.rs"]
mod source_web_events_tests;

/// Real-acquisition (issue #298 Wave 1b) input for `WebSourceAdapter`: no more
/// `manifest_path`/`markdown_root` disk handoff from a `crawl_for_source`
/// pre-pass — `fetch_provider`/`render_provider` are threaded straight into
/// the adapter, and `crawl_options` carries the (already-validated) web
/// adapter option set (`render_mode`, `max_pages`, `max_depth`,
/// `url_whitelist`, ...) that `resolve_web_run` folds into the routed
/// `SourcePlan`.
///
/// Does not derive `Debug` — `Arc<dyn FetchProvider>`/`Arc<dyn RenderProvider>`
/// are trait objects with no `Debug` bound.
#[derive(Clone)]
pub struct WebSourceIndexInput {
    pub source: String,
    pub scope: SourceScope,
    pub map_urls: Vec<String>,
    pub crawl_options: MetadataMap,
    pub output: OutputPolicy,
    pub collection: String,
    pub owner_id: String,
    pub job_id: JobId,
    pub embedding_provider_id: ProviderId,
    pub vector_provider_id: ProviderId,
    pub embedding_model: String,
    pub embedding_dimensions: u32,
    pub auth_snapshot: Option<AuthSnapshot>,
    pub attempt: u32,
    /// `SourceRequest.embed` (source-pipeline.md, Validation Checklist:
    /// "`embed=false` never writes vectors"). When `false`, discovery,
    /// acquire, normalize, prepare, ledger publish, and graph-candidate
    /// collection still run; only collection creation, embedding, and vector
    /// upsert are skipped.
    pub embed: bool,
    pub fetch_provider: Arc<dyn FetchProvider>,
    pub render_provider: Arc<dyn RenderProvider>,
    pub artifact_store: Arc<dyn ArtifactStore>,
    pub document_cache: Arc<dyn DocumentCache>,
    pub event_store: Option<Arc<dyn JobStore>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WebSourceIndexOutput {
    pub job_id: JobId,
    pub source_id: SourceId,
    pub generation: SourceGenerationId,
    pub documents_prepared: u64,
    pub chunks_prepared: u64,
    pub vector_points_written: u64,
    pub removed_pages: u64,
    /// Parser-produced graph candidates from every prepared document in this
    /// generation, carried up for the `graphing` stage
    /// (`source::graph::write_baseline_graph`) to write. Empty on the
    /// unchanged-refresh and map-only paths, since neither prepares documents.
    pub graph_candidates: Vec<GraphCandidate>,
    pub warnings: Vec<SourceWarning>,
    pub artifacts: Vec<ArtifactRef>,
    pub inline: Option<InlineSourceResult>,
}

pub async fn index_web_source(
    input: WebSourceIndexInput,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
) -> anyhow::Result<WebSourceIndexOutput> {
    let run = resolve_web_run(&input)?;
    let previous_source = ledger.get_source(run.source_id.clone()).await?;
    ledger.upsert_source(source_summary(&input, &run)).await?;
    let lease = ledger
        .acquire_lease(LeaseRequest {
            lease_key: format!("source:{}", run.source_id.0),
            owner_id: input.owner_id.clone(),
            ttl_seconds: WEB_LEASE_TTL_SECONDS,
            job_id: Some(input.job_id),
            metadata: MetadataMap::new(),
        })
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!("web source refresh already running for {}", run.source_id.0)
        })?;
    let result = index_web_source_with_lease(
        &input,
        ledger,
        embedding_provider,
        vector_store,
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
            Err(anyhow::Error::new(err).context("failed to release web source lease"))
        }
        (Err(err), Err(release_err)) => Err(err.context(format!(
            "additionally failed to release web source lease: {release_err}"
        ))),
    }
}

async fn index_web_source_with_lease(
    input: &WebSourceIndexInput,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
    previous_source: Option<SourceSummary>,
    run: WebAdapterRun,
    lease: &LeaseGuard,
) -> anyhow::Result<WebSourceIndexOutput> {
    let events = crate::source::events::SourceEventEmitter::for_web(
        input.event_store.clone(),
        input.job_id,
        input.scope,
    )
    .with_source(run.source_id.clone(), run.canonical_uri.clone())
    .with_attempt(input.attempt);
    let result = run_web_pipeline(
        input,
        ledger,
        embedding_provider,
        vector_store,
        previous_source,
        run,
        lease,
        &events,
    )
    .await;
    if let Err(error) = &result {
        crate::source::progress::pipeline_failed(&events, error).await;
    }
    result
}

#[allow(clippy::too_many_arguments)]
async fn run_web_pipeline(
    input: &WebSourceIndexInput,
    ledger: &dyn LedgerStore,
    embedding_provider: &dyn EmbeddingProvider,
    vector_store: &dyn VectorStore,
    previous_source: Option<SourceSummary>,
    run: WebAdapterRun,
    lease: &LeaseGuard,
    events: &crate::source::events::SourceEventEmitter,
) -> anyhow::Result<WebSourceIndexOutput> {
    use crate::source::progress;

    let adapter = WebSourceAdapter::new(
        Arc::clone(&input.fetch_provider),
        Arc::clone(&input.render_provider),
    );
    events
        .running(PipelinePhase::Discovering, "discovering web source items")
        .await;
    let mut manifest = adapter.discover(&run.plan).await?;
    progress::discovered(events, &manifest).await;
    events
        .running(PipelinePhase::Diffing, "diffing web source manifest")
        .await;
    let diff = ledger.diff_manifest(manifest.clone()).await?;
    progress::diffed(events, &diff).await;
    if let Some(output) =
        unchanged_refresh_output(input, ledger, previous_source, &run, &manifest, &diff).await?
    {
        progress::published(
            events,
            &output.generation,
            manifest.items.len() as u64,
            &output.warnings,
            0,
            0,
        )
        .await;
        return Ok(output);
    }
    let diff = overlay_previous_web_etags(ledger, &diff).await?;

    events
        .running(PipelinePhase::Fetching, "fetching changed web source items")
        .await;
    let generation = ledger.create_generation(run.source_id.clone()).await?;
    manifest.generation = generation.generation.clone();
    let diff = retarget_diff_generation(diff, &generation.generation);
    ledger.put_manifest(manifest.clone()).await?;
    let manifest_items = manifest.items.len() as u64;
    // `scope = Map` is discover-only. `embed=false` is not discover-only: the
    // source contract requires acquire/normalize/prepare/graph to still run
    // while skipping only collection creation, embedding, and vector upsert.
    let output = if input.scope == SourceScope::Map {
        publish_map_generation(input, ledger, run, generation, manifest, diff).await?
    } else if !input.embed {
        events
            .running(
                PipelinePhase::Normalizing,
                "normalizing web source documents",
            )
            .await;
        events
            .running(PipelinePhase::Preparing, "preparing web source documents")
            .await;
        publish_prepared_generation_without_vectors(NoVectorGenerationRequest {
            input,
            ledger,
            run,
            lease,
            generation,
            manifest,
            diff,
        })
        .await?
    } else {
        events
            .running(
                PipelinePhase::Normalizing,
                "normalizing web source documents",
            )
            .await;
        events
            .running(PipelinePhase::Preparing, "preparing web source documents")
            .await;
        events
            .running(PipelinePhase::Embedding, "embedding web source chunks")
            .await;
        events
            .running(PipelinePhase::Upserting, "upserting web source vectors")
            .await;
        events
            .running(
                PipelinePhase::Publishing,
                "publishing web source generation",
            )
            .await;
        publish_vector_generation(VectorGenerationRequest {
            input,
            ledger,
            embedding_provider,
            vector_store,
            run,
            lease,
            generation,
            manifest,
            diff,
        })
        .await?
    };
    progress::published(
        events,
        &output.generation,
        manifest_items,
        &output.warnings,
        output.documents_prepared,
        output.chunks_prepared,
    )
    .await;
    Ok(output)
}

fn retarget_diff_generation(
    mut diff: SourceManifestDiff,
    generation: &SourceGenerationId,
) -> SourceManifestDiff {
    diff.next_generation = generation.clone();
    diff
}

async fn publish_map_generation(
    input: &WebSourceIndexInput,
    ledger: &dyn LedgerStore,
    run: WebAdapterRun,
    generation: SourceGeneration,
    manifest: SourceManifest,
    diff: SourceManifestDiff,
) -> anyhow::Result<WebSourceIndexOutput> {
    let completed = complete_generation(
        ledger,
        generation,
        &diff,
        GenerationDocumentCounts {
            discovered: manifest.items.len() as u64,
            ..GenerationDocumentCounts::default()
        },
    )
    .await?;
    let published = publish_generation_without_vectors(ledger, &completed).await?;
    ledger
        .upsert_source(completed_source_summary(
            input,
            &run,
            manifest.items.len() as u64,
            &diff,
            0,
        ))
        .await?;
    Ok(WebSourceIndexOutput {
        job_id: input.job_id,
        source_id: run.source_id,
        generation: published.generation,
        documents_prepared: 0,
        chunks_prepared: 0,
        vector_points_written: 0,
        removed_pages: diff.counts.removed,
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        artifacts: Vec::new(),
        inline: None,
    })
}

struct NoVectorGenerationRequest<'a> {
    input: &'a WebSourceIndexInput,
    ledger: &'a dyn LedgerStore,
    run: WebAdapterRun,
    lease: &'a LeaseGuard,
    generation: SourceGeneration,
    manifest: SourceManifest,
    diff: SourceManifestDiff,
}

async fn publish_prepared_generation_without_vectors(
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
    } = request;
    let prepared = prepare_changed_documents_without_vectors(
        input,
        &run,
        &diff,
        &generation.generation,
        ledger,
    )
    .await
    .map_err(|err| err.context("failed to prepare web source generation without vectors"));
    let prepared = match prepared {
        Ok(prepared) => prepared,
        Err(err) => return Err(fail_generation(ledger, generation, err).await),
    };
    let effective_diff = apply_reused_item_keys(&diff, &prepared.reused_item_keys);
    if let Err(err) = ensure_lease_before_publish(ledger, input, lease).await {
        let err = fail_generation(ledger, generation, err).await;
        return Err(cleanup_artifacts_after_error(
            input.artifact_store.as_ref(),
            &prepared.artifacts,
            err,
        )
        .await);
    }
    if let Err(err) = record_artifacts_on_manifest(
        ledger,
        &mut manifest,
        &effective_diff,
        &prepared.artifact_index,
    )
    .await
    {
        let err = fail_generation(ledger, generation, err).await;
        return Err(cleanup_artifacts_after_error(
            input.artifact_store.as_ref(),
            &prepared.artifacts,
            err,
        )
        .await);
    }
    let completed = match complete_generation(
        ledger,
        generation.clone(),
        &effective_diff,
        GenerationDocumentCounts {
            discovered: manifest.items.len() as u64,
            prepared: prepared.documents_prepared,
            embedded: 0,
            published: prepared.documents_prepared,
            failed: 0,
        },
    )
    .await
    {
        Ok(completed) => completed,
        Err(err) => {
            let err = fail_generation(ledger, generation, err).await;
            return Err(cleanup_artifacts_after_error(
                input.artifact_store.as_ref(),
                &prepared.artifacts,
                err,
            )
            .await);
        }
    };
    let published = match publish_generation_without_vectors(ledger, &completed).await {
        Ok(published) => published,
        Err(err) => {
            return Err(cleanup_artifacts_after_error(
                input.artifact_store.as_ref(),
                &prepared.artifacts,
                err,
            )
            .await);
        }
    };
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

#[cfg(test)]
#[path = "web_source_failure_tests.rs"]
mod failure_tests;
#[cfg(test)]
#[path = "web_source_tests.rs"]
mod tests;
