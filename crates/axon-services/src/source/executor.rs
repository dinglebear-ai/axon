//! Generic adapter-owned pipeline for sources.

pub(super) mod artifact_candidates;
mod created_generation;
mod generation_reservation;
mod generation_spool;
use generation_reservation::reserve_unoccupied_generation;
mod generation_state;
mod generation_work;
mod helpers;
mod index;
pub(super) mod lease_heartbeat;
use lease_heartbeat::run_with_lease;
mod metadata;
mod preparation;
mod progress;
mod publish;
pub(super) use index::index_materialized_source;
mod finalization;
mod refresh_inventory;
mod retention;
mod reuse;
mod vector_points;
mod vectorize;
use super::events::SourceEventEmitter;
use super::execution::SourceExecutionContext;
use super::progress as source_progress;
use super::result_map::IndexCounts;
use crate::context::TargetLocalSourceRuntime;
use anyhow::Context as _;
use axon_adapters::{SourceAdapter, acquisition::MaterializedSource};
use axon_api::source::*;
use axon_jobs::boundary::JobStore;
use axon_ledger::store::LedgerStore;
use helpers::*;
#[cfg(test)]
#[path = "executor/web_refresh_tests.rs"]
mod web_refresh_tests;
use std::future::Future;
const SOURCE_LEASE_TTL_SECONDS: u64 = 30 * 60;
const PUBLICATION_CONFIG_KEY: &str = "axon_publication_config_snapshot_id";
/// Bound on added+modified items acquired/normalized/prepared/embedded per
/// streaming batch inside `run_created_generation` — matches the batch size
/// `web_source`/`local_source` already streamed diffs at before their
/// collapse into this runner (finding C1).
// Keep web acquisition in small enough waves that fetching the next group can
// overlap embedding the current one. This is materially faster on local Apple
// Silicon than erecting a site-wide fetch/embed barrier.
const ACQUIRE_BATCH_SIZE: usize = 16;

fn acquire_batch_size() -> usize {
    std::env::var("AXON_ACQUIRE_BATCH_SIZE")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(ACQUIRE_BATCH_SIZE)
        .clamp(1, 1024)
}

/// First acquisition wave size; defaults to the steady-state batch size. A
/// smaller first wave starts embedding sooner — the first fetch is the only
/// one that cannot overlap embedding.
fn first_acquire_batch_size(default_size: usize) -> usize {
    std::env::var("AXON_FIRST_ACQUIRE_BATCH_SIZE")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default_size)
        .clamp(1, 1024)
}
#[derive(Clone)]
pub(super) struct SourcePipelineInput<'a> {
    pub(super) graph_stage: Option<std::sync::Arc<axon_graph::stage::GraphStage>>,
    pub(super) adapter: &'a dyn SourceAdapter,
    pub(super) plan: SourcePlan,
    pub(super) collection: &'a str,
    pub(super) owner_id: &'a str,
    pub(super) auth_snapshot: Option<&'a AuthSnapshot>,
    pub(super) execution: &'a SourceExecutionContext,
}

async fn merge_source_and_release(
    runtime: &TargetLocalSourceRuntime,
    result: anyhow::Result<IndexCounts>,
    release: Result<(), axon_api::source::ApiError>,
) -> anyhow::Result<IndexCounts> {
    match (result, release) {
        (Ok(output), Ok(())) => Ok(output),
        (Err(error), Ok(())) => Err(error),
        (Ok(mut output), Err(error)) => {
            output.warnings.push(deferred_warning(
                "source.lease.release_deferred",
                format!(
                    "generation {} was published, but releasing the source lease failed: {error}",
                    output.generation.0
                ),
            ));
            persist_degraded_summary(runtime, &mut output).await;
            Ok(output)
        }
        (Err(error), Err(release_error)) => Err(error.context(format!(
            "additionally failed to release source lease: {release_error}"
        ))),
    }
}

