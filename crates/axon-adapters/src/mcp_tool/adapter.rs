//! `SourceAdapter` wiring for `mcp://server/tool` sources — discover/acquire/
//! normalize built on top of the metadata-only-by-default
//! `resolve_and_acquire` contract in the parent `mcp_tool` module. See that
//! module's doc comment for why real (`Execute`-mode) tool calls are not
//! wired here yet.

use async_trait::async_trait;
use axon_api::source::*;
use serde_json::json;
use uuid::Uuid;

use crate::adapter::Result as AdapterResult;
use crate::adapter::SourceAdapter;
use crate::capability::AdapterCapability;
use crate::manifest::item_identity;

use super::metadata::mcp_tool_source_document;
use super::{
    McpExecutionMode, McpToolAcquireResult, McpToolTarget, parse_mcp_target, resolve_and_acquire,
};

const ADAPTER_NAME: &str = "mcp_tool";

/// Real `SourceAdapter` wiring for `mcp://server/tool` sources. See the
/// parent module's doc comment for why this always resolves in
/// [`McpExecutionMode::MetadataOnly`] today.
#[derive(Debug, Clone, Default)]
pub struct McpToolSourceAdapter;

impl McpToolSourceAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SourceAdapter for McpToolSourceAdapter {
    fn name(&self) -> &'static str {
        ADAPTER_NAME
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    async fn capabilities(&self) -> AdapterResult<SourceAdapterCapability> {
        Ok(mcp_tool_capability(self.version()).into())
    }

    async fn discover(&self, plan: &SourcePlan) -> AdapterResult<SourceManifest> {
        discover_sync(plan)
    }

    async fn acquire(
        &self,
        plan: &SourcePlan,
        diff: &SourceManifestDiff,
    ) -> AdapterResult<SourceAcquisition> {
        acquire_sync(plan, diff)
    }

    async fn normalize(
        &self,
        plan: &SourcePlan,
        acquisition: SourceAcquisition,
    ) -> AdapterResult<StageExecutionResult<Vec<SourceDocument>>> {
        normalize_sync(plan, acquisition)
    }
}

fn mcp_tool_capability(version: &str) -> AdapterCapability {
    AdapterCapability::new(
        AdapterRef {
            name: ADAPTER_NAME.to_string(),
            version: version.to_string(),
        },
        SourceKind::McpTool,
        SourceScope::Tool,
    )
    .with_scope(SourceScope::Api)
}

/// Parses `plan.request.source` into its `(server, tool)` identity without
/// resolving any content — used by `discover`/`normalize` to name the
/// manifest item/document.
fn resolve_target(plan: &SourcePlan) -> AdapterResult<McpToolTarget> {
    parse_mcp_target(&plan.request.source)
        .map_err(|err| ApiError::new(err.code, axon_error::ErrorStage::Planning, err.message))
}

/// Resolves `plan.request.source` in metadata-only mode — see the parent
/// module's doc comment for why `Execute` mode is never selected here today.
fn resolve_metadata(plan: &SourcePlan) -> AdapterResult<McpToolAcquireResult> {
    resolve_and_acquire(
        &plan.request.source,
        McpExecutionMode::MetadataOnly,
        false,
        &[],
        None,
    )
    .map_err(|err| ApiError::new(err.code, axon_error::ErrorStage::Planning, err.message))
}

fn discover_sync(plan: &SourcePlan) -> AdapterResult<SourceManifest> {
    mcp_tool_capability(env!("CARGO_PKG_VERSION")).validate_scope(plan.route.scope)?;
    validate_adapter(plan)?;
    let target = resolve_target(plan)?;
    let item_key = format!("{}/{}", target.server, target.tool);

    let identity = item_identity(
        SourceKind::McpTool,
        &plan.route.source.canonical_uri,
        &item_key,
    )?;
    let mut item_metadata = MetadataMap::new();
    item_metadata.insert("mcp_server".to_string(), json!(target.server));
    item_metadata.insert("mcp_tool_name".to_string(), json!(target.tool));

    let item = ManifestItem {
        source_id: plan.route.source.source_id.clone(),
        source_item_key: identity.source_item_key,
        canonical_uri: identity.canonical_uri,
        item_kind: ItemKind::ToolCall,
        content_kind: Some(ContentKind::Structured),
        display_path: Some(item_key),
        parent_key: None,
        size_bytes: None,
        content_hash: None,
        mtime: None,
        version: None,
        fetch_plan: None,
        metadata: item_metadata,
        graph_hints: Vec::new(),
    };

    Ok(SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: SourceGenerationId::from("gen_mcp_tool_discovery"),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items: vec![item],
        created_at: timestamp(),
        metadata: MetadataMap::new(),
    })
}

