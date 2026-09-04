//! Transport-neutral source orchestrator.
//!
//! [`index_source`] is the single entrypoint every surface (CLI, MCP, REST)
//! calls to acquire, normalize, embed, and publish one source. It:
//!
//! 1. **Routes** the request through [`routing::resolve_source_route`], which
//!    delegates canonicalization, adapter matching, and scope validation to
//!    `axon-route` before the data plane or acquisition dispatch is touched.
//! 2. **Guards** on the data plane: source indexing needs a running
//!    [`TargetLocalSourceRuntime`] (qdrant + tei). When it is absent, a degraded
//!    [`SourceResult`] (`status = Failed`) with a clear warning is returned
//!    instead of an `Err`, matching the CLI's `require_data_plane` intent while
//!    keeping the transport contract (`Ok(SourceResult)`).
//! 3. **Dispatches** a routed [`axon_api::source::SourcePlan`] through the
//!    family adapter's acquisition boundary, then invokes the shared document
//!    preparation and publication pipeline.
//! 4. **Maps** the counts onto a [`SourceResult`] via
//!    [`result_map::to_source_result`].
//!
//! Non-web source acquisition is adapter-owned; services retain one
//! transport-neutral prepare/embed/publish pipeline.

pub(crate) mod adapter_registry;
pub mod authorize;
pub mod batch;
pub mod dispatch;
mod dispatch_kind;
pub(crate) mod document_cache;
pub mod enqueue;
pub(crate) mod events;
pub(crate) mod execution;
mod executor;
mod family_dispatch;

pub(crate) fn spawn_artifact_candidate_outbox_drain(
    runtime: &crate::context::TargetLocalSourceRuntime,
) {
    executor::artifact_candidates::spawn_outbox_drain(runtime);
}
pub mod foreground_progress;
pub mod graph;
pub mod job_tracking;
pub(crate) mod local_identity;
pub(crate) mod output;
pub(crate) mod progress;
pub mod prune;
pub mod result_map;
pub mod routing;
pub mod security;
pub mod tool_policy;
pub use batch::{SourcePipelineBatch, plan_source_pipeline_batches};
pub use security::{
    SourceSecurityError, enforce_local_source_allowed_roots, enforce_local_source_policy,
    redact_local_path_for_public_payload,
};

use std::sync::Arc;

use axon_adapters::SourceAdapter;
use axon_api::source::{
    AuthSnapshot, ExecutionAffinity, LifecycleStatus, PipelinePhase, RoutePlan, Severity,
    SourceKind, SourceRequest, SourceResult, SourceScope, SourceWarning,
};

use crate::context::{ServiceContext, TargetLocalSourceRuntime};
use crate::reserved_call::{self, ProviderCallContext};
pub(crate) use execution::SourceExecutionContext;
use family_dispatch::{adapter_name_for, dispatch_item_limited_kind, dispatch_web_kind};
use result_map::{IndexCounts, to_source_result_with_counts};

/// Stable owner id used to lease sources indexed through this orchestrator when
/// the request does not carry its own. Matches the CLI's historical owner id.
const DEFAULT_OWNER_ID: &str = "cli";

/// Acquire, normalize, embed, and publish one source through the unified
/// pipeline.
///
/// Routes `request.source` to its acquisition family, runs that family's
/// acquire + bridge, and returns a transport-neutral [`SourceResult`]. A missing
/// data plane or an unsupported input yields a degraded/failed `SourceResult`
/// (not an `Err`); genuine acquisition/index failures bubble up as `Err`.
pub async fn index_source(
    request: SourceRequest,
    ctx: &ServiceContext,
) -> anyhow::Result<SourceResult> {
    // This entrypoint is reserved for in-process CLI/system callers. Make its
    // trusted-local identity explicit so server transports cannot accidentally
    // inherit the historical `None`-means-local convention.
    index_source_with_auth(
        request,
        ctx,
        Some(AuthSnapshot::trusted_cli(env!("CARGO_PKG_VERSION"))),
    )
    .await
}

pub async fn index_source_with_progress(
    request: SourceRequest,
    ctx: &ServiceContext,
    foreground: foreground_progress::ForegroundProgressSender,
) -> anyhow::Result<SourceResult> {
    let execution = SourceExecutionContext::inline_with_progress(
        request.clone(),
        Some(AuthSnapshot::trusted_cli(env!("CARGO_PKG_VERSION"))),
        foreground,
    );
    index_source_inner(request, ctx, execution).await
}

pub(crate) async fn index_source_with_execution(
    request: SourceRequest,
    ctx: &ServiceContext,
    execution: SourceExecutionContext,
) -> anyhow::Result<SourceResult> {
    index_source_inner(request, ctx, execution).await
}

pub async fn index_source_with_auth(
    request: SourceRequest,
    ctx: &ServiceContext,
    auth_snapshot: Option<AuthSnapshot>,
) -> anyhow::Result<SourceResult> {
    let execution = SourceExecutionContext::inline(request.clone(), auth_snapshot);
    index_source_inner(request, ctx, execution).await
}