async fn discover_and_diff(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    emitter: &SourceEventEmitter,
    coordinator: &progress::ProgressCoordinator,
) -> anyhow::Result<(
    SourceManifest,
    SourceManifestDiff,
    std::collections::BTreeSet<SourceItemKey>,
)> {
    coordinator
        .report(
            emitter,
            PipelinePhase::Discovering,
            progress::stage_counts(None, 0, None, 0, None, 0),
            "discovering source items",
        )
        .await;
    let mut manifest = input.adapter.discover(&input.plan).await?;
    apply_max_items(&mut manifest, input.plan.limits.effective.max_items);
    let item_count = manifest.items.len() as u64;
    coordinator
        .checkpoint(
            PipelinePhase::Discovering,
            progress::stage_counts(Some(item_count), item_count, None, 0, None, 0),
            "discovered source items",
        )
        .await;
    source_progress::discovered(emitter, &manifest).await;
    manifest.metadata.insert(
        PUBLICATION_CONFIG_KEY.to_string(),
        serde_json::json!(retention::processing_identity(runtime, input).0),
    );
    coordinator
        .report(
            emitter,
            PipelinePhase::Diffing,
            progress::stage_counts(Some(item_count), 0, None, 0, None, 0),
            "diffing source manifest",
        )
        .await;
    let unvisited =
        retention::merge_inventory(runtime.ledger.as_ref(), &input.plan, &mut manifest).await?;
    let diff = runtime.ledger.diff_manifest_ref(&manifest).await?;
    coordinator
        .checkpoint(
            PipelinePhase::Diffing,
            progress::stage_counts(Some(item_count), item_count, None, 0, None, 0),
            "diffed source manifest",
        )
        .await;
    source_progress::diffed(emitter, &diff).await;
    Ok((manifest, diff, unvisited))
}

async fn run_generation(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    emitter: &SourceEventEmitter,
    lease: &LeaseGuard,
    previous: Option<SourceSummary>,
) -> anyhow::Result<IndexCounts> {
    let coordinator = progress::ProgressCoordinator::new(runtime, input);
    let (mut manifest, mut diff, unvisited) = lease_heartbeat::until_cancelled(
        input.execution,
        discover_and_diff(runtime, input, emitter, &coordinator),
    )
    .await?;
    let publication_config_unchanged =
        refresh_inventory::prepare(runtime, input, &mut manifest, &mut diff, &unvisited).await?;
    if !manifest_has_changes(&diff)
        && publication_config_unchanged
        && input.plan.request.refresh != SourceRefreshPolicy::Force
    {
        return unchanged_result(
            runtime.ledger.as_ref(),
            input,
            manifest,
            &diff,
            previous.as_ref(),
        )
        .await;
    }
    if !publication_config_unchanged || input.plan.request.refresh == SourceRefreshPolicy::Force {
        force_publication_refresh(&mut diff);
    }
    diff = lease_heartbeat::until_cancelled(
        input.execution,
        reuse::overlay_trusted_validators(runtime, input, diff),
    )
    .await?;

    if input.plan.request.embed {
        lease_heartbeat::until_cancelled(input.execution, async {
            crate::reserved_call::ensure_source_vectors_ready(runtime)
                .await
                .map_err(anyhow::Error::from)
        })
        .await?;
    }
    let (generation, occupied_generations) =
        reserve_unoccupied_generation(runtime, input, &manifest.source_id).await?;
    diff.next_generation = generation.generation.clone();
    manifest.generation = generation.generation.clone();
    manifest.metadata.insert(
        axon_ledger::GENERATION_VECTOR_COLLECTION_METADATA_KEY.into(),
        input.collection.into(),
    );
    // Boxed: this is by far the largest future in the pipeline, and holding
    // it inline alongside the cancellation select overflows the default test
    // stack in debug builds.
    let run = Box::pin(async {
        runtime.ledger.put_manifest_ref(&manifest).await?;
        let recovered = runtime
            .ledger
            .recover_abandoned_generations(
                generation.clone(),
                lease.clone(),
                input.collection.to_string(),
            )
            .await?;
        if recovered > 0 {
            tracing::info!(source_id = %generation.source_id.0, generation = %generation.generation.0, recovered,
                "recorded lease-fenced cleanup for abandoned source generations");
        }
        created_generation::run_created_generation(
            runtime,
            input,
            emitter,
            lease,
            manifest,
            diff,
            generation.clone(),
            previous,
            &coordinator,
        )
        .await
    });
    // Cooperative cancellation: resolve to an error instead of letting the
    // caller drop the pipeline future mid-flight, so the failed-generation
    // cleanup below (vector cleanup + `fail_generation`) still runs for the
    // uncommitted generation (2026-08-23 adversarial pipeline review, M3).
    let result = match input.execution.cancellation.as_ref() {
        Some(cancel) => {
            tokio::select! {
                biased;
                () = cancel.cancelled() => Err(anyhow::anyhow!(
                    "source indexing canceled before generation {} was published",
                    generation.generation.0
                )),
                result = run => result,
            }
        }
        None => run.await,
    };
    match result {
        Ok(counts) => Ok(retire_imported_generations(
            runtime,
            input,
            &generation,
            occupied_generations,
            counts,
        )
        .await),
        Err(error) => {
            Err(finalization::finalize_failed_generation(runtime, input, generation, error).await)
        }
    }
}

