//! Generic adapter-owned pipeline for non-web sources.

mod created_generation;
mod helpers;
mod metadata;
mod publish;
mod vectorize;
use super::events::SourceEventEmitter;
use super::execution::SourceExecutionContext;
use super::progress;
use super::result_map::IndexCounts;
use crate::context::TargetLocalSourceRuntime;
use anyhow::Context as _;
use axon_adapters::{SourceAdapter, acquisition::MaterializedSource};
use axon_api::source::*;
use axon_jobs::boundary::JobStore;
use axon_ledger::store::LedgerStore;
use helpers::*;
use std::future::Future;
const SOURCE_LEASE_TTL_SECONDS: u64 = 30 * 60;
const PUBLICATION_CONFIG_KEY: &str = "axon_publication_config_snapshot_id";
/// Bound on added+modified items acquired/normalized/prepared/embedded per
/// streaming batch inside `run_created_generation` — matches the batch size
/// `web_source`/`local_source` already streamed diffs at before their
/// collapse into this runner (finding C1).
const ACQUIRE_BATCH_SIZE: usize = 64;
pub(super) struct NonWebPipelineInput<'a> {
    pub(super) adapter: &'a dyn SourceAdapter,
    pub(super) plan: SourcePlan,
    pub(super) collection: &'a str,
    pub(super) owner_id: &'a str,
    pub(super) auth_snapshot: Option<&'a AuthSnapshot>,
    pub(super) execution: &'a SourceExecutionContext,
}

pub(super) async fn index_materialized_source<'a, F, Fut>(
    runtime: &'a TargetLocalSourceRuntime,
    mut input: NonWebPipelineInput<'a>,
    materialize: F,
) -> anyhow::Result<IndexCounts>
where
    F: FnOnce(SourcePlan) -> Fut + Send + 'a,
    Fut: Future<Output = anyhow::Result<MaterializedSource>> + Send + 'a,
{
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
        .with_source(
            input.plan.route.source.source_id.clone(),
            input.plan.route.source.canonical_uri.clone(),
        )
        .with_attempt(input.execution.attempt);

    let result = run_with_lease(runtime, &mut input, &emitter, materialize).await;
    let status_result = if owns_status {
        record_terminal_status(runtime.jobs.as_ref(), &input, &result).await
    } else {
        Ok(())
    };
    input.adapter.release(&input.plan);
    status_result?;
    result
}

async fn run_with_lease<'a, F, Fut>(
    runtime: &'a TargetLocalSourceRuntime,
    input: &mut NonWebPipelineInput<'a>,
    emitter: &'a SourceEventEmitter,
    materialize: F,
) -> anyhow::Result<IndexCounts>
where
    F: FnOnce(SourcePlan) -> Fut + Send + 'a,
    Fut: Future<Output = anyhow::Result<MaterializedSource>> + Send + 'a,
{
    let source_id = input.plan.route.source.source_id.clone();
    let previous = runtime.ledger.get_source(source_id.clone()).await?;
    // Upsert the source row BEFORE the first job-status update. `jobs.source_id`
    // has a foreign key to `sources(source_id)`, and `record_running_phase`
    // stamps `jobs.source_id`; if the source row does not exist yet the update
    // fails with a FOREIGN KEY constraint, the job stays Queued, and the
    // terminal handler's Queued -> Failed then masks the real cause with a
    // spurious `job.invalid_transition`. Seen live on every generic non-web
    // family (git/feed/youtube/reddit/session/registry); the web/local paths
    // already upsert the source first.
    let running_counts = previous
        .as_ref()
        .map(preserved_source_counts)
        .unwrap_or_else(empty_source_counts);
    runtime
        .ledger
        .upsert_source(metadata::source_summary(
            input,
            LifecycleStatus::Running,
            running_counts,
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
    let result = match materialize(input.plan.clone()).await {
        Ok(materialized) => {
            input.plan = materialized.plan.clone();
            let result = run_generation(runtime, input, emitter, &lease, previous.clone()).await;
            drop(materialized);
            result
        }
        Err(error) => Err(error),
    };
    if let Err(error) = &result {
        progress::pipeline_failed(emitter, error).await;
    }
    if let Err(error) = &result {
        runtime
            .ledger
            .upsert_source(metadata::source_summary(
                input,
                LifecycleStatus::Failed,
                previous
                    .as_ref()
                    .map(preserved_source_counts)
                    .unwrap_or_else(empty_source_counts),
                previous.as_ref(),
            ))
            .await
            .with_context(|| {
                format!("source failed with `{error}` and its summary could not be finalized")
            })?;
    }
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
    progress::discovered(emitter, &manifest).await;
    manifest.metadata.insert(
        PUBLICATION_CONFIG_KEY.to_string(),
        serde_json::json!(input.plan.config_snapshot_id.0.clone()),
    );
    record_running_phase(
        runtime,
        input,
        emitter,
        PipelinePhase::Diffing,
        "diffing source manifest",
    )
    .await?;
    let mut diff = runtime.ledger.diff_manifest(manifest.clone()).await?;
    progress::diffed(emitter, &diff).await;
    let publication_config_unchanged = match diff.previous_generation.as_ref() {
        Some(generation) => runtime
            .ledger
            .get_manifest(manifest.source_id.clone(), generation.clone())
            .await?
            .is_some_and(|previous| {
                publication_config_matches(&previous, &input.plan.config_snapshot_id)
            }),
        None => false,
    };
    if !manifest_has_changes(&diff) && publication_config_unchanged {
        return unchanged_result(
            runtime.ledger.as_ref(),
            input,
            &manifest,
            &diff,
            previous.as_ref(),
        )
        .await;
    }
    if !publication_config_unchanged {
        force_publication_refresh(&mut diff);
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

    let result = created_generation::run_created_generation(
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
        // Wrap as `{"source_request": <..>}` — the shape the source worker
        // (`run_source_request_with_context`) requires. Writing a raw
        // SourceRequest here diverges from `enqueue_source`, so if a worker
        // ever claimed one of these generic non-web jobs (recovery/retry of
        // an interrupted git/feed/youtube/reddit/session/registry index) it
        // failed with "source job request is missing `source_request`".
        request: Some(serde_json::json!({ "source_request": input.plan.request })),
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
        Err(error) => (
            LifecycleStatus::Failed,
            Some(terminal_source_error(error)),
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
