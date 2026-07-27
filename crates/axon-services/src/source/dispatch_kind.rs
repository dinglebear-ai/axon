//! Source-kind dispatch table for the unified source orchestrator.

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
    match kind {
        SourceKind::Local | SourceKind::Git => {
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
        SourceKind::Feed | SourceKind::Youtube | SourceKind::Reddit => {
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
        SourceKind::Web => {
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
        SourceKind::Session => {
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
        SourceKind::Registry => {
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
        SourceKind::CliTool | SourceKind::McpTool | SourceKind::Memory | SourceKind::Upload => {
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
    kind: SourceKind,
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
        SourceKind::CliTool => {
            let policy = dispatch::tool_auth::ToolExecutionPolicy::from_process();
            dispatch::dispatch_cli_tool(
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
            let policy = dispatch::tool_auth::ToolExecutionPolicy::from_process();
            dispatch::dispatch_mcp_tool(
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
        SourceKind::Upload => {
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
        _ => Err(anyhow::anyhow!(
            "non-virtual source kind routed to virtual dispatcher: {kind:?}"
        )),
    }
}
