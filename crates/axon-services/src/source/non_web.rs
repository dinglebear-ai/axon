//! Generic adapter-owned pipeline for non-web sources.

mod metadata;
mod publish;
mod vectorize;

use std::sync::Arc;

use anyhow::Context as _;
use axon_adapters::{SourceAdapter, SourceEnricher};
use axon_api::source::*;
use axon_jobs::boundary::JobStore;
use axon_ledger::store::LedgerStore;

use super::events::SourceEventEmitter;
use super::execution::SourceExecutionContext;
use super::result_map::IndexCounts;
use crate::context::TargetLocalSourceRuntime;

const SOURCE_LEASE_TTL_SECONDS: u64 = 30 * 60;

pub(super) struct NonWebPipelineInput<'a> {
    pub(super) adapter: &'a dyn SourceAdapter,
    pub(super) plan: SourcePlan,
    pub(super) collection: &'a str,
    pub(super) owner_id: &'a str,
    pub(super) auth_snapshot: Option<&'a AuthSnapshot>,
    pub(super) execution: &'a SourceExecutionContext,
}

pub(super) async fn index_materialized_source(
    runtime: &TargetLocalSourceRuntime,
    mut input: NonWebPipelineInput<'_>,
) -> anyhow::Result<IndexCounts> {
    input.plan.config_snapshot_id = crate::config_snapshot_hash::config_snapshot_id(
        &crate::config_snapshot_hash::JobConfigSnapshot {
            source_kind: input.adapter.name(),
            source_ref: &input.plan.route.source.canonical_uri,
            collection: input.collection,
            embedding_provider_id: &runtime.embedding_provider_id.0,
            vector_provider_id: &runtime.vector_provider_id.0,
            embedding_model: &runtime.embedding_model,
            embedding_dimensions: runtime.embedding_dimensions,
            embed: input.plan.request.embed,
            max_items: input.plan.limits.effective.max_items,
        },
    );
    let owns_status = input.execution.existing_job_id.is_none();
    let job_id = match input.execution.existing_job_id {
        Some(job_id) => job_id,
        None => {
            runtime
                .jobs
                .create(job_create_request(&input))
                .await?
                .job_id
        }
    };
    input.plan.job_id = job_id;
    let emitter = SourceEventEmitter::new(Some(runtime.jobs.clone()), Some(job_id))
        .with_route(
            input.plan.route.source.source_kind,
            input.plan.route.scope,
            input.plan.route.adapter.clone(),
        )
        .with_attempt(input.execution.attempt);

    let result = run_with_lease(runtime, &input, &emitter).await;
    if owns_status {
        record_terminal_status(runtime.jobs.as_ref(), &input, &result).await?;
    }
    result
}

async fn run_with_lease(
    runtime: &TargetLocalSourceRuntime,
    input: &NonWebPipelineInput<'_>,
    emitter: &SourceEventEmitter,
) -> anyhow::Result<IndexCounts> {
    let source_id = input.plan.route.source.source_id.clone();
    let previous = runtime.ledger.get_source(source_id.clone()).await?;
    runtime
        .ledger
        .upsert_source(metadata::source_summary(
            input,
            LifecycleStatus::Running,
            previous
                .as_ref()
                .map(|source| source.counts.clone())
                .unwrap_or_else(empty_source_counts),
            previous.as_ref(),
        ))
        .await?;
    record_running_phase(
        runtime,
        input,
        emitter,
        PipelinePhase::Leasing,
        "acquiring source lease",
    )
    .await?;
    let lease = runtime
        .ledger
        .acquire_lease(LeaseRequest {
            lease_key: format!("source:{}", source_id.0),
            owner_id: input.owner_id.to_string(),
            ttl_seconds: SOURCE_LEASE_TTL_SECONDS,
            job_id: Some(input.plan.job_id),
            metadata: MetadataMap::new(),
        })
        .await?
        .ok_or_else(|| anyhow::anyhow!("source refresh already running for {}", source_id.0))?;
    let result = run_generation(runtime, input, emitter, &lease, previous).await;
    let release = runtime
        .ledger
        .release_lease(lease.lease_id, input.owner_id.to_string())
        .await;
    match (result, release) {
        (Ok(output), Ok(())) => Ok(output),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => {
            Err(anyhow::Error::new(error).context("failed to release source lease"))
        }
        (Err(error), Err(release_error)) => Err(error.context(format!(
            "additionally failed to release source lease: {release_error}"
        ))),
    }
}

