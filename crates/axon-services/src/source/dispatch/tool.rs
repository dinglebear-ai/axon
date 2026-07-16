//! CLI/MCP tool source dispatch.
//!
//! Tool sources are metadata-only by default. Execution/call mode is a separate
//! path because it crosses a local-command boundary and must re-check auth,
//! allowlists, secret-path guards, and artifact redaction immediately before
//! invoking the adapter.

use axon_adapters::{
    SourceAdapter, cli_tool::CliToolSourceAdapter, mcp_tool::McpToolSourceAdapter,
};
use axon_api::source::{
    AdapterRef, AuthSnapshot, AuthorityLevel, DocumentCounts, ItemCounts, LifecycleStatus,
    PublishGenerationRequest, PublishState, SourceCounts, SourceGeneration, SourceGenerationId,
    SourceKind, SourceManifest, SourceManifestDiff, SourcePlan, SourceRequest, SourceSummary,
    Timestamp,
};
use axon_core::logging::log_info;

use crate::context::TargetLocalSourceRuntime;
use crate::source::result_map::IndexCounts;

use super::tool_artifacts::capture_tool_output_artifacts;
use super::tool_auth::{authorize_cli_tool_execution, authorize_mcp_tool_execution};

/// CLI tool source: commit metadata or execute an allowlisted command through
/// the `CliToolSourceAdapter`. Execution requires `scope=api`, execute auth,
/// an exact command allowlist, and redacted artifact capture.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_cli_tool(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    route: &axon_api::source::RoutePlan,
) -> anyhow::Result<IndexCounts> {
    let execution_authorized = authorize_cli_tool_execution(input, auth_snapshot, route)?;
    log_info(&format!(
        "command=source kind=cli_tool mode={} input={input}",
        if execution_authorized {
            "execute"
        } else {
            "metadata"
        }
    ));
    dispatch_static_tool_adapter(
        runtime,
        input,
        owner_id,
        route,
        SourceKind::CliTool,
        "cli_tool",
        &CliToolSourceAdapter::new(),
        execution_authorized,
    )
    .await
}

/// MCP tool source: commit schema metadata or call an allowlisted target
/// through the `McpToolSourceAdapter`. Calls require `scope=api`, execute
/// auth, exact target/caller allowlists, a caller command, and redacted
/// artifact capture.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_mcp_tool(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    owner_id: &str,
    auth_snapshot: Option<&AuthSnapshot>,
    route: &axon_api::source::RoutePlan,
) -> anyhow::Result<IndexCounts> {
    let execution_authorized = authorize_mcp_tool_execution(input, auth_snapshot, route)?;
    log_info(&format!(
        "command=source kind=mcp_tool mode={} input={input}",
        if execution_authorized {
            "call"
        } else {
            "metadata"
        }
    ));
    dispatch_static_tool_adapter(
        runtime,
        input,
        owner_id,
        route,
        SourceKind::McpTool,
        "mcp_tool",
        &McpToolSourceAdapter::new(),
        execution_authorized,
    )
    .await
}

async fn dispatch_static_tool_adapter(
    runtime: &TargetLocalSourceRuntime,
    input: &str,
    owner_id: &str,
    route: &axon_api::source::RoutePlan,
    source_kind: SourceKind,
    adapter_name: &'static str,
    adapter: &dyn SourceAdapter,
    execution_authorized: bool,
) -> anyhow::Result<IndexCounts> {
    let plan = static_tool_plan(
        input,
        route,
        source_kind,
        adapter_name,
        execution_authorized,
    );
    let mut manifest = adapter.discover(&plan).await?;
    let previous_source = runtime
        .ledger
        .get_source(plan.route.source.source_id.clone())
        .await?;
    runtime
        .ledger
        .upsert_source(static_tool_source_summary(
            &plan,
            owner_id,
            LifecycleStatus::Running,
            0,
            0,
        ))
        .await?;
    let diff = runtime.ledger.diff_manifest(manifest.clone()).await?;
    if let Some(output) =
        unchanged_static_tool_output(runtime, previous_source, &plan, owner_id, &manifest, &diff)
            .await?
    {
        return Ok(output);
    }

    let generation = runtime
        .ledger
        .create_generation(plan.route.source.source_id.clone())
        .await?;
    manifest.generation = generation.generation.clone();
    let diff = retarget_diff_generation(diff, &generation.generation);
    runtime.ledger.put_manifest(manifest.clone()).await?;
    let mut acquisition = adapter.acquire(&plan, &diff).await?;
    let artifacts = capture_tool_output_artifacts(runtime, &plan, &mut acquisition).await?;
    let documents = adapter.normalize(&plan, acquisition).await?.data;
    let completed = runtime
        .ledger
        .complete_generation(completed_static_tool_generation(
            generation,
            &diff,
            manifest.items.len() as u64,
            documents.len() as u64,
        ))
        .await?;
    let published = runtime
        .ledger
        .publish_generation(PublishGenerationRequest {
            source_id: completed.source_id.clone(),
            generation: completed.generation.clone(),
            expected_previous_generation: diff.previous_generation.clone(),
        })
        .await?;
    runtime
        .ledger
        .upsert_source(static_tool_source_summary(
            &plan,
            owner_id,
            LifecycleStatus::Completed,
            manifest.items.len() as u64,
            documents.len() as u64,
        ))
        .await?;

    Ok(IndexCounts {
        job_id: plan.job_id,
        source_id: plan.route.source.source_id,
        generation: published.generation,
        documents_prepared: documents.len() as u64,
        chunks_prepared: 0,
        vector_points_written: 0,
        removed: diff.counts.removed,
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        artifacts,
        inline: None,
    })
}

