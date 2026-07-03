//! Transport-neutral source orchestrator.
//!
//! [`index_source`] is the single entrypoint every surface (CLI, MCP, REST)
//! calls to acquire, normalize, embed, and publish one source. It:
//!
//! 1. **Classifies** the request's `source` string into an acquisition class
//!    ([`classify::classify_source_input`]) — local / git / feed / youtube /
//!    reddit / web / session / registry, in a fixed, tested order.
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
pub mod result_map;

use axon_api::source::{
    JobId, LedgerSummary, LifecycleStatus, SourceCounts, SourceGenerationId, SourceId, SourceKind,
    SourceRequest, SourceResult, SourceScope, SourceWarning,
};
use uuid::Uuid;

use crate::context::{ServiceContext, TargetLocalSourceRuntime};
use classify::SourceInputKind;
use result_map::{IndexCounts, adapter_ref, to_source_result};

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
    let input = request.source.trim().to_string();
    if input.is_empty() {
        return Ok(unsupported_result(
            &request.source,
            "source request requires a non-empty local path, git URL, feed URL, youtube target, \
             reddit target, web URL, session selector, or registry target",
        ));
    }

    let kind = classify::classify_source_input(&input).await;
    if kind == SourceInputKind::Unsupported {
        return Ok(unsupported_result(
            &input,
            &format!(
                "source supports local paths, git repository URLs, feed URLs, youtube targets, \
                 reddit targets, web URLs, session selectors (session:<claude|codex|gemini>:<path>), \
                 and registry targets (pkg:<npm|pypi|crates>/<package>); {input} is none of these"
            ),
        ));
    }

    let Some(runtime) = ctx.target_local_source_runtime() else {
        return Ok(degraded_no_data_plane(&input, kind));
    };

    let collection = request
        .collection
        .clone()
        .unwrap_or_else(|| ctx.cfg().collection.clone());
    let owner_id = DEFAULT_OWNER_ID;

    let counts = dispatch_kind(kind, ctx.cfg(), runtime, &input, &collection, owner_id).await?;

    Ok(to_source_result(
        source_kind_for(kind),
        adapter_ref(adapter_name_for(kind)),
        request.scope.unwrap_or_else(|| default_scope_for(kind)),
        input,
        counts,
    ))
}

/// Route the classified kind to its dispatch function.
async fn dispatch_kind(
    kind: SourceInputKind,
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
            dispatch::dispatch_web(cfg, runtime, input, collection, owner_id).await
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

/// Map an acquisition class to its [`SourceKind`] DTO variant.
fn source_kind_for(kind: SourceInputKind) -> SourceKind {
    match kind {
        SourceInputKind::Local => SourceKind::Local,
        SourceInputKind::Git => SourceKind::Git,
        SourceInputKind::Feed => SourceKind::Feed,
        SourceInputKind::Youtube => SourceKind::Youtube,
        SourceInputKind::Reddit => SourceKind::Reddit,
        SourceInputKind::Web => SourceKind::Web,
        SourceInputKind::Session => SourceKind::Session,
        SourceInputKind::Registry => SourceKind::Registry,
        SourceInputKind::Unsupported => SourceKind::Web,
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

/// Default scope for each family when the request omits one.
fn default_scope_for(kind: SourceInputKind) -> SourceScope {
    match kind {
        SourceInputKind::Local => SourceScope::Directory,
        SourceInputKind::Git => SourceScope::Repo,
        SourceInputKind::Feed => SourceScope::Feed,
        SourceInputKind::Youtube => SourceScope::Video,
        SourceInputKind::Reddit => SourceScope::Subreddit,
        SourceInputKind::Web => SourceScope::Site,
        SourceInputKind::Session => SourceScope::File,
        SourceInputKind::Registry => SourceScope::Package,
        SourceInputKind::Unsupported => SourceScope::Site,
    }
}

/// Build a degraded [`SourceResult`] when the data plane is not configured.
///
/// Mirrors the CLI's `require_data_plane` guard, but as a `Failed`
/// `SourceResult` with an explanatory warning instead of an `Err`, so the
/// transport contract (`Ok(SourceResult)`) is preserved.
fn degraded_no_data_plane(input: &str, kind: SourceInputKind) -> SourceResult {
    failed_result(
        input,
        source_kind_for(kind),
        adapter_name_for(kind),
        default_scope_for(kind),
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
        "unsupported",
        SourceScope::Site,
        "unsupported_source",
        message,
    )
}

/// Shared constructor for a `Failed` [`SourceResult`] carrying a single warning.
fn failed_result(
    input: &str,
    kind: SourceKind,
    adapter: &str,
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
        adapter: adapter_ref(adapter),
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
