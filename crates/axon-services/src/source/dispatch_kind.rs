//! Source-kind dispatch table for the unified source orchestrator.

use std::sync::Arc;

use axon_adapters::SourceAdapter;

use super::result_map::IndexCounts;
use super::{SourceExecutionContext, dispatch, dispatch_item_limited_kind, dispatch_web_kind};
use crate::context::{ServiceContext, TargetLocalSourceRuntime};
use axon_api::source::{
    AuthSnapshot, OutputPolicy, RoutePlan, SourceKind, SourceLimits, SourceScope,
};

#[allow(clippy::too_many_arguments)]
pub(super) async fn dispatch_kind(
    kind: SourceKind,
    scope: SourceScope,
    ctx: &ServiceContext,
    cfg: &axon_core::config::Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    output: &axon_api::source::OutputPolicy,
    limits: &axon_api::source::SourceLimits,
    route: &axon_api::source::RoutePlan,
    project_filter: Option<&str>,
    execution: &SourceExecutionContext,
) -> anyhow::Result<IndexCounts> {
    let (adapter, canonical_route) =
        canonical_registry_selection(kind, ctx, runtime, route).await?;
    let route = &canonical_route;

    match kind {
        SourceKind::Local | SourceKind::Git => {
            dispatch_local_or_git(
                kind,
                adapter,
                cfg,
                runtime,
                input,
                collection,
                owner_id,
                auth_snapshot,
                embed,
                route,
                execution,
            )
            .await
        }
        SourceKind::Feed | SourceKind::Youtube | SourceKind::Reddit => {
            dispatch_item_limited_kind(
                kind,
                adapter,
                runtime,
                input,
                collection,
                owner_id,
                auth_snapshot,
                embed,
                limits.max_items,
                route,
                execution,
            )
            .await
        }
        SourceKind::Web => {
            dispatch_web_kind(
                adapter,
                cfg,
                runtime,
                input,
                collection,
                owner_id,
                scope,
                auth_snapshot,
                embed,
                output,
                limits,
                route,
                execution,
            )
            .await
        }
        SourceKind::Session => {
            dispatch::dispatch_session(
                adapter,
                runtime,
                input,
                collection,
                owner_id,
                auth_snapshot,
                embed,
                limits.max_items,
                project_filter,
                route,
                execution,
            )
            .await
        }
        SourceKind::Registry => {
            dispatch_item_limited_kind(
                kind,
                adapter,
                runtime,
                input,
                collection,
                owner_id,
                auth_snapshot,
                embed,
                limits.max_items,
                route,
                execution,
            )
            .await
        }
        SourceKind::CliTool | SourceKind::McpTool | SourceKind::Memory | SourceKind::Upload => {
            dispatch_virtual_kind(
                kind,
                adapter,
                cfg,
                runtime,
                input,
                collection,
                owner_id,
                auth_snapshot,
                embed,
                route,
                execution,
            )
            .await
        }
    }
}

async fn canonical_registry_selection(
    kind: SourceKind,
    ctx: &ServiceContext,
    runtime: &TargetLocalSourceRuntime,
    route: &RoutePlan,
) -> anyhow::Result<(Arc<dyn SourceAdapter>, RoutePlan)> {
    let adapter = runtime
        .source_adapter_registry(ctx)
        .await?
        .adapter_for_source_kind(kind)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no source adapter registered for source kind {kind:?} (route adapter {})",
                route.adapter.name
            )
        })?;
    let adapter_ref = axon_api::source::AdapterRef {
        name: adapter.name().to_string(),
        version: adapter.version().to_string(),
    };
    let mut canonical_route = route.clone();
    canonical_route.adapter = adapter_ref.clone();
    canonical_route.source.adapter = adapter_ref;
    canonical_route.source.source_kind = kind;
    Ok((adapter, canonical_route))
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_local_or_git(
    kind: SourceKind,
    adapter: Arc<dyn SourceAdapter>,
    cfg: &axon_core::config::Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    route: &RoutePlan,
    execution: &SourceExecutionContext,
) -> anyhow::Result<IndexCounts> {
    match kind {
        SourceKind::Local => {
            dispatch::dispatch_local(
                adapter,
                cfg,
                runtime,
                input,
                collection,
                owner_id,
                auth_snapshot,
                embed,
                route,
                execution,
            )
            .await
        }
        SourceKind::Git => {
            dispatch::dispatch_git(
                adapter,
                cfg,
                runtime,
                input,
                collection,
                owner_id,
                auth_snapshot,
                embed,
                route,
                execution,
            )
            .await
        }
        _ => unreachable!("non-local source kind routed to local dispatcher"),
    }
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_virtual_kind(
    kind: SourceKind,
    adapter: Arc<dyn SourceAdapter>,
    cfg: &axon_core::config::Config,
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    route: &RoutePlan,
    execution: &SourceExecutionContext,
) -> anyhow::Result<IndexCounts> {
    match kind {
        SourceKind::CliTool => {
            let policy = dispatch::tool_auth::ToolExecutionPolicy::from_config(cfg);
            dispatch::dispatch_cli_tool(
                adapter,
                runtime,
                input,
                collection,
                owner_id,
                auth_snapshot,
                embed,
                route,
                execution,
                &policy,
            )
            .await
        }
        SourceKind::McpTool => {
            let policy = dispatch::tool_auth::ToolExecutionPolicy::from_config(cfg);
            dispatch::dispatch_mcp_tool(
                adapter,
                runtime,
                input,
                collection,
                owner_id,
                auth_snapshot,
                embed,
                route,
                execution,
                &policy,
            )
            .await
        }
        SourceKind::Memory => {
            dispatch::dispatch_memory(
                adapter,
                runtime,
                input,
                collection,
                owner_id,
                auth_snapshot,
                embed,
                route,
                execution,
            )
            .await
        }
        SourceKind::Upload => {
            dispatch::dispatch_upload(
                adapter,
                runtime,
                input,
                collection,
                owner_id,
                auth_snapshot,
                embed,
                route,
                execution,
            )
            .await
        }
        _ => Err(anyhow::anyhow!(
            "non-virtual source kind routed to virtual dispatcher: {kind:?}"
        )),
    }
}