async fn unchanged_static_tool_output(
    runtime: &TargetLocalSourceRuntime,
    previous_source: Option<SourceSummary>,
    plan: &SourcePlan,
    owner_id: &str,
    manifest: &SourceManifest,
    diff: &SourceManifestDiff,
) -> anyhow::Result<Option<IndexCounts>> {
    if diff.counts.added > 0 || diff.counts.modified > 0 || diff.counts.removed > 0 {
        return Ok(None);
    }
    let Some(committed_generation) = diff.previous_generation.clone() else {
        return Ok(None);
    };
    let documents_total = previous_source
        .map(|source| source.counts.documents_total)
        .unwrap_or(0);
    runtime
        .ledger
        .upsert_source(static_tool_source_summary(
            plan,
            owner_id,
            LifecycleStatus::Completed,
            manifest.items.len() as u64,
            documents_total,
        ))
        .await?;
    Ok(Some(IndexCounts {
        job_id: plan.job_id,
        source_id: plan.route.source.source_id.clone(),
        generation: committed_generation,
        documents_prepared: 0,
        chunks_prepared: 0,
        vector_points_written: 0,
        removed: 0,
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        artifacts: Vec::new(),
        inline: None,
    }))
}

fn static_tool_plan(
    input: &str,
    routed: &axon_api::source::RoutePlan,
    source_kind: SourceKind,
    adapter_name: &'static str,
    execution_authorized: bool,
) -> SourcePlan {
    let adapter = AdapterRef {
        name: adapter_name.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let mut route = routed.clone();
    route.adapter = adapter.clone();
    route.source.adapter = adapter;
    route.source.source_kind = source_kind;
    route.scope = routed.scope;

    let mut request = SourceRequest::new(input.to_string());
    request.scope = Some(routed.scope);
    request.adapter = Some(adapter_name.to_string());
    request.embed = false;
    request.options = route.validated_options.clone();
    request.metadata.insert(
        "tool_execute_authorized".to_string(),
        serde_json::json!(execution_authorized),
    );

    SourcePlan {
        job_id: super::placeholder_job_id(),
        request,
        route,
        stage_plan: Vec::new(),
        limits: axon_api::source::EffectiveLimits {
            request: axon_api::source::SourceLimits::default(),
            adapter_defaults: axon_api::source::SourceLimits::default(),
            config_defaults: axon_api::source::SourceLimits::default(),
            effective: axon_api::source::SourceLimits::default(),
        },
        config_snapshot_id: axon_api::source::ConfigSnapshotId::new("cfg_tool_source"),
        provider_reservations: Vec::new(),
    }
}

fn static_tool_source_summary(
    plan: &SourcePlan,
    _owner_id: &str,
    status: LifecycleStatus,
    items_total: u64,
    documents_total: u64,
) -> SourceSummary {
    SourceSummary {
        source_id: plan.route.source.source_id.clone(),
        canonical_uri: plan.route.source.canonical_uri.clone(),
        display_name: plan.route.source.canonical_uri.clone(),
        source_kind: plan.route.source.source_kind,
        adapter: plan.route.adapter.clone(),
        authority: AuthorityLevel::Inferred,
        status,
        counts: SourceCounts {
            items_total,
            items_changed: documents_total,
            documents_total,
            chunks_total: 0,
            vector_points_total: 0,
            bytes_total: 0,
        },
        created_at: timestamp(),
        updated_at: timestamp(),
        graph_node_ids: Vec::new(),
        last_refreshed_at: if status == LifecycleStatus::Completed {
            Some(timestamp())
        } else {
            None
        },
        user_label: None,
        tags: vec!["tool".to_string(), "metadata".to_string()],
        watch_id: None,
        last_job_id: Some(plan.job_id),
    }
}

fn retarget_diff_generation(
    mut diff: SourceManifestDiff,
    generation: &SourceGenerationId,
) -> SourceManifestDiff {
    diff.next_generation = generation.clone();
    diff
}

fn completed_static_tool_generation(
    generation: SourceGeneration,
    diff: &SourceManifestDiff,
    items_total: u64,
    documents_total: u64,
) -> SourceGeneration {
    SourceGeneration {
        status: LifecycleStatus::Completed,
        publish_state: PublishState::Publishing,
        published_at: None,
        item_counts: ItemCounts {
            added: diff.counts.added,
            modified: diff.counts.modified,
            removed: diff.counts.removed,
            unchanged: diff.counts.unchanged,
            failed: diff.counts.failed,
        },
        document_counts: DocumentCounts {
            discovered: items_total,
            prepared: documents_total,
            embedded: 0,
            published: 0,
            failed: 0,
        },
        ..generation
    }
}

fn timestamp() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}
