//! Adapter acquisition dispatch for `index_source`.
//!
//! Services build a routed `SourcePlan` and hand it to the family adapter for
//! materialization. The returned adapter-owned guard keeps temporary artifacts
//! alive while the shared non-web document pipeline runs.

mod local;
mod tool;
mod tool_artifacts;
pub(super) mod tool_auth;
mod virtual_sources;
mod web;
pub(crate) mod web_options;

use anyhow::Context as _;
use axon_adapters::feed::FeedSourceAdapter;
use axon_adapters::git::GitSourceAdapter;
use axon_adapters::reddit::RedditSourceAdapter;
use axon_adapters::registry_sources::RegistrySourceAdapter;
use axon_adapters::sessions::{SessionRoots, SessionSourceAdapter};
use axon_adapters::youtube::YoutubeSourceAdapter;
use axon_adapters::{SourceAdapter, acquisition::MaterializedSource};
use axon_api::source::{
    AuthSnapshot, ConfigSnapshotId, EffectiveLimits, JobId, SourceLimits, SourcePlan, SourceRequest,
};
use axon_core::logging::log_info;
use uuid::Uuid;

use super::SourceExecutionContext;
use super::non_web::{NonWebPipelineInput, index_materialized_source};
use super::result_map::IndexCounts;
use crate::context::TargetLocalSourceRuntime;
pub(crate) use local::dispatch_local;
pub(crate) use tool::{dispatch_cli_tool, dispatch_mcp_tool};
pub(crate) use virtual_sources::{dispatch_memory, dispatch_upload};
pub(crate) use web::dispatch_web;

/// Placeholder job id for a `SourcePlan`/`LocalSourceIndexInput` field that
/// gets overwritten with the real durable job id before any use — every
/// generic non-web family (`family_source_plan`, below) and `dispatch_web`
/// (`dispatch/web.rs`) construct their plan with this placeholder and then
/// immediately replace it once the real (worker-supplied or freshly created)
/// job id is known, so the placeholder itself is never observed.
fn placeholder_job_id() -> JobId {
    JobId::new(Uuid::nil())
}

fn family_source_plan(
    input: &str,
    route: &axon_api::source::RoutePlan,
    embed: bool,
    max_items: Option<u64>,
    project_filter: Option<&str>,
) -> SourcePlan {
    let mut request = SourceRequest::new(input.to_string());
    request.scope = Some(route.scope);
    request.adapter = Some(route.adapter.name.clone());
    request.embed = embed;
    request.options = route.validated_options.clone();
    request.limits.max_items = max_items;
    if let Some(project_filter) = project_filter {
        request.options.values.insert(
            "project_filter".to_string(),
            serde_json::json!(project_filter),
        );
    }
    let effective = SourceLimits {
        max_items,
        ..SourceLimits::default()
    };
    SourcePlan {
        job_id: placeholder_job_id(),
        request,
        route: route.clone(),
        stage_plan: Vec::new(),
        limits: EffectiveLimits {
            request: effective.clone(),
            adapter_defaults: SourceLimits::default(),
            config_defaults: SourceLimits::default(),
            effective,
        },
        config_snapshot_id: ConfigSnapshotId::new("cfg_source_dispatch"),
        provider_reservations: Vec::new(),
    }
}

/// Git-repository source: adapter-owned materialization followed by the shared
/// non-web document pipeline. The checkout guard stays alive through publish.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_git(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    route: &axon_api::source::RoutePlan,
    execution: &SourceExecutionContext,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source collection={collection} kind=git embed={embed}"
    ));
    let adapter = GitSourceAdapter::new();
    let materializer = adapter.clone();
    dispatch_materialized(
        runtime,
        &adapter,
        family_source_plan(input, route, embed, None, None),
        collection,
        owner_id,
        auth_snapshot,
        execution,
        move |plan| async move {
            materializer
                .materialize(plan)
                .await
                .map_err(anyhow::Error::new)
        },
    )
    .await
    .context("git source indexing failed")
}

/// Feed source: adapter-owned bounded fetch followed by the shared document
/// pipeline.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_feed(
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
    log_info(&format!(
        "command=source collection={collection} kind=feed embed={embed} max_items={max_items:?}"
    ));
    let adapter = FeedSourceAdapter::new();
    let materializer = adapter.clone();
    dispatch_materialized(
        runtime,
        &adapter,
        family_source_plan(input, route, embed, max_items, None),
        collection,
        owner_id,
        auth_snapshot,
        execution,
        move |plan| async move {
            materializer
                .materialize(plan)
                .await
                .map_err(anyhow::Error::new)
        },
    )
    .await
    .context("feed source indexing failed")
}

