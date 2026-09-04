//! Small family-specific dispatch adapters used by the source orchestrator.

use std::sync::Arc;

use axon_adapters::SourceAdapter;
use axon_api::source::{AuthSnapshot, SourceKind, SourceScope};

use super::{SourceExecutionContext, dispatch};
use crate::context::TargetLocalSourceRuntime;
use crate::source::result_map::IndexCounts;

/// Route a family whose acquisition supports `max_items` to its adapter.
#[allow(clippy::too_many_arguments)]
pub(super) async fn dispatch_item_limited_kind(
    kind: SourceKind,
    adapter: Arc<dyn SourceAdapter>,
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
        SourceKind::Feed => {
            dispatch::dispatch_feed(
                adapter,
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
        SourceKind::Youtube => {
            dispatch::dispatch_youtube(
                adapter,
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
        SourceKind::Reddit => {
            dispatch::dispatch_reddit(
                adapter,
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
        SourceKind::Registry => {
            dispatch::dispatch_registry(
                adapter,
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
pub(super) async fn dispatch_web_kind(
    adapter: Arc<dyn SourceAdapter>,
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
        adapter,
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
pub(super) fn adapter_name_for(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Local => "local",
        SourceKind::Git => "git",
        SourceKind::Feed => "feed",
        SourceKind::Youtube => "youtube",
        SourceKind::Reddit => "reddit",
        SourceKind::Web => "web",
        SourceKind::Session => "sessions",
        SourceKind::Registry => "registry",
        SourceKind::CliTool => "cli_tool",
        SourceKind::McpTool => "mcp_tool",
        SourceKind::Memory => "memory",
        SourceKind::Upload => "upload",
    }
}
