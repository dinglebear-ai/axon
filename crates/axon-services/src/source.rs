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

pub mod classify;
pub mod dispatch;
pub mod graph;
pub mod prune;
pub mod result_map;
pub mod routing;
pub mod tool_policy;

use axon_api::source::{
    AdapterRef, JobId, LedgerSummary, LifecycleStatus, SourceCounts, SourceGenerationId, SourceId,
    SourceKind, SourceRequest, SourceResult, SourceScope, SourceWarning,
};
use axon_core::http::validate_url;
use axon_error::ApiError;
use std::fmt;
use uuid::Uuid;

use crate::context::{ServiceContext, TargetLocalSourceRuntime};
use classify::SourceInputKind;
use result_map::{IndexCounts, adapter_ref, to_source_result};

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
    lower.ends_with("/.env")
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
    let input = request.source.trim().to_string();
    if input.is_empty() {
        return Ok(unsupported_result(
            &request.source,
            "source request requires a non-empty local path, git URL, feed URL, youtube target, \
             reddit target, web URL, session selector, or registry target",
        ));
    }

    let routed = match routing::resolve_source_route(&request) {
        Ok(routed) => routed,
        Err(err) => return Ok(route_error_result(&input, err)),
    };
    let kind = routed.kind;
    let route = routed.route;
    if kind == SourceInputKind::Unsupported {
        return Ok(route_error_result(
            &input,
            ApiError::new(
                "source.route.unsupported_dispatch",
                axon_error::ErrorStage::Routing,
                "resolved source kind does not have a source dispatch implementation yet",
            )
            .with_context("source_kind", format!("{:?}", route.source.source_kind)),
        ));
    }

    let Some(runtime) = ctx.target_local_source_runtime() else {
        return Ok(degraded_no_data_plane(
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
    )
    .await?;

    // Write the baseline source graph (source container + document nodes +
    // containment edges) from the just-published manifest. A missing pool or a
    // graph-store error degrades to a zero-count summary rather than failing the
    // already-committed index.
    let graph = graph::write_baseline_graph(
        kind,
        ctx.jobs.sqlite_pool(),
        runtime.ledger.as_ref(),
        &counts,
        &input,
    )
    .await;

    // Drain cleanup debt: after the new generation is committed, the ledger has
    // recorded superseded-item vector deletes for the prior generation. Run the
    // prune executor to perform those generation-fenced deletes and mark the
    // debt resolved. Failures degrade gracefully — the index is already
    // published, so a cleanup problem must not fail acquisition.
    let _drain = prune::drain_cleanup_debt(
        runtime.ledger.as_ref(),
        runtime.vector_store.as_ref(),
        &collection,
        &counts,
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

/// Route the classified kind to its dispatch function.
async fn dispatch_kind(
    kind: SourceInputKind,
    scope: SourceScope,
    cfg: &axon_core::config::Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
) -> anyhow::Result<IndexCounts> {
    match kind {
        SourceInputKind::Local => {
            dispatch::dispatch_local(runtime, input, collection, owner_id).await
        }
        SourceInputKind::Git => dispatch::dispatch_git(runtime, input, collection, owner_id).await,
        SourceInputKind::Feed => {
            dispatch::dispatch_feed(runtime, input, collection, owner_id).await
        }
        SourceInputKind::Youtube => {
            dispatch::dispatch_youtube(runtime, input, collection, owner_id).await
        }
        SourceInputKind::Reddit => {
            dispatch::dispatch_reddit(runtime, input, collection, owner_id).await
        }
        SourceInputKind::Web => {
            dispatch::dispatch_web(cfg, runtime, input, collection, owner_id, scope).await
        }
        SourceInputKind::Session => {
            dispatch::dispatch_session(runtime, input, collection, owner_id).await
        }
        SourceInputKind::Registry => {
            dispatch::dispatch_registry(runtime, input, collection, owner_id).await
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

/// Build a degraded [`SourceResult`] when the data plane is not configured.
///
/// Mirrors the CLI's `require_data_plane` guard, but as a `Failed`
/// `SourceResult` with an explanatory warning instead of an `Err`, so the
/// transport contract (`Ok(SourceResult)`) is preserved.
fn degraded_no_data_plane(
    input: &str,
    kind: SourceKind,
    adapter: AdapterRef,
    scope: SourceScope,
) -> SourceResult {
    failed_result(
        input,
        kind,
        adapter,
        scope,
        "data_plane_unconfigured",
        "source indexing requires a running data plane (set qdrant_url + tei_url; \
         available under serve/mcp/--wait)",
    )
}

/// Build a failed [`SourceResult`] for an unsupported / empty input.
fn unsupported_result(input: &str, message: &str) -> SourceResult {
    failed_result(
        input,
        SourceKind::Web,
        adapter_ref("unsupported"),
        SourceScope::Site,
        "unsupported_source",
        message,
    )
}

fn route_error_result(input: &str, err: ApiError) -> SourceResult {
    let mut result = unsupported_result(input, &err.message);
    result.warnings.clear();
    result.warnings.push(SourceWarning {
        code: err.code.0,
        severity: axon_api::source::Severity::Failed,
        message: err.message,
        source_item_key: None,
        retryable: false,
    });
    result
}

/// Shared constructor for a `Failed` [`SourceResult`] carrying a single warning.
fn failed_result(
    input: &str,
    kind: SourceKind,
    adapter: AdapterRef,
    scope: SourceScope,
    code: &str,
    message: &str,
) -> SourceResult {
    let zero = SourceCounts {
        items_total: 0,
        items_changed: 0,
        documents_total: 0,
        chunks_total: 0,
        vector_points_total: 0,
        bytes_total: 0,
    };
    let source_id = SourceId::new(input);
    SourceResult {
        job_id: JobId::new(Uuid::nil()),
        source_id: source_id.clone(),
        canonical_uri: input.to_string(),
        source_kind: kind,
        adapter,
        scope,
        status: LifecycleStatus::Failed,
        ledger: LedgerSummary {
            source_id,
            generation: SourceGenerationId::new(""),
            committed_generation: None,
            status: LifecycleStatus::Failed,
            counts: zero.clone(),
        },
        graph: axon_api::source::GraphWriteSummary {
            nodes_upserted: 0,
            edges_upserted: 0,
            evidence_records: 0,
            degraded: true,
        },
        counts: zero,
        warnings: vec![SourceWarning {
            code: code.to_string(),
            severity: axon_api::source::Severity::Failed,
            message: message.to_string(),
            source_item_key: None,
            retryable: false,
        }],
        inline: None,
        job: None,
        watch: None,
        artifacts: Vec::new(),
        errors: Vec::new(),
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