/// Reddit source: adapter-owned OAuth and bounded acquisition followed by the
/// shared document pipeline.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_reddit(
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
    log_info(&format!(
        "command=source collection={collection} kind=reddit embed={embed} max_items={max_items:?}"
    ));
    let adapter = RedditSourceAdapter::new();
    let materializer = adapter.clone();
    dispatch_materialized(
        runtime,
        &adapter,
        family_source_plan(input, route, embed, max_items, None),
        collection,
        owner_id,
        auth_snapshot,
        execution,
        move |plan| async move {
            materializer
                .materialize(plan)
                .await
                .map_err(anyhow::Error::new)
        },
    )
    .await
    .context("reddit source indexing failed")
}

/// YouTube source: adapter-owned yt-dlp acquisition followed by the shared
/// document pipeline.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_youtube(
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
    log_info(&format!(
        "command=source collection={collection} kind=youtube embed={embed} max_items={max_items:?}"
    ));
    let adapter = YoutubeSourceAdapter::new();
    let materializer = adapter.clone();
    dispatch_materialized(
        runtime,
        &adapter,
        family_source_plan(input, route, embed, max_items, None),
        collection,
        owner_id,
        auth_snapshot,
        execution,
        move |plan| async move {
            materializer
                .materialize(plan)
                .await
                .map_err(anyhow::Error::new)
        },
    )
    .await
    .context("youtube source indexing failed")
}

/// Registry source: adapter-owned package acquisition followed by the shared
/// document pipeline.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_registry(
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
    log_info(&format!(
        "command=source collection={collection} kind=registry embed={embed} max_items={max_items:?}"
    ));
    let adapter = RegistrySourceAdapter::new();
    let materializer = adapter.clone();
    dispatch_materialized(
        runtime,
        &adapter,
        family_source_plan(input, route, embed, max_items, None),
        collection,
        owner_id,
        auth_snapshot,
        execution,
        move |plan| async move {
            materializer
                .materialize(plan)
                .await
                .map_err(anyhow::Error::new)
        },
    )
    .await
    .context("registry source indexing failed")
}

/// Session source: adapter-owned validated selection followed by the shared
/// document pipeline.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_session(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    max_items: Option<u64>,
    project_filter: Option<&str>,
    route: &axon_api::source::RoutePlan,
    execution: &SourceExecutionContext,
) -> anyhow::Result<IndexCounts> {
    let roots = SessionRoots::from_home_env()?;
    dispatch_session_with_roots(
        runtime,
        input,
        collection,
        owner_id,
        auth_snapshot,
        embed,
        max_items,
        project_filter,
        route,
        &roots,
        execution,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_session_with_roots(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    max_items: Option<u64>,
    project_filter: Option<&str>,
    route: &axon_api::source::RoutePlan,
    roots: &SessionRoots,
    execution: &SourceExecutionContext,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source collection={collection} kind=session embed={embed} max_items={max_items:?}"
    ));
    let adapter = SessionSourceAdapter::new();
    let materializer = adapter.clone();
    let roots = roots.clone();
    dispatch_materialized(
        runtime,
        &adapter,
        family_source_plan(input, route, embed, max_items, project_filter),
        collection,
        owner_id,
        auth_snapshot,
        execution,
        move |plan| async move {
            materializer
                .materialize_with_roots(plan, &roots)
                .await
                .map_err(anyhow::Error::new)
        },
    )
    .await
    .context("session source indexing failed")
}

async fn dispatch_materialized<'a, F, Fut>(
    runtime: &'a TargetLocalSourceRuntime,
    adapter: &'a dyn SourceAdapter,
    plan: SourcePlan,
    collection: &'a str,
    owner_id: &'a str,
    auth_snapshot: Option<&'a AuthSnapshot>,
    execution: &'a SourceExecutionContext,
    materialize: F,
) -> anyhow::Result<IndexCounts>
where
    F: FnOnce(SourcePlan) -> Fut + Send + 'a,
    Fut: std::future::Future<Output = anyhow::Result<MaterializedSource>> + Send + 'a,
{
    index_materialized_source(
        runtime,
        NonWebPipelineInput {
            adapter,
            plan,
            collection,
            owner_id,
            auth_snapshot,
            execution,
        },
        materialize,
    )
    .await
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "dispatch/tool_tests.rs"]
mod tool_tests;

#[cfg(test)]
#[path = "dispatch/local_collapse_tests.rs"]
mod local_collapse_tests;