async fn retire_imported_generations(
    runtime: &TargetLocalSourceRuntime,
    input: &SourcePipelineInput<'_>,
    generation: &SourceGeneration,
    occupied_generations: Vec<SourceGenerationId>,
    mut counts: IndexCounts,
) -> IndexCounts {
    for occupied in occupied_generations {
        if let Err(error) = crate::reserved_call::retire_generation(
            runtime,
            crate::reserved_call::ProviderCallContext::for_phase(
                input.plan.job_id,
                input.execution.attempt,
                PipelinePhase::Publishing,
                input.execution.priority,
                format!("retire-imported-generation:{}", occupied.0),
            ),
            input.collection.to_string(),
            generation.source_id.clone(),
            occupied.clone(),
            generation.generation.clone(),
        )
        .await
        {
            counts.warnings.push(
                publish::record_retirement_debt(
                    runtime,
                    input,
                    generation,
                    occupied,
                    input.collection,
                    error,
                )
                .await,
            );
        }
    }
    counts
}

fn job_create_request(input: &SourcePipelineInput<'_>) -> JobCreateRequest {
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
        // ever claimed one of these canonical source jobs (recovery/retry of
        // an interrupted git/feed/youtube/reddit/session/registry index) it
        // failed with "source job request is missing `source_request`".
        request: Some(serde_json::json!({
            "source_request": input.plan.request,
            "source_kind": input.plan.route.source.source_kind,
            "adapter": input.plan.route.adapter.name,
        })),
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

pub(super) fn successful_status(warnings: &[SourceWarning]) -> LifecycleStatus {
    if warnings.iter().any(SourceWarning::degrades_completion) {
        LifecycleStatus::CompletedDegraded
    } else {
        LifecycleStatus::Completed
    }
}

pub(super) async fn persist_degraded_summary(
    runtime: &TargetLocalSourceRuntime,
    output: &mut IndexCounts,
) {
    let update = async {
        let Some(mut summary) = runtime.ledger.get_source(output.source_id.clone()).await? else {
            return Ok::<(), axon_error::ApiError>(());
        };
        let now = timestamp();
        summary.status = LifecycleStatus::CompletedDegraded;
        summary.updated_at = now.clone();
        summary.last_refreshed_at = Some(now);
        runtime.ledger.upsert_source(summary).await
    }
    .await;
    if let Err(error) = update {
        output.warnings.push(deferred_warning(
            "source.summary.degraded_status_deferred",
            format!(
                "generation {} completed with warnings, but persisting its degraded source summary failed: {error}",
                output.generation.0
            ),
        ));
    }
}

fn deferred_warning(code: &str, message: String) -> SourceWarning {
    SourceWarning {
        code: code.to_string(),
        severity: Severity::Warning,
        message,
        source_item_key: None,
        retryable: true,
    }
}

pub(in crate::source) async fn record_terminal_status(
    jobs: &dyn JobStore,
    input: &SourcePipelineInput<'_>,
    result: &anyhow::Result<IndexCounts>,
) -> anyhow::Result<()> {
    let (status, error, counts) = match result {
        Ok(output) => (
            successful_status(&output.warnings),
            None,
            Some(stage_counts(output)),
        ),
        Err(error) => (
            LifecycleStatus::Failed,
            Some(terminal_source_error(error)),
            None,
        ),
    };
    record_terminal_update(
        jobs,
        input.plan.job_id,
        input.plan.route.source.source_id.clone(),
        input.adapter.name(),
        status,
        counts,
        error,
    )
    .await
}

pub(in crate::source) async fn record_completed_status(
    jobs: &dyn JobStore,
    output: &IndexCounts,
    adapter_name: &str,
) -> anyhow::Result<()> {
    record_terminal_update(
        jobs,
        output.job_id,
        output.source_id.clone(),
        adapter_name,
        successful_status(&output.warnings),
        Some(stage_counts(output)),
        None,
    )
    .await
}

pub(in crate::source) async fn record_failed_status(
    jobs: &dyn JobStore,
    output: &IndexCounts,
    adapter_name: &str,
    error: &anyhow::Error,
) -> anyhow::Result<()> {
    record_terminal_update(
        jobs,
        output.job_id,
        output.source_id.clone(),
        adapter_name,
        LifecycleStatus::Failed,
        None,
        Some(terminal_source_error(error)),
    )
    .await
}

async fn record_terminal_update(
    jobs: &dyn JobStore,
    job_id: JobId,
    source_id: SourceId,
    adapter_name: &str,
    status: LifecycleStatus,
    counts: Option<StageCounts>,
    error: Option<SourceError>,
) -> anyhow::Result<()> {
    jobs.update_status(JobStatusUpdate {
        job_id,
        source_id: Some(source_id),
        status,
        phase: PipelinePhase::Complete,
        stage_id: None,
        counts,
        current: None,
        message: Some(format!("{adapter_name} source {status:?}").to_ascii_lowercase()),
        error,
    })
    .await?;
    Ok(())
}
