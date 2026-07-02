mod publish;
mod vectorize;
mod web_source_job;

pub use self::web_source_job::index_web_source_with_job;

use std::path::PathBuf;

use axon_adapters::{SourceAdapter, web::WebSourceAdapter};
use axon_api::source::*;
use axon_embedding::provider::EmbeddingProvider;
use axon_ledger::store::LedgerStore;
use axon_route::{AdapterRegistry, InMemoryAuthorityRegistry, SourceResolver, SourceRouter};
use axon_vectors::store::VectorStore;

use self::publish::{
    GenerationDocumentCounts, complete_generation, completed_source_summary,
    ensure_lease_before_publish, fail_generation, fail_generation_and_rollback_vectors,
    mark_vectors_for_completed_generation, publish_generation_and_rollback_vectors,
    publish_generation_without_vectors,
};
use self::vectorize::{
    VectorizeResult, collection_spec, published_status, vectorize_changed_documents,
};

pub(super) const WEB_LEASE_TTL_SECONDS: u64 = 30 * 60;

#[derive(Debug, Clone)]
pub struct WebSourceIndexInput {
    pub source: String,
    pub scope: SourceScope,
    pub manifest_path: Option<PathBuf>,
    pub markdown_root: Option<PathBuf>,
    pub map_urls: Vec<String>,
    pub collection: String,
    pub owner_id: String,
    pub job_id: JobId,
    pub embedding_provider_id: ProviderId,
    pub vector_provider_id: ProviderId,
    pub embedding_model: String,
    pub embedding_dimensions: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebSourceIndexOutput {
    pub job_id: JobId,
    pub source_id: SourceId,
    pub generation: SourceGenerationId,
    pub documents_prepared: u64,
    pub chunks_prepared: u64,
    pub vector_points_written: u64,
    pub removed_pages: u64,
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
    let mut manifest = WebSourceAdapter::new().discover(&run.plan).await?;
    let diff = ledger.diff_manifest(manifest.clone()).await?;
    if let Some(output) =
        unchanged_refresh_output(input, ledger, previous_source, &run, &manifest, &diff).await?
    {
        return Ok(output);
    }

    let generation = ledger.create_generation(run.source_id.clone()).await?;
    manifest.generation = generation.generation.clone();
    ledger.put_manifest(manifest.clone()).await?;
    if input.scope == SourceScope::Map {
        return publish_map_generation(input, ledger, run, generation, manifest, diff).await;
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
        &diff,
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
        &diff,
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
        diff: &diff,
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
    })
}

#[derive(Debug, Clone)]
pub(super) struct WebAdapterRun {
    source_id: SourceId,
    canonical_uri: String,
    adapter: AdapterRef,
    scope: SourceScope,
    plan: SourcePlan,
}

fn resolve_web_run(input: &WebSourceIndexInput) -> anyhow::Result<WebAdapterRun> {
    let mut request = SourceRequest::new(input.source.clone());
    request.scope = Some(input.scope);
    request.adapter = Some("web".to_string());
    if input.scope == SourceScope::Map {
        request
            .options
            .values
            .insert("map_urls".to_string(), serde_json::json!(input.map_urls));
    } else {
        let manifest_path = input.manifest_path.as_ref().ok_or_else(|| {
            anyhow::anyhow!("web source indexing requires manifest_path for non-map scopes")
        })?;
        let markdown_root = input.markdown_root.as_ref().ok_or_else(|| {
            anyhow::anyhow!("web source indexing requires markdown_root for non-map scopes")
        })?;
        request.options.values.insert(
            "manifest_path".to_string(),
            manifest_path.display().to_string().into(),
        );
        request.options.values.insert(
            "markdown_root".to_string(),
            markdown_root.display().to_string().into(),
        );
    }
    let registry = AdapterRegistry::target_defaults();
    let resolver = SourceResolver::new(InMemoryAuthorityRegistry::default(), registry.clone());
    let resolved = resolver.resolve(&request)?;
    let route = SourceRouter::new(registry).route(&request, resolved)?;
    let source_id = route.source.source_id.clone();
    let canonical_uri = route.source.canonical_uri.clone();
    let adapter = route.adapter.clone();
    let scope = route.scope;
    Ok(WebAdapterRun {
        source_id,
        canonical_uri,
        adapter,
        scope,
        plan: source_plan(input, request, route),
    })
}

fn source_plan(
    input: &WebSourceIndexInput,
    request: SourceRequest,
    route: RoutePlan,
) -> SourcePlan {
    SourcePlan {
        job_id: input.job_id,
        request,
        route,
        stage_plan: Vec::new(),
        limits: EffectiveLimits {
            request: SourceLimits::default(),
            adapter_defaults: SourceLimits::default(),
            config_defaults: SourceLimits::default(),
            effective: SourceLimits::default(),
        },
        config_snapshot_id: ConfigSnapshotId::new("cfg_web_source"),
        provider_reservations: Vec::new(),
    }
}

async fn unchanged_refresh_output(
    input: &WebSourceIndexInput,
    ledger: &dyn LedgerStore,
    previous_source: Option<SourceSummary>,
    run: &WebAdapterRun,
    manifest: &SourceManifest,
    diff: &SourceManifestDiff,
) -> anyhow::Result<Option<WebSourceIndexOutput>> {
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
    Ok(Some(WebSourceIndexOutput {
        job_id: input.job_id,
        source_id: run.source_id.clone(),
        generation: committed_generation,
        documents_prepared: 0,
        chunks_prepared: 0,
        vector_points_written: 0,
        removed_pages: 0,
    }))
}

fn manifest_diff_has_changes(diff: &SourceManifestDiff) -> bool {
    diff.counts.added > 0
        || diff.counts.modified > 0
        || diff.counts.removed > 0
        || diff.counts.skipped > 0
        || diff.counts.failed > 0
}

pub(super) fn source_summary(input: &WebSourceIndexInput, run: &WebAdapterRun) -> SourceSummary {
    SourceSummary {
        source_id: run.source_id.clone(),
        canonical_uri: run.canonical_uri.clone(),
        display_name: run.canonical_uri.clone(),
        source_kind: SourceKind::Web,
        adapter: run.adapter.clone(),
        authority: AuthorityLevel::Inferred,
        status: LifecycleStatus::Running,
        counts: SourceCounts {
            items_total: 0,
            items_changed: 0,
            documents_total: 0,
            chunks_total: 0,
            vector_points_total: 0,
            bytes_total: 0,
        },
        created_at: timestamp(),
        updated_at: timestamp(),
        tags: vec![format!("{:?}", run.scope).to_ascii_lowercase()],
        watch_id: None,
        last_job_id: Some(input.job_id),
    }
}

fn unchanged_source_summary(
    input: &WebSourceIndexInput,
    run: &WebAdapterRun,
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

pub(super) fn timestamp() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}

#[cfg(test)]
#[path = "web_source_failure_tests.rs"]
mod failure_tests;
#[cfg(test)]
#[path = "web_source_tests.rs"]
mod tests;
