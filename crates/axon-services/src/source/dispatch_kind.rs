//! Source-kind dispatch table for the unified source orchestrator.

use axon_api::source::{AuthSnapshot, OutputPolicy, RoutePlan, SourceLimits, SourceScope};

use super::classify::SourceInputKind;
use super::result_map::IndexCounts;
use super::{SourceExecutionContext, dispatch, dispatch_item_limited_kind, dispatch_web_kind};
use crate::context::{ServiceContext, TargetLocalSourceRuntime};

#[allow(clippy::too_many_arguments)]
pub(super) async fn dispatch_kind(
    kind: SourceInputKind,
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
    match kind {
        SourceInputKind::Local | SourceInputKind::Git => {
            dispatch_local_or_git(
                kind,
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
        SourceInputKind::Feed | SourceInputKind::Youtube | SourceInputKind::Reddit => {
            dispatch_item_limited_kind(
                kind,
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
        SourceInputKind::Web => {
            dispatch_web_kind(
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
        SourceInputKind::Session => {
            dispatch::dispatch_session(
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
        SourceInputKind::Registry => {
            dispatch_item_limited_kind(
                kind,
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
        SourceInputKind::CliTool
        | SourceInputKind::McpTool
        | SourceInputKind::Memory
        | SourceInputKind::Upload
        | SourceInputKind::Unsupported => {
            dispatch_virtual_kind(
                kind,
                ctx,
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

#[allow(clippy::too_many_arguments)]
async fn dispatch_local_or_git(
    kind: SourceInputKind,
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
                execution,
            )
            .await
        }
        _ => unreachable!("non-local source kind routed to local dispatcher"),
    }
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_virtual_kind(
    kind: SourceInputKind,
    ctx: &ServiceContext,
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
        SourceInputKind::CliTool => {
            dispatch::dispatch_cli_tool(runtime, input, owner_id, auth_snapshot, route).await
        }
        SourceInputKind::McpTool => {
            dispatch::dispatch_mcp_tool(runtime, input, owner_id, auth_snapshot, route).await
        }
        SourceInputKind::Memory => {
            dispatch::dispatch_memory(
                ctx,
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
        SourceInputKind::Upload => {
            dispatch::dispatch_upload(
                ctx,
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
        SourceInputKind::Unsupported => Err(anyhow::anyhow!("unsupported source input: {input}")),
        _ => unreachable!("non-virtual source kind routed to virtual dispatcher"),
    }
}
