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

pub mod authorize;
pub mod batch;
pub mod classify;
pub mod dispatch;
mod dispatch_kind;
pub mod enqueue;
pub(crate) mod events;
pub(crate) mod execution;
pub mod graph;
pub mod job_tracking;
mod non_web;
pub(crate) mod progress;
pub mod prune;
pub mod result_map;
pub mod routing;
pub mod security;
pub mod tool_policy;
pub use batch::{SourcePipelineBatch, plan_source_pipeline_batches};
pub use security::{
    SourceSecurityError, enforce_local_source_policy, enforce_network_source_policy,
    redact_local_path_for_public_payload,
};

use axon_api::source::{AuthSnapshot, PipelinePhase, SourceRequest, SourceResult, SourceScope};

use crate::context::{ServiceContext, TargetLocalSourceRuntime};
use classify::SourceInputKind;
pub(crate) use execution::SourceExecutionContext;
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
    index_source_with_auth(request, ctx, None).await
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
    let input = request.source.trim().to_string();
    if input.is_empty() {
        return Ok(result_map::unsupported_result(
            &request.source,
            "source request requires a non-empty local path, git URL, feed URL, youtube target, \
             reddit target, web URL, session selector, or registry target",
        ));
    }

    let routed = match routing::resolve_authorized_source_route(
        &request,
        &input,
        execution.auth_snapshot.as_ref(),
        events::SourceEventEmitter::new(ctx.job_store(), execution.existing_job_id)
            .with_attempt(execution.attempt),
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

    let collection = request
        .collection
        .clone()
        .unwrap_or_else(|| ctx.cfg().collection.clone());
    let owner_id = DEFAULT_OWNER_ID;

    let counts = dispatch_kind::dispatch_kind(
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
    )
    .await?;

    // Write the source graph: the baseline container + document + containment
    // skeleton from the just-published manifest, plus every parser-produced
    // `GraphCandidate` collected from this generation's prepared documents
    // (source-pipeline.md's `parsing` stage output feeding the `graphing`
    // stage). A missing pool or a graph-store error degrades to a zero-count
    // summary rather than failing the already-committed index.
    let graph = graph::write_baseline_graph(
        kind,
        ctx.jobs.sqlite_pool(),
        runtime.ledger.as_ref(),
        &counts,
        &route.source.canonical_uri,
        counts.graph_candidates.clone(),
    )
    .await;

    // Record the graph write as a child `graph` job of the parent source job,
    // when it produced non-trivial output (see `job_tracking` module docs for
    // why this is a child job rather than a standalone `axon graph` command).
    job_tracking::track_graph_mutation(
        ctx.job_store(),
        counts.job_id,
        execution.auth_snapshot.as_ref(),
        &graph,
    )
    .await;

    // Drain cleanup debt: after the new generation is committed, the ledger has
    // recorded superseded-item deletes (vector, ledger, graph, memory) for the
    // prior generation. Run the prune executor against every store boundary we
    // can open here so `GraphPrune`/`MemoryPrune` debt actually drains in
    // production, not just vector/ledger. Failures degrade gracefully — the
    // index is already published, so a cleanup problem must not fail
    // acquisition; a store that fails to open just leaves its debt kind
    // pending (see `open_cleanup_debt_stores`).
    event_emitter
        .running(PipelinePhase::Cleaning, "cleaning source generation debt")
        .await;
    let drain = drain_source_cleanup_debt(ctx, runtime, &collection, &counts).await;

    // Record the drain as a child `prune` job of the parent source job, when
    // it touched at least one pending debt entry.
    job_tracking::track_prune(
        ctx.job_store(),
        counts.job_id,
        execution.auth_snapshot.as_ref(),
        &drain,
    )
    .await;

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

async fn drain_source_cleanup_debt(
    ctx: &ServiceContext,
    runtime: &TargetLocalSourceRuntime,
    collection: &str,
    counts: &IndexCounts,
) -> prune::DebtDrainSummary {
    let (graph_store, memory_store) = open_cleanup_debt_stores(ctx).await;
    prune::drain_cleanup_debt_full_with_boundaries(
        runtime.ledger.as_ref(),
        runtime.vector_store.as_ref(),
        graph_store.as_deref(),
        memory_store.as_deref(),
        Some(runtime.jobs.as_ref()),
        Some(runtime.artifact_store.as_ref()),
        Some(runtime.document_cache.as_ref()),
        collection,
        counts,
    )
    .await
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
async fn open_cleanup_debt_stores(
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

/// Route the classified kind to its dispatch function.
///
/// `embed` and `limits` come straight from the transport-neutral
/// [`SourceRequest`] — see `docs/pipeline-unification/foundation/source-pipeline.md`
/// (`SourceRequest` + Validation Checklist: "`embed=false` never writes
/// vectors"). Every family bridge receives the real `request.embed` instead of
/// an implicit `true`. `limits.max_pages` and `limits.max_depth` are honored by
/// `web` (the only family whose acquisition path supports page/depth bounds);
/// `limits.max_items` is honored by `feed`/`youtube`/`reddit`/`session`/
/// `registry`, which each cap their discovered-manifest item count before
/// diffing. `local`/`git` do not take a `max_items` cap today, so it is not
/// threaded to them.
#[allow(clippy::too_many_arguments)]
async fn dispatch_item_limited_kind(
    kind: SourceInputKind,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    max_items: Option<u64>,
    route: &axon_api::source::RoutePlan,
    execution: &SourceExecutionContext,
) -> anyhow::Result<IndexCounts> {
    match kind {
        SourceInputKind::Feed => {
            dispatch::dispatch_feed(
                runtime,
                input,
                collection,
                owner_id,
                auth_snapshot,
                embed,
                max_items,
                route,
                execution,
            )
            .await
        }
        SourceInputKind::Youtube => {
            dispatch::dispatch_youtube(
                runtime,
                input,
                collection,
                owner_id,
                auth_snapshot,
                embed,
                max_items,
                route,
                execution,
            )
            .await
        }
        SourceInputKind::Reddit => {
            dispatch::dispatch_reddit(
                runtime,
                input,
                collection,
                owner_id,
                auth_snapshot,
                embed,
                max_items,
                route,
                execution,
            )
            .await
        }
        SourceInputKind::Registry => {
            dispatch::dispatch_registry(
                runtime,
                input,
                collection,
                owner_id,
                auth_snapshot,
                embed,
                max_items,
                route,
                execution,
            )
            .await
        }
        _ => Err(anyhow::anyhow!(
            "source kind does not support max-items dispatch: {kind:?}"
        )),
    }
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_web_kind(
    cfg: &axon_core::config::Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    scope: SourceScope,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    output: &axon_api::source::OutputPolicy,
    limits: &axon_api::source::SourceLimits,
    route: &axon_api::source::RoutePlan,
    execution: &SourceExecutionContext,
) -> anyhow::Result<IndexCounts> {
    dispatch::dispatch_web(
        cfg,
        runtime,
        input,
        collection,
        owner_id,
        scope,
        auth_snapshot,
        embed,
        limits.max_pages,
        limits.max_depth,
        output,
        route,
        execution,
    )
    .await
}

/// Adapter name reported on the result for each family.
fn adapter_name_for(kind: SourceInputKind) -> &'static str {
    match kind {
        SourceInputKind::Local => "local",
        SourceInputKind::Git => "git",
        SourceInputKind::Feed => "feed",
        SourceInputKind::Youtube => "youtube",
        SourceInputKind::Reddit => "reddit",
        SourceInputKind::Web => "web",
        SourceInputKind::Session => "sessions",
        SourceInputKind::Registry => "registry",
        SourceInputKind::CliTool => "cli_tool",
        SourceInputKind::McpTool => "mcp_tool",
        SourceInputKind::Memory => "memory",
        SourceInputKind::Upload => "upload",
        SourceInputKind::Unsupported => "unsupported",
    }
}