async fn run_generation(
    runtime: &TargetLocalSourceRuntime,
    input: &NonWebPipelineInput<'_>,
    emitter: &SourceEventEmitter,
    lease: &LeaseGuard,
    previous: Option<SourceSummary>,
) -> anyhow::Result<IndexCounts> {
    record_running_phase(
        runtime,
        input,
        emitter,
        PipelinePhase::Discovering,
        "discovering source items",
    )
    .await?;
    let mut manifest = input.adapter.discover(&input.plan).await?;
    apply_max_items(&mut manifest, input.plan.limits.effective.max_items);
    record_running_phase(
        runtime,
        input,
        emitter,
        PipelinePhase::Diffing,
        "diffing source manifest",
    )
    .await?;
    let diff = runtime.ledger.diff_manifest(manifest.clone()).await?;
    if !manifest_has_changes(&diff) {
        return unchanged_result(
            runtime.ledger.as_ref(),
            input,
            &manifest,
            &diff,
            previous.as_ref(),
        )
        .await;
    }

    if input.plan.request.embed {
        ensure_providers_ready(runtime).await?;
    }
    let generation = runtime
        .ledger
        .create_generation(manifest.source_id.clone())
        .await?;
    manifest.generation = generation.generation.clone();
    runtime.ledger.put_manifest(manifest.clone()).await?;

    let result = run_created_generation(
        runtime,
        input,
        emitter,
        lease,
        manifest,
        diff,
        generation.clone(),
        previous,
    )
    .await;
    if result.is_err() {
        let committed = runtime
            .ledger
            .committed_generation(generation.source_id.clone())
            .await?
            .is_some_and(|current| current == generation.generation);
        if !committed && input.plan.request.embed {
            let _ = runtime
                .vector_store
                .delete(VectorDeleteSelector::Generation {
                    collection: input.collection.to_string(),
                    source_id: generation.source_id.clone(),
                    generation: generation.generation.clone(),
                })
                .await;
        }
        if !committed && let Err(fail_error) = runtime.ledger.fail_generation(generation).await {
            return result.map_err(|error| {
                error.context(format!(
                    "also failed to mark source generation failed: {fail_error}"
                ))
            });
        }
    }
    result
}