async fn index_source_inner(
    request: SourceRequest,
    ctx: &ServiceContext,
    execution: SourceExecutionContext,
) -> anyhow::Result<SourceResult> {
    let input = match validated_source_input(&request) {
        Ok(input) => input,
        Err(result) => return Ok(result),
    };

    let routed = match routing::resolve_authorized_source_route(
        &request,
        &input,
        execution.auth_snapshot.as_ref(),
        if execution.existing_job_id.is_some() {
            ExecutionAffinity::Worker
        } else {
            ExecutionAffinity::Inline
        },
        ctx.cfg().allow_tool_execution,
        Some(&ctx.cfg().source_local_allowed_roots),
        events::SourceEventEmitter::new(ctx.job_store(), execution.existing_job_id)
            .with_attempt(execution.attempt)
            .with_optional_foreground(execution.foreground.clone()),
    )
    .await
    {
        Ok(routed) => routed,
        Err(err) => return Ok(result_map::route_error_result(&input, err)),
    };
    let kind = routed.kind;
    let route = routed.route;
    let adapter = routed.adapter;
    let event_emitter = routed.event_emitter;

    let Some(runtime) = ctx.target_local_source_runtime() else {
        event_emitter
            .failed(
                PipelinePhase::Authorizing,
                "source data plane is unavailable",
            )
            .await;
        return Ok(result_map::degraded_no_data_plane(
            &route.source.canonical_uri,
            route.source.source_kind,
            adapter,
            route.scope,
        ));
    };

    let collection = source_collection(&request, ctx);
    let owner_id = DEFAULT_OWNER_ID;

    // Boxed: `dispatch_kind` owns the entire source pipeline (adapter
    // acquisition through vector publish); polled inline, the nested debug
    // poll frames overflow the default test-thread stack (the same class of
    // failure as `run_generation`'s boxed pipeline future).
    let counts = Box::pin(dispatch_kind::dispatch_kind(
        kind,
        route.scope,
        ctx,
        ctx.cfg(),
        runtime,
        &input,
        &collection,
        owner_id,
        execution.auth_snapshot.as_ref(),
        request.embed,
        &request.output,
        &request.limits,
        &route,
        request
            .options
            .values
            .get("project_filter")
            .and_then(serde_json::Value::as_str),
        &execution,
    ))
    .await?;

    let terminal_counts = counts.clone();
    let adapter_name = adapter.name.clone();
    let result = finalize_source_index(
        ctx,
        runtime,
        &execution,
        &collection,
        kind,
        route,
        adapter,
        counts,
        &event_emitter,
    )
    .await;
    finalize_owned_source_job(runtime, &execution, &adapter_name, terminal_counts, result).await
}

