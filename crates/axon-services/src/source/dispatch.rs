//! Adapter acquisition dispatch for `index_source`.
//!
//! Services build a routed `SourcePlan` and hand it to the family adapter for
//! materialization. The returned adapter-owned guard keeps temporary artifacts
//! alive while the shared non-web document pipeline runs.

mod tool;
mod tool_artifacts;
mod tool_auth;
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
    AuthScope, AuthSnapshot, ConfigSnapshotId, EffectiveLimits, JobId, SourceLimits, SourcePlan,
    SourceRequest,
};
use axon_core::logging::log_info;
use uuid::Uuid;

use super::SourceExecutionContext;
use super::non_web::{NonWebPipelineInput, index_materialized_source};
use super::result_map::IndexCounts;
use crate::context::TargetLocalSourceRuntime;
use crate::{LocalSourceIndexInput, LocalSourceSelectionPolicy, index_local_source_with_job};
pub(crate) use tool::{dispatch_cli_tool, dispatch_mcp_tool};
pub(crate) use virtual_sources::{dispatch_memory, dispatch_upload};
pub(crate) use web::dispatch_web;

/// Placeholder used only by the remaining local-source implementation, which
/// replaces it with the durable job id before execution.
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

/// Local-path source: dispatch straight to the local bridge (no acquisition).
#[allow(clippy::too_many_arguments)]
pub async fn dispatch_local(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    embed: bool,
    route: &axon_api::source::RoutePlan,
) -> anyhow::Result<IndexCounts> {
    log_info(&format!(
        "command=source collection={collection} kind=local embed={embed}"
    ));
    let has_local_scope = auth_snapshot
        .map(|snapshot| super::authorize::snapshot_allows_scope(snapshot, AuthScope::Local))
        .unwrap_or(true);
    super::enforce_local_source_policy(input, has_local_scope)?;
    let index_input = LocalSourceIndexInput {
        root: std::path::PathBuf::from(input),
        collection: collection.to_string(),
        owner_id: owner_id.to_string(),
        job_id: placeholder_job_id(),
        embedding_provider_id: runtime.embedding_provider_id.clone(),
        vector_provider_id: runtime.vector_provider_id.clone(),
        embedding_model: runtime.embedding_model.clone(),
        embedding_dimensions: runtime.embedding_dimensions,
        selection_policy: LocalSourceSelectionPolicy::Permissive,
        embedding_reservations: Some(runtime.embedding_reservations.clone()),
        vector_reservations: Some(runtime.vector_reservations.clone()),
        auth_snapshot: auth_snapshot.cloned(),
        embed,
        route: Some(route.clone()),
    };
    let output = index_local_source_with_job(
        index_input,
        runtime.jobs.as_ref(),
        runtime.ledger.as_ref(),
        runtime.embedding_provider.as_ref(),
        runtime.vector_store.as_ref(),
    )
    .await
    .map_err(|e| anyhow::anyhow!(e.to_string()))
    .context("local source indexing failed")?;
    Ok(IndexCounts {
        job_id: output.job_id,
        source_id: output.source_id,
        generation: output.generation,
        documents_prepared: output.documents_prepared,
        chunks_prepared: output.chunks_prepared,
        vector_points_written: output.vector_points_written,
        removed: output.removed_files,
        graph_candidates: output.graph_candidates,
        warnings: Vec::new(),
        artifacts: Vec::new(),
        inline: None,
    })
}

/// Git-repository source: adapter-owned materialization followed by the shared
/// non-web document pipeline. The checkout guard stays alive through publish.
#[allow(clippy::too_many_arguments)]
pub async fn dispatch_git(
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
    let acquired = adapter
        .materialize(family_source_plan(input, route, embed, None, None))
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))
        .context("git clone failed")?;
    dispatch_materialized(
        runtime,
        &adapter,
        acquired,
        collection,
        owner_id,
        auth_snapshot,
        execution,
    )
    .await
    .context("git source indexing failed")
}

/// Feed source: adapter-owned bounded fetch followed by the shared document
/// pipeline.
#[allow(clippy::too_many_arguments)]
pub async fn dispatch_feed(
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
    let acquired = adapter
        .materialize(family_source_plan(input, route, embed, max_items, None))
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))
        .context("feed fetch failed")?;
    dispatch_materialized(
        runtime,
        &adapter,
        acquired,
        collection,
        owner_id,
        auth_snapshot,
        execution,
    )
    .await
    .context("feed source indexing failed")
}

/// Reddit source: adapter-owned OAuth and bounded acquisition followed by the
/// shared document pipeline.
#[allow(clippy::too_many_arguments)]
pub async fn dispatch_reddit(
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
    let acquired = adapter
        .materialize(family_source_plan(input, route, embed, max_items, None))
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))
        .context("reddit fetch failed")?;
    dispatch_materialized(
        runtime,
        &adapter,
        acquired,
        collection,
        owner_id,
        auth_snapshot,
        execution,
    )
    .await
    .context("reddit source indexing failed")
}

/// YouTube source: adapter-owned yt-dlp acquisition followed by the shared
/// document pipeline.
#[allow(clippy::too_many_arguments)]
pub async fn dispatch_youtube(
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
    let acquired = adapter
        .materialize(family_source_plan(input, route, embed, max_items, None))
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))
        .context("youtube fetch failed")?;
    dispatch_materialized(
        runtime,
        &adapter,
        acquired,
        collection,
        owner_id,
        auth_snapshot,
        execution,
    )
    .await
    .context("youtube source indexing failed")
}

/// Registry source: adapter-owned package acquisition followed by the shared
/// document pipeline.
#[allow(clippy::too_many_arguments)]
pub async fn dispatch_registry(
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
    let acquired = adapter
        .materialize(family_source_plan(input, route, embed, max_items, None))
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))
        .context("registry fetch failed")?;
    dispatch_materialized(
        runtime,
        &adapter,
        acquired,
        collection,
        owner_id,
        auth_snapshot,
        execution,
    )
    .await
    .context("registry source indexing failed")
}

/// Session source: adapter-owned validated selection followed by the shared
/// document pipeline.
#[allow(clippy::too_many_arguments)]
pub async fn dispatch_session(
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
    let acquired = adapter
        .materialize_with_roots(
            family_source_plan(input, route, embed, max_items, project_filter),
            roots,
        )
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))
        .context("session selection failed")?;
    dispatch_materialized(
        runtime,
        &adapter,
        acquired,
        collection,
        owner_id,
        auth_snapshot,
        execution,
    )
    .await
    .context("session source indexing failed")
}

async fn dispatch_materialized(
    runtime: &TargetLocalSourceRuntime,
    adapter: &dyn SourceAdapter,
    materialized: MaterializedSource,
    collection: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    execution: &SourceExecutionContext,
) -> anyhow::Result<IndexCounts> {
    let plan = materialized.plan.clone();
    let result = index_materialized_source(
        runtime,
        NonWebPipelineInput {
            adapter,
            plan,
            collection,
            owner_id,
            auth_snapshot,
            execution,
        },
    )
    .await;
    drop(materialized);
    result
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "dispatch/tool_tests.rs"]
mod tool_tests;