fn acquire_sync(plan: &SourcePlan, diff: &SourceManifestDiff) -> AdapterResult<SourceAcquisition> {
    validate_adapter(plan)?;
    let manifest_items = diff
        .added
        .iter()
        .chain(diff.modified.iter())
        .cloned()
        .collect::<Vec<_>>();
    let resolved = resolve_metadata(plan)?;
    let content = resolved
        .documents
        .first()
        .map(|doc| doc.content.clone())
        .unwrap_or_default();

    let mut fetched_items = Vec::with_capacity(manifest_items.len());
    for item in &manifest_items {
        fetched_items.push(AcquiredSourceItem {
            manifest_item: item.clone(),
            fetch_status: LifecycleStatus::Completed,
            content_ref: ContentRef::InlineText {
                text: content.clone(),
            },
            raw_artifact_id: None,
            headers: RedactedHeaders {
                headers: Vec::new(),
            },
            fetched_at: timestamp(),
            metadata: MetadataMap::new(),
        });
    }

    let manifest = SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: diff.next_generation.clone(),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items: manifest_items,
        created_at: timestamp(),
        metadata: MetadataMap::new(),
    };

    Ok(SourceAcquisition {
        header: stage_header(
            plan.job_id,
            "mcp_tool_fetch",
            PipelinePhase::Fetching,
            fetched_items.len(),
        ),
        source_id: manifest.source_id.clone(),
        generation: manifest.generation.clone(),
        adapter: manifest.adapter.clone(),
        scope: manifest.scope,
        manifest,
        fetched_items,
        artifacts: Vec::new(),
    })
}

fn normalize_sync(
    plan: &SourcePlan,
    acquisition: SourceAcquisition,
) -> AdapterResult<StageExecutionResult<Vec<SourceDocument>>> {
    validate_adapter(plan)?;
    let target = resolve_target(plan)?;
    let resolved = resolve_metadata(plan)?;
    // "call" only ever appears once `Execute`-mode threading (see the parent
    // module's doc comment) lands and `resolved.tool_call_count` can be
    // nonzero.
    let tool_action = if resolved.tool_call_count > 0 {
        "call"
    } else {
        "metadata"
    };
    let documents = acquisition
        .fetched_items
        .iter()
        .map(|item| mcp_tool_source_document(plan, &acquisition, item, &target, tool_action))
        .collect::<Vec<_>>();
    Ok(StageExecutionResult {
        header: stage_header(
            plan.job_id,
            "mcp_tool_normalize",
            PipelinePhase::Normalizing,
            documents.len(),
        ),
        data: documents,
    })
}

fn validate_adapter(plan: &SourcePlan) -> AdapterResult<()> {
    if plan.route.adapter.name == ADAPTER_NAME {
        return Ok(());
    }
    Err(ApiError::new(
        "adapter.mcp_tool.mismatch",
        axon_error::ErrorStage::Routing,
        "route selected a different adapter",
    )
    .with_context("adapter", plan.route.adapter.name.clone()))
}

fn stage_header(
    job_id: JobId,
    stage_id: &'static str,
    phase: PipelinePhase,
    item_count: usize,
) -> StageResultHeader {
    StageResultHeader {
        job_id,
        stage_id: StageId::new(Uuid::new_v5(&Uuid::NAMESPACE_OID, stage_id.as_bytes())),
        phase,
        status: LifecycleStatus::Completed,
        started_at: timestamp(),
        completed_at: Some(timestamp()),
        counts: StageCounts {
            items_total: Some(item_count as u64),
            items_done: item_count as u64,
            documents_total: Some(item_count as u64),
            documents_done: item_count as u64,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        },
        warnings: Vec::new(),
        error: None,
    }
}

fn timestamp() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}

#[cfg(test)]
#[path = "adapter_tests.rs"]
mod tests;