async fn finalize_owned_source_job(
    runtime: &TargetLocalSourceRuntime,
    execution: &SourceExecutionContext,
    adapter_name: &str,
    mut counts: IndexCounts,
    result: anyhow::Result<SourceResult>,
) -> anyhow::Result<SourceResult> {
    if execution.existing_job_id.is_some() {
        return result;
    }
    match result {
        Ok(mut output) => {
            // Post-publication audit warnings are produced after the dispatch
            // counts snapshot was taken. Terminalize from the authoritative
            // result warnings so the durable job cannot claim clean success
            // while the caller sees degraded completion.
            counts.warnings = output.warnings.clone();
            if let Err(status_error) =
                executor::record_completed_status(runtime.jobs.as_ref(), &counts, adapter_name)
                    .await
            {
                let warning = SourceWarning {
                    code: "source.job.terminal_status_deferred".to_string(),
                    severity: Severity::Warning,
                    message: format!(
                        "generation {} was published, but persisting the terminal job status failed: {status_error}",
                        counts.generation.0
                    ),
                    source_item_key: None,
                    retryable: true,
                };
                counts.warnings.push(warning.clone());
                executor::persist_degraded_summary(runtime, &mut counts).await;
                output.warnings.push(warning);
                output.status = LifecycleStatus::CompletedDegraded;
                output.ledger.status = LifecycleStatus::CompletedDegraded;
            }
            Ok(output)
        }
        Err(error) => {
            executor::record_failed_status(runtime.jobs.as_ref(), &counts, adapter_name, &error)
                .await
                .map_err(|status_error| {
                    anyhow::anyhow!(
                        "{error:#}; terminal job status update also failed: {status_error}"
                    )
                })?;
            Err(error)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn finalize_source_index(
    ctx: &ServiceContext,
    runtime: &TargetLocalSourceRuntime,
    execution: &SourceExecutionContext,
    collection: &str,
    kind: SourceKind,
    route: RoutePlan,
    adapter: axon_api::source::AdapterRef,
    mut counts: IndexCounts,
    event_emitter: &events::SourceEventEmitter,
) -> anyhow::Result<SourceResult> {
    let graph_candidates = std::mem::take(&mut counts.graph_candidates);
    let graph_manifest = counts.published_manifest.take();
    let graph_counts = counts.clone();
    let graph_context = ProviderCallContext::for_phase(
        counts.job_id,
        execution.attempt,
        PipelinePhase::Graphing,
        execution.priority,
        format!("graph:{}:{}", counts.source_id.0, counts.generation.0),
    );
    let graph = graph::write_baseline_graph_with_db_gate(
        Some(runtime),
        Some(graph_context),
        kind,
        ctx.jobs.sqlite_pool(),
        runtime.ledger.as_ref(),
        &graph_counts,
        &route.source.canonical_uri,
        graph_manifest,
        graph_candidates,
        Some(Arc::clone(&runtime.db_stage_slots)),
    )
    .await;

    let graph_audit_warning = job_tracking::track_graph_mutation(
        Some(runtime.jobs.clone()),
        counts.job_id,
        execution.auth_snapshot.as_ref(),
        &graph,
    )
    .await;
    record_post_publish_audit_warning(
        runtime,
        &mut counts,
        job_tracking::graph_outcome_warning(&graph),
    )
    .await;
    record_post_publish_audit_warning(runtime, &mut counts, graph_audit_warning).await;

    event_emitter
        .running(PipelinePhase::Cleaning, "cleaning source generation debt")
        .await;
    let drain = drain_source_cleanup_debt(ctx, runtime, collection, &counts).await;
    let prune_audit_warning = job_tracking::track_prune(
        Some(runtime.jobs.clone()),
        counts.job_id,
        execution.auth_snapshot.as_ref(),
        &drain,
    )
    .await;
    record_post_publish_audit_warning(
        runtime,
        &mut counts,
        job_tracking::prune_outcome_warning(&drain),
    )
    .await;
    record_post_publish_audit_warning(runtime, &mut counts, prune_audit_warning).await;
    event_emitter
        .completed(PipelinePhase::Complete, "source indexing complete")
        .await;

    let source_counts = runtime
        .ledger
        .get_source(counts.source_id.clone())
        .await?
        .map(|source| source.counts);
    Ok(to_source_result_with_counts(
        route.source.source_kind,
        adapter,
        route.scope,
        route.source.canonical_uri,
        counts,
        graph,
        source_counts,
    ))
}

async fn record_post_publish_audit_warning(
    runtime: &TargetLocalSourceRuntime,
    counts: &mut IndexCounts,
    warning: Option<SourceWarning>,
) {
    if let Some(warning) = warning {
        counts.warnings.push(warning);
        executor::persist_degraded_summary(runtime, counts).await;
    }
}

fn validated_source_input(request: &SourceRequest) -> Result<String, SourceResult> {
    let input = request.source.trim().to_string();
    if !input.is_empty() {
        return Ok(input);
    }
    Err(result_map::unsupported_result(
        &request.source,
        "source request requires a non-empty local path, git URL, feed URL, youtube target, \
         reddit target, web URL, session selector, or registry target",
    ))
}

fn source_collection(request: &SourceRequest, ctx: &ServiceContext) -> String {
    request
        .collection
        .clone()
        .unwrap_or_else(|| ctx.cfg().collection.clone())
}

async fn drain_source_cleanup_debt(
    ctx: &ServiceContext,
    runtime: &TargetLocalSourceRuntime,
    collection: &str,
    counts: &IndexCounts,
) -> prune::DebtDrainSummary {
    if let Err(error) = prune::bind_vector_cleanup_collection(
        runtime.ledger.as_ref(),
        &counts.source_id,
        collection,
    )
    .await
    {
        tracing::warn!(
            source_id = %counts.source_id.0,
            error = %error.message,
            "failed to persist vector cleanup collection identity; vector debt will stay pending"
        );
    }
    crate::reserved_call::drain_source_cleanup_debt(ctx, runtime, collection, counts).await
}

/// Open the `GraphStore`/`MemoryStore` handles the cleanup-debt drain uses to
/// resolve `GraphPrune`/`MemoryPrune` debt in production.
///
/// Degrades independently per store — a failure to open either one is logged
/// and yields `None` for that store rather than failing `index_source` (the
/// generation is already published by the time this runs). The memory store
/// is opened through [`crate::memory::memory_store`] — the same
/// SQLite-authoritative store every `memory` subaction uses. The drain also
/// receives the unified job store so a successful `forget()` enqueues its
/// canonical terminal `memory://` publication.
pub(crate) async fn open_cleanup_debt_stores(
    ctx: &ServiceContext,
) -> (
    Option<std::sync::Arc<dyn axon_graph::store::GraphStore>>,
    Option<std::sync::Arc<dyn axon_memory::store::MemoryStore>>,
) {
    let pool = ctx.jobs.sqlite_pool();
    let graph_store = match crate::graph::open_graph_store(ctx.cfg(), pool.as_deref()).await {
        Ok(store) => {
            Some(std::sync::Arc::new(store) as std::sync::Arc<dyn axon_graph::store::GraphStore>)
        }
        Err(err) => {
            tracing::warn!(
                error = %err,
                "failed to open graph store for cleanup-debt drain; GraphPrune debt will stay pending"
            );
            None
        }
    };

    let memory_store = match crate::memory::memory_store(ctx).await {
        Ok(store) => Some(store),
        Err(err) => {
            tracing::warn!(
                error = %err,
                "failed to open memory store for cleanup-debt drain; MemoryPrune debt will stay pending"
            );
            None
        }
    };

    (graph_store, memory_store)
}
