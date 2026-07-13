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
//! 3. **Dispatches** to the family's existing acquire helper + bridge
//!    ([`dispatch`]), which returns the numeric [`result_map::IndexCounts`].
//! 4. **Maps** the counts onto a [`SourceResult`] via
//!    [`result_map::to_source_result`].
//!
//! This is a relocation of the orchestration that previously lived in the CLI
//! (`commands/source.rs` + `commands/source/*.rs`). The acquire helpers and the
//! eight `index_*_source_with_job` bridges are unchanged.

pub mod authorize;
pub mod classify;
pub mod dispatch;
pub mod enqueue;
pub(crate) mod execution;
pub mod graph;
pub mod job_tracking;
pub mod prune;
pub mod result_map;
pub mod routing;
pub mod tool_policy;

use axon_api::source::{
    AdapterRef, AuthScope, AuthSnapshot, SourceRequest, SourceResult, SourceScope,
};
use axon_core::http::validate_url;
use axon_error::{ApiError, ErrorStage};
use std::fmt;

use crate::context::{ServiceContext, TargetLocalSourceRuntime};
use classify::SourceInputKind;
pub(crate) use execution::SourceExecutionContext;
use result_map::{IndexCounts, to_source_result};

/// Stable owner id used to lease sources indexed through this orchestrator when
/// the request does not carry its own. Matches the CLI's historical owner id.
const DEFAULT_OWNER_ID: &str = "cli";

/// One bounded batch boundary in the source pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcePipelineBatch {
    pub batch_id: usize,
    pub item_count: usize,
    pub chunk_count: usize,
    pub byte_count: usize,
    pub provider_reservation_id: Option<String>,
    pub elapsed_ms: u64,
}