#[allow(clippy::too_many_arguments)]
async fn run_created_generation(
    runtime: &TargetLocalSourceRuntime,
    input: &NonWebPipelineInput<'_>,
    emitter: &SourceEventEmitter,
    lease: &LeaseGuard,
    manifest: SourceManifest,
    diff: SourceManifestDiff,
    mut generation: SourceGeneration,
    previous: Option<SourceSummary>,
) -> anyhow::Result<IndexCounts> {
    record_running_phase(
        runtime,
        input,
        emitter,
        PipelinePhase::Fetching,
        "acquiring changed source items",
    )
    .await?;
    let acquisition = input.adapter.acquire(&input.plan, &diff).await?;
    let mut artifacts = acquisition.artifacts.clone();
    let mut warnings = acquisition.header.warnings.clone();
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
    warnings.extend(normalized.header.warnings.clone());
    let mut documents = normalized.data;
    apply_enrichments(&mut documents, &enrichments);
    let enrichment_graph = enrichment_graph_candidates(&enrichments);
    metadata::sanitize_documents(input.plan.route.source.source_kind, &mut documents);

    record_running_phase(
        runtime,
        input,
        emitter,
        PipelinePhase::Preparing,
        "preparing source documents",
    )
    .await?;
    let collection = collection_spec(input.collection, runtime.embedding_dimensions);
    if input.plan.request.embed {
        runtime
            .vector_store
            .ensure_collection(collection.clone())
            .await?;
    }
    let mut vectorized = vectorize::prepare_embed_publish(
        runtime,
        input,
        documents,
        &enrichment_graph,
        input.plan.route.source.source_kind == SourceKind::Session,
        &generation.generation,
        collection.clone(),
        emitter,
    )
    .await?;
    vectorized.warnings.splice(0..0, warnings);
    for enrichment in enrichments.values() {
        vectorized.warnings.extend(enrichment.warnings.clone());
        artifacts.extend(enrichment.artifacts.clone());
    }

    publish::ensure_lease(runtime.ledger.as_ref(), input, lease).await?;
    generation = publish::complete_generation(
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
    for status in &vectorized.document_statuses {
        runtime
            .ledger
            .update_document_status(publish::published_status(status))
            .await?;
    }
    let counts = SourceCounts {
        items_total: manifest.items.len() as u64,
        items_changed: diff.counts.added + diff.counts.modified + diff.counts.removed,
        documents_total: vectorized.documents_prepared,
        chunks_total: vectorized.chunks_prepared,
        vector_points_total: vectorized.points_written,
        bytes_total: manifest
            .items
            .iter()
            .map(|item| item.size_bytes.unwrap_or(0))
            .sum(),
    };
    runtime
        .ledger
        .upsert_source(metadata::source_summary(
            input,
            LifecycleStatus::Completed,
            counts,
            previous.as_ref(),
        ))
        .await?;
    emitter
        .completed(PipelinePhase::Publishing, "published source generation")
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

async fn unchanged_result(
    ledger: &dyn LedgerStore,
    input: &NonWebPipelineInput<'_>,
    manifest: &SourceManifest,
    diff: &SourceManifestDiff,
    previous: Option<&SourceSummary>,
) -> anyhow::Result<IndexCounts> {
    let generation = diff
        .previous_generation
        .clone()
        .ok_or_else(|| anyhow::anyhow!("unchanged source has no committed generation"))?;
    let counts = previous
        .map(|source| source.counts.clone())
        .unwrap_or(SourceCounts {
            items_total: manifest.items.len() as u64,
            items_changed: 0,
            documents_total: manifest.items.len() as u64,
            chunks_total: 0,
            vector_points_total: 0,
            bytes_total: manifest
                .items
                .iter()
                .map(|item| item.size_bytes.unwrap_or(0))
                .sum(),
        });
    ledger
        .upsert_source(metadata::source_summary(
            input,
            LifecycleStatus::Completed,
            counts,
            previous,
        ))
        .await?;
    Ok(IndexCounts {
        job_id: input.plan.job_id,
        source_id: manifest.source_id.clone(),
        generation,
        documents_prepared: 0,
        chunks_prepared: 0,
        vector_points_written: 0,
        removed: 0,
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        artifacts: Vec::new(),
        inline: None,
    })
}

fn job_create_request(input: &NonWebPipelineInput<'_>) -> JobCreateRequest {
    JobCreateRequest {
        request_id: None,
        job_kind: JobKind::Source,
        job_intent: JobIntent::Run,
        source_id: None,
        watch_id: None,
        parent_job_id: None,
        root_job_id: None,
        attempt: input.execution.attempt,
        priority: input.execution.priority,
        idempotency_key: input.execution.idempotency_key.clone(),
        stage_plan: input.plan.stage_plan.clone(),
        request: serde_json::to_value(&input.plan.request).ok(),
        auth_snapshot: input
            .auth_snapshot
            .cloned()
            .unwrap_or_else(|| AuthSnapshot::trusted_system("runtime")),
        config_snapshot_id: Some(input.plan.config_snapshot_id.clone()),
        requirements: MetadataMap::new(),
        result_schema: Some("source_result".to_string()),
        warnings: Vec::new(),
        error: None,
        metadata: MetadataMap::new(),
        deadline_at: None,
    }
}

async fn record_terminal_status(
    jobs: &dyn JobStore,
    input: &NonWebPipelineInput<'_>,
    result: &anyhow::Result<IndexCounts>,
) -> anyhow::Result<()> {
    let (status, error, counts) = match result {
        Ok(output) => (LifecycleStatus::Completed, None, Some(stage_counts(output))),
        Err(_error) => (
            LifecycleStatus::Failed,
            Some(SourceError {
                code: "source.index_failed".to_string(),
                severity: Severity::Failed,
                message: format!("{} source indexing failed", input.adapter.name()),
                source_item_key: None,
                retryable: false,
                provider_id: None,
                cause: None,
            }),
            None,
        ),
    };
    jobs.update_status(JobStatusUpdate {
        job_id: input.plan.job_id,
        source_id: Some(input.plan.route.source.source_id.clone()),
        status,
        phase: PipelinePhase::Complete,
        stage_id: None,
        counts,
        current: None,
        message: Some(format!("{} source {status:?}", input.adapter.name()).to_ascii_lowercase()),
        error,
    })
    .await?;
    Ok(())
}

fn stage_counts(output: &IndexCounts) -> StageCounts {
    StageCounts {
        items_total: Some(output.documents_prepared + output.removed),
        items_done: output.documents_prepared,
        documents_total: Some(output.documents_prepared),
        documents_done: output.documents_prepared,
        chunks_total: Some(output.chunks_prepared),
        chunks_done: output.chunks_prepared,
        bytes_total: None,
        bytes_done: 0,
    }
}

async fn record_running_phase(
    runtime: &TargetLocalSourceRuntime,
    input: &NonWebPipelineInput<'_>,
    emitter: &SourceEventEmitter,
    phase: PipelinePhase,
    message: &str,
) -> anyhow::Result<()> {
    runtime
        .jobs
        .update_status(JobStatusUpdate {
            job_id: input.plan.job_id,
            source_id: Some(input.plan.route.source.source_id.clone()),
            status: LifecycleStatus::Running,
            phase,
            stage_id: None,
            counts: None,
            current: None,
            message: Some(message.to_string()),
            error: None,
        })
        .await?;
    emitter.running(phase, message).await;
    Ok(())
}

async fn enrich(
    enricher: Arc<dyn SourceEnricher>,
    plan: &SourcePlan,
    items: &[AcquiredSourceItem],
) -> anyhow::Result<std::collections::BTreeMap<SourceItemKey, SourceEnrichment>> {
    let mut output = std::collections::BTreeMap::new();
    for item in items {
        let result = enricher.enrich(plan, item).await?;
        output.insert(item.manifest_item.source_item_key.clone(), result);
    }
    Ok(output)
}

fn apply_enrichments(
    documents: &mut [SourceDocument],
    enrichments: &std::collections::BTreeMap<SourceItemKey, SourceEnrichment>,
) {
    for document in documents {
        if let Some(enrichment) = enrichments.get(&document.source_item_key) {
            document.parser_hints.extend(enrichment.parse_hints.clone());
            document.chunk_hints.extend(enrichment.chunk_hints.clone());
            document.metadata.extend(enrichment.metadata.clone());
        }
    }
}

fn enrichment_graph_candidates(
    enrichments: &std::collections::BTreeMap<SourceItemKey, SourceEnrichment>,
) -> std::collections::BTreeMap<SourceItemKey, Vec<GraphCandidate>> {
    enrichments
        .iter()
        .filter_map(|(key, enrichment)| {
            (!enrichment.graph_candidates.is_empty())
                .then(|| (key.clone(), enrichment.graph_candidates.clone()))
        })
        .collect()
}

fn collection_spec(collection: &str, dimensions: u32) -> CollectionSpec {
    CollectionSpec {
        collection: collection.to_string(),
        dense: VectorConfig {
            name: "dense".to_string(),
            dimensions,
            distance: VectorDistance::Cosine,
        },
        payload_indexes: [
            "source_id",
            "source_generation",
            "source_item_key",
            "document_id",
            "chunk_id",
        ]
        .into_iter()
        .map(payload_index)
        .collect(),
        sparse: Some(SparseVectorConfig {
            name: "bm42".to_string(),
            modifier: SparseVectorModifier::Idf,
        }),
        aliases: Vec::new(),
        distance: Some(VectorDistance::Cosine),
        metadata: MetadataMap::new(),
    }
}

fn payload_index(field_name: &str) -> PayloadIndexSpec {
    PayloadIndexSpec {
        field_name: field_name.to_string(),
        field_schema: PayloadFieldSchema::Keyword,
        required_for_filters: true,
    }
}

fn apply_max_items(manifest: &mut SourceManifest, max_items: Option<u64>) {
    if let Some(limit) = max_items.and_then(|value| usize::try_from(value).ok()) {
        manifest.items.truncate(limit);
    }
}

fn manifest_has_changes(diff: &SourceManifestDiff) -> bool {
    !diff.added.is_empty()
        || !diff.modified.is_empty()
        || !diff.removed.is_empty()
        || !diff.failed.is_empty()
}

fn empty_source_counts() -> SourceCounts {
    SourceCounts {
        items_total: 0,
        items_changed: 0,
        documents_total: 0,
        chunks_total: 0,
        vector_points_total: 0,
        bytes_total: 0,
    }
}

async fn ensure_providers_ready(runtime: &TargetLocalSourceRuntime) -> anyhow::Result<()> {
    let embedding = runtime.embedding_provider.capabilities().await?;
    let vector = runtime.vector_store.capabilities().await?;
    for capability in [&embedding, &vector] {
        if !matches!(
            capability.health,
            HealthStatus::Healthy | HealthStatus::Degraded
        ) {
            return Err(capability
                .last_error
                .clone()
                .unwrap_or_else(|| {
                    ApiError::new(
                        "provider.not_ready",
                        ErrorStage::Planning,
                        format!("provider {} is not ready", capability.provider_id.0),
                    )
                })
                .into());
        }
    }
    if !vector
        .vector_store
        .as_ref()
        .is_some_and(|capability| capability.generation_publish)
    {
        anyhow::bail!("vector provider does not support source generation publication");
    }
    Ok(())
}

fn timestamp() -> Timestamp {
    Timestamp::from(chrono::Utc::now())
}
