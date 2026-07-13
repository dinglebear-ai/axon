mod job_execution;
mod publish;
mod reuse;
mod run;
mod vectorize;
mod vectorize_helpers;
mod web_source_job;

pub(crate) use self::job_execution::WebSourceJobExecution;
pub(crate) use self::web_source_job::index_web_source_with_execution;
pub use self::web_source_job::index_web_source_with_job;
pub(crate) use self::web_source_job::job_create_request as web_source_job_create_request;

use std::sync::Arc;

use axon_adapters::boundary::{FetchProvider, RenderProvider};
use axon_adapters::{SourceAdapter, web::WebSourceAdapter};
use axon_api::source::*;
use axon_embedding::provider::EmbeddingProvider;
use axon_ledger::store::LedgerStore;
use axon_vectors::store::VectorStore;

use self::publish::{
    GenerationDocumentCounts, complete_generation, completed_source_summary,
    ensure_lease_before_publish, fail_generation, fail_generation_and_rollback_vectors,
    mark_vectors_for_completed_generation, publish_generation_and_rollback_vectors,
    publish_generation_without_vectors,
};
use self::run::apply_reused_item_keys;
use self::run::{
    WebAdapterRun, overlay_previous_web_etags, resolve_web_run, source_summary,
    unchanged_refresh_output,
};
use self::vectorize::{
    VectorizeResult, collection_spec, prepare_changed_documents_without_vectors, published_status,
    vectorize_changed_documents,
};

pub(super) const WEB_LEASE_TTL_SECONDS: u64 = 30 * 60;

#[cfg(test)]
#[path = "source_web_304_reuse_tests.rs"]
mod source_web_304_reuse_tests;

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
    pub collection: String,
    pub owner_id: String,
    pub job_id: JobId,
    pub embedding_provider_id: ProviderId,
    pub vector_provider_id: ProviderId,
    pub embedding_model: String,
    pub embedding_dimensions: u32,
    pub auth_snapshot: Option<AuthSnapshot>,
    /// `SourceRequest.embed` (source-pipeline.md, Validation Checklist:
    /// "`embed=false` never writes vectors"). When `false`, discovery,
    /// acquire, normalize, prepare, ledger publish, and graph-candidate
    /// collection still run; only collection creation, embedding, and vector
    /// upsert are skipped.
    pub embed: bool,
    pub fetch_provider: Arc<dyn FetchProvider>,
    pub render_provider: Arc<dyn RenderProvider>,
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
    let adapter = WebSourceAdapter::new(
        Arc::clone(&input.fetch_provider),
        Arc::clone(&input.render_provider),
    );
    let mut manifest = adapter.discover(&run.plan).await?;
    let diff = ledger.diff_manifest(manifest.clone()).await?;
    if let Some(output) =
        unchanged_refresh_output(input, ledger, previous_source, &run, &manifest, &diff).await?
    {
        return Ok(output);
    }
    let diff = overlay_previous_web_etags(ledger, &diff).await?;

    let generation = ledger.create_generation(run.source_id.clone()).await?;
    manifest.generation = generation.generation.clone();
    ledger.put_manifest(manifest.clone()).await?;
    // `scope = Map` is discover-only. `embed=false` is not discover-only: the
    // source contract requires acquire/normalize/prepare/graph to still run
    // while skipping only collection creation, embedding, and vector upsert.
    if input.scope == SourceScope::Map {
        return publish_map_generation(input, ledger, run, generation, manifest, diff).await;
    }
    if !input.embed {
        return publish_prepared_generation_without_vectors(NoVectorGenerationRequest {
            input,
            ledger,
            run,
            lease,
            generation,
            manifest,
            diff,
        })
        .await;
    }

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
    .await
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
        manifest,
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
        return Err(fail_generation(ledger, generation, err).await);
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
        Err(err) => return Err(fail_generation(ledger, generation, err).await),
    };
    let published = publish_generation_without_vectors(ledger, &completed).await?;
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
    })
}

struct VectorGenerationRequest<'a> {
    input: &'a WebSourceIndexInput,
    ledger: &'a dyn LedgerStore,
    embedding_provider: &'a dyn EmbeddingProvider,
    vector_store: &'a dyn VectorStore,
    run: WebAdapterRun,
    lease: &'a LeaseGuard,
    generation: SourceGeneration,
    manifest: SourceManifest,
    diff: SourceManifestDiff,
}

async fn publish_vector_generation(
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
        manifest,
        diff,
    } = request;
    let collection = collection_spec(input);
    if let Err(err) = vector_store.ensure_collection(collection.clone()).await {
        return Err(fail_generation(ledger, generation, anyhow::Error::new(err)).await);
    }
    let vectorized = vectorize_changed_documents(
        input,
        &run,
        &diff,
        &generation.generation,
        ledger,
        embedding_provider,
        vector_store,
        collection.clone(),
    )
    .await
    .map_err(|err| err.context("failed to vectorize web source generation"));
    let vectorized = match vectorized {
        Ok(vectorized) => vectorized,
        Err(err) => {
            return Err(fail_generation_and_rollback_vectors(
                ledger,
                vector_store,
                &collection,
                generation,
                err,
            )
            .await);
        }
    };
    let effective_diff = apply_reused_item_keys(&diff, &vectorized.reused_item_keys);
    if let Err(err) = ensure_lease_before_publish(ledger, input, lease).await {
        return Err(fail_generation_and_rollback_vectors(
            ledger,
            vector_store,
            &collection,
            generation,
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
            prepared: vectorized.documents_prepared,
            embedded: vectorized.documents_prepared,
            published: vectorized.documents_prepared,
            failed: 0,
        },
    )
    .await
    {
        Ok(completed) => completed,
        Err(err) => {
            return Err(fail_generation_and_rollback_vectors(
                ledger,
                vector_store,
                &collection,
                generation,
                err,
            )
            .await);
        }
    };
    let publish_stats = match mark_vectors_for_completed_generation(
        vector_store,
        &collection,
        &run.source_id,
        &completed,
        &effective_diff,
        vectorized.chunks_prepared,
    )
    .await
    {
        Ok(stats) => stats,
        Err(err) => {
            return Err(fail_generation_and_rollback_vectors(
                ledger,
                vector_store,
                &collection,
                completed,
                err,
            )
            .await);
        }
    };
    let published =
        publish_generation_and_rollback_vectors(ledger, vector_store, &collection, &completed)
            .await?;
    record_published_vector_generation(PublishedVectorRecord {
        input,
        ledger,
        run: &run,
        manifest: &manifest,
        diff: &effective_diff,
        vectorized,
        published,
        points_written: publish_stats.total_points_written(),
    })
    .await
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
    })
}

#[cfg(test)]
#[path = "web_source_failure_tests.rs"]
mod failure_tests;
#[cfg(test)]
#[path = "web_source_tests.rs"]
mod tests;