/// Build the canonical bounded batch plan shared by source-family ports.
///
/// Source adapters stream item/document candidates. The service layer applies
/// this boundary before prepare, embedding, vector upsert, and graph writes so
/// no public source path needs to collect the whole source before downstream
/// stages can make progress.
pub fn plan_source_pipeline_batches(
    item_count: usize,
    batch_size: usize,
) -> anyhow::Result<Vec<SourcePipelineBatch>> {
    if batch_size == 0 {
        anyhow::bail!("source pipeline batch size must be greater than zero");
    }

    Ok((0..item_count)
        .collect::<Vec<_>>()
        .chunks(batch_size)
        .enumerate()
        .map(|(batch_id, chunk)| SourcePipelineBatch {
            batch_id,
            item_count: chunk.len(),
            chunk_count: chunk.len(),
            byte_count: 0,
            provider_reservation_id: None,
            elapsed_ms: 0,
        })
        .collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSecurityError {
    pub code: &'static str,
    pub message: String,
}

impl fmt::Display for SourceSecurityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for SourceSecurityError {}

/// Enforce SSRF policy before HTTP fetch, Chrome render, artifact writes, jobs,
/// graph writes, or vector writes can be created for network sources.
pub fn enforce_network_source_policy(urls: &[&str]) -> Result<(), SourceSecurityError> {
    for url in urls {
        validate_url(url).map_err(|err| SourceSecurityError {
            code: "security.ssrf_denied",
            message: format!("network source denied before side effects: {err}"),
        })?;
    }
    Ok(())
}

/// Enforce local-source scope and high-risk path policy before filesystem reads.
pub fn enforce_local_source_policy(
    path: &str,
    has_local_scope: bool,
) -> Result<(), SourceSecurityError> {
    if !has_local_scope {
        return Err(SourceSecurityError {
            code: "auth.scope_required",
            message: "local source requires axon:local or trusted local context".to_string(),
        });
    }
    if is_secret_like_local_path(path) {
        return Err(SourceSecurityError {
            code: "security.local_secret_denied",
            message: "secret-like local path denied before side effects".to_string(),
        });
    }
    Ok(())
}

pub fn redact_local_path_for_public_payload(path: &str) -> String {
    if path.starts_with('/') || path.starts_with("~/") {
        "[redacted-local-path]".to_string()
    } else {
        path.to_string()
    }
}

fn is_secret_like_local_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower == ".env"
        || lower.ends_with("/.env")
        || lower.contains("/.ssh/")
        || lower.contains("/.codex/")
        || lower.contains("/.gemini/")
        || lower.contains("browser-profile")
        || lower.contains("cloud")
}

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

    let routed = match routing::resolve_source_route(&request) {
        Ok(routed) => routed,
        Err(err) => return Ok(result_map::route_error_result(&input, err)),
    };
    let kind = routed.kind;
    let route = routed.route;
    // Authorizing stage (source-pipeline.md Stage Registry): the route plan's
    // declared credential requirements must be satisfied before any
    // discovering/fetching side effect. Does not degrade or mutate.
    if let Err(err) = authorize::authorize_route(&route) {
        return Ok(result_map::route_error_result(&input, err));
    }
    if let Err(err) =
        authorize::authorize_safety_class(route.safety_class, execution.auth_snapshot.as_ref())
    {
        return Ok(result_map::route_error_result(&input, err));
    }
    if let Err(err) = authorize_local_source_policy(&input, kind, execution.auth_snapshot.as_ref())
    {
        return Ok(result_map::route_error_result(&input, err));
    }
    if kind == SourceInputKind::Unsupported {
        return Ok(result_map::route_error_result(
            &input,
            ApiError::new(
                "source.route.unsupported_dispatch",
                ErrorStage::Routing,
                "resolved source kind does not have a source dispatch implementation yet",
            )
            .with_context("source_kind", format!("{:?}", route.source.source_kind)),
        ));
    }

    let Some(runtime) = ctx.target_local_source_runtime() else {
        return Ok(result_map::degraded_no_data_plane(
            &route.source.canonical_uri,
            route.source.source_kind,
            AdapterRef {
                name: route.adapter.name,
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            route.scope,
        ));
    };

    let collection = request
        .collection
        .clone()
        .unwrap_or_else(|| ctx.cfg().collection.clone());
    let owner_id = DEFAULT_OWNER_ID;

    let counts = dispatch_kind(
        kind,
        route.scope,
        ctx.cfg(),
        runtime,
        &input,
        &collection,
        owner_id,
        execution.auth_snapshot.as_ref(),
        request.embed,
        &request.limits,
        &route,
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
        &input,
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
    let (graph_store, memory_store) = open_cleanup_debt_stores(ctx).await;
    let drain = prune::drain_cleanup_debt_full(
        runtime.ledger.as_ref(),
        runtime.vector_store.as_ref(),
        graph_store.as_deref(),
        memory_store.as_deref(),
        &collection,
        &counts,
    )
    .await;

    // Record the drain as a child `prune` job of the parent source job, when
    // it touched at least one pending debt entry.
    job_tracking::track_prune(
        ctx.job_store(),
        counts.job_id,
        execution.auth_snapshot.as_ref(),
        &drain,
    )
    .await;

    Ok(to_source_result(
        route.source.source_kind,
        AdapterRef {
            name: route.adapter.name,
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        route.scope,
        route.source.canonical_uri,
        counts,
        graph,
    ))
}

fn source_security_api_error(err: SourceSecurityError) -> ApiError {
    ApiError::new(err.code, ErrorStage::Authorizing, err.message)
}

fn authorize_local_source_policy(
    input: &str,
    kind: SourceInputKind,
    auth_snapshot: Option<&AuthSnapshot>,
) -> Result<(), ApiError> {
    if kind != SourceInputKind::Local {
        return Ok(());
    }
    let has_local_scope = auth_snapshot
        .map(|snapshot| authorize::snapshot_allows_scope(snapshot, AuthScope::Local))
        .unwrap_or(true);
    enforce_local_source_policy(input, has_local_scope).map_err(source_security_api_error)
}

/// Open the `GraphStore`/`MemoryStore` handles the cleanup-debt drain uses to
/// resolve `GraphPrune`/`MemoryPrune` debt in production.
///
/// Degrades independently per store — a failure to open either one is logged
/// and yields `None` for that store rather than failing `index_source` (the
/// generation is already published by the time this runs). The memory store
/// is opened through [`crate::memory::memory_store`] — the same composed
/// (graph-mirrored, vector-backed) store every `memory` subaction uses — so a
/// drained `forget()` also hides the memory's vector points and graph recall
/// edges, not just its SQLite row.
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
/// an implicit `true`. `limits.max_pages` is honored by `web` (the only family
/// whose acquisition path supports a page cap); `limits.max_items` is honored
/// by `feed`/`youtube`/`reddit`/`session`/`registry`, which each cap their
/// discovered-manifest item count before diffing. `local`/`git` do not take a
/// `max_items` cap today, so it is not threaded to them.
#[allow(clippy::too_many_arguments)]
async fn dispatch_kind(
    kind: SourceInputKind,
    scope: SourceScope,
    cfg: &axon_core::config::Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    limits: &axon_api::source::SourceLimits,
    route: &axon_api::source::RoutePlan,
    execution: &SourceExecutionContext,
) -> anyhow::Result<IndexCounts> {
    match kind {
        SourceInputKind::Local => {
            dispatch::dispatch_local(
                runtime,
                input,
                collection,
                owner_id,
                auth_snapshot,
                embed,
                route,
            )
            .await
        }
        SourceInputKind::Git => {
            dispatch::dispatch_git(
                runtime,
                input,
                collection,
                owner_id,
                auth_snapshot,
                embed,
                route,
            )
            .await
        }
        SourceInputKind::Feed => {
            dispatch::dispatch_feed(
                runtime,
                input,
                collection,
                owner_id,
                auth_snapshot,
                embed,
                limits.max_items,
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
                limits.max_items,
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
                limits.max_items,
            )
            .await
        }
        SourceInputKind::Web => {
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
                execution,
            )
            .await
        }
        SourceInputKind::Session => {
            dispatch::dispatch_session(
                runtime,
                input,
                collection,
                owner_id,
                auth_snapshot,
                embed,
                limits.max_items,
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
                limits.max_items,
            )
            .await
        }
        // Unsupported is handled by the caller before dispatch.
        SourceInputKind::Unsupported => Err(anyhow::anyhow!("unsupported source input: {input}")),
    }
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
        SourceInputKind::Unsupported => "unsupported",
    }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "source_batch_tests.rs"]
mod source_batch_tests;

#[cfg(test)]
#[path = "source_security_tests.rs"]
mod source_security_tests;
