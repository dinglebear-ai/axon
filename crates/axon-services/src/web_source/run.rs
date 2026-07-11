use axon_api::source::*;
use axon_ledger::store::LedgerStore;
use axon_route::{AdapterRegistry, InMemoryAuthorityRegistry, SourceResolver, SourceRouter};

use super::WebSourceIndexInput;
use super::WebSourceIndexOutput;

#[derive(Debug, Clone)]
pub(super) struct WebAdapterRun {
    pub(super) source_id: SourceId,
    pub(super) canonical_uri: String,
    pub(super) adapter: AdapterRef,
    pub(super) scope: SourceScope,
    pub(super) plan: SourcePlan,
}

pub(super) fn resolve_web_run(input: &WebSourceIndexInput) -> anyhow::Result<WebAdapterRun> {
    let mut request = SourceRequest::new(input.source.clone());
    request.scope = Some(input.scope);
    request.adapter = Some("web".to_string());
    if input.scope == SourceScope::Map {
        request
            .options
            .values
            .insert("map_urls".to_string(), serde_json::json!(input.map_urls));
    } else {
        let manifest_path = input.manifest_path.as_ref().ok_or_else(|| {
            anyhow::anyhow!("web source indexing requires manifest_path for non-map scopes")
        })?;
        let markdown_root = input.markdown_root.as_ref().ok_or_else(|| {
            anyhow::anyhow!("web source indexing requires markdown_root for non-map scopes")
        })?;
        request.options.values.insert(
            "manifest_path".to_string(),
            manifest_path.display().to_string().into(),
        );
        request.options.values.insert(
            "markdown_root".to_string(),
            markdown_root.display().to_string().into(),
        );
    }
    let registry = AdapterRegistry::target_defaults();
    let resolver = SourceResolver::new(InMemoryAuthorityRegistry::default(), registry.clone());
    let resolved = resolver.resolve(&request)?;
    let route = SourceRouter::new(registry).route(&request, resolved)?;
    let source_id = route.source.source_id.clone();
    let canonical_uri = route.source.canonical_uri.clone();
    let adapter = route.adapter.clone();
    let scope = route.scope;
    Ok(WebAdapterRun {
        source_id,
        canonical_uri,
        adapter,
        scope,
        plan: source_plan(input, request, route),
    })
}

fn source_plan(
    input: &WebSourceIndexInput,
    request: SourceRequest,
    route: RoutePlan,
) -> SourcePlan {
    SourcePlan {
        job_id: input.job_id,
        request,
        route,
        stage_plan: Vec::new(),
        limits: EffectiveLimits {
            request: SourceLimits::default(),
            adapter_defaults: SourceLimits::default(),
            config_defaults: SourceLimits::default(),
            effective: SourceLimits::default(),
        },
        config_snapshot_id: ConfigSnapshotId::new("cfg_web_source"),
        provider_reservations: Vec::new(),
    }
}

pub(super) async fn unchanged_refresh_output(
    input: &WebSourceIndexInput,
    ledger: &dyn LedgerStore,
    previous_source: Option<SourceSummary>,
    run: &WebAdapterRun,
    manifest: &SourceManifest,
    diff: &SourceManifestDiff,
) -> anyhow::Result<Option<WebSourceIndexOutput>> {
    if manifest_diff_has_changes(diff) {
        return Ok(None);
    }
    let Some(committed_generation) = diff.previous_generation.clone() else {
        return Ok(None);
    };
    ledger
        .upsert_source(unchanged_source_summary(
            input,
            run,
            previous_source,
            manifest.items.len() as u64,
        ))
        .await?;
    Ok(Some(WebSourceIndexOutput {
        job_id: input.job_id,
        source_id: run.source_id.clone(),
        generation: committed_generation,
        documents_prepared: 0,
        chunks_prepared: 0,
        vector_points_written: 0,
        removed_pages: 0,
        graph_candidates: Vec::new(),
    }))
}

fn manifest_diff_has_changes(diff: &SourceManifestDiff) -> bool {
    diff.counts.added > 0
        || diff.counts.modified > 0
        || diff.counts.removed > 0
        || diff.counts.skipped > 0
        || diff.counts.failed > 0
}

pub(super) fn source_summary(input: &WebSourceIndexInput, run: &WebAdapterRun) -> SourceSummary {
    SourceSummary {
        source_id: run.source_id.clone(),
        canonical_uri: run.canonical_uri.clone(),
        display_name: run.canonical_uri.clone(),
        source_kind: SourceKind::Web,
        adapter: run.adapter.clone(),
        authority: AuthorityLevel::Inferred,
        status: LifecycleStatus::Running,
        counts: SourceCounts {
            items_total: 0,
            items_changed: 0,
            documents_total: 0,
            chunks_total: 0,
            vector_points_total: 0,
            bytes_total: 0,
        },
        created_at: timestamp(),
        updated_at: timestamp(),
        graph_node_ids: Vec::new(),
        last_refreshed_at: None,
        user_label: None,
        tags: vec![format!("{:?}", run.scope).to_ascii_lowercase()],
        watch_id: None,
        last_job_id: Some(input.job_id),
    }
}

fn unchanged_source_summary(
    input: &WebSourceIndexInput,
    run: &WebAdapterRun,
    previous: Option<SourceSummary>,
    item_count: u64,
) -> SourceSummary {
    if let Some(mut summary) = previous {
        summary.status = LifecycleStatus::Completed;
        summary.counts.items_total = item_count;
        summary.counts.items_changed = 0;
        summary.updated_at = timestamp();
        return summary;
    }
    let mut summary = source_summary(input, run);
    summary.status = LifecycleStatus::Completed;
    summary.counts.items_total = item_count;
    summary.updated_at = timestamp();
    summary
}

pub(super) fn timestamp() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}
