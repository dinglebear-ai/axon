//! `SourceAdapter` wiring for `mcp://server/tool` sources: discover/acquire/
//! normalize built on top of the metadata-only-by-default
//! `resolve_and_acquire` contract in the parent `mcp_tool` module.

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
    CommandMcpToolCaller, McpExecutionMode, McpToolAcquireResult, McpToolDocument, McpToolTarget,
    RedactionStatus, parse_mcp_target, resolve_and_acquire,
};

const ADAPTER_NAME: &str = "mcp_tool";

/// Real `SourceAdapter` wiring for `mcp://server/tool` sources.
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
        discover_plan(plan)
    }

    async fn acquire(
        &self,
        plan: &SourcePlan,
        diff: &SourceManifestDiff,
    ) -> AdapterResult<SourceAcquisition> {
        acquire_plan(plan, diff).await
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
/// resolving any content. Used by `discover`/`normalize` to name the manifest
/// item/document.
fn resolve_target(plan: &SourcePlan) -> AdapterResult<McpToolTarget> {
    parse_mcp_target(&plan.request.source)
        .map_err(|err| ApiError::new(err.code, axon_error::ErrorStage::Planning, err.message))
}

async fn resolve_for_acquire(plan: &SourcePlan) -> AdapterResult<McpToolAcquireResult> {
    let mode = execution_mode(plan);
    let has_execute_scope = plan
        .request
        .metadata
        .0
        .get("tool_execute_authorized")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let allowlist = mcp_allowlist(plan);
    let caller = command_caller(plan);
    resolve_and_acquire(
        &plan.request.source,
        mode,
        has_execute_scope,
        &allowlist
            .iter()
            .map(|(server, tool)| (server.as_str(), tool.as_str()))
            .collect::<Vec<_>>(),
        caller
            .as_ref()
            .map(|caller| caller as &dyn super::McpToolCaller),
    )
    .await
    .map_err(|err| ApiError::new(err.code, axon_error::ErrorStage::Authorizing, err.message))
}

fn discover_plan(plan: &SourcePlan) -> AdapterResult<SourceManifest> {
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
        content_hash: execution_content_hash(plan),
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

async fn acquire_plan(
    plan: &SourcePlan,
    diff: &SourceManifestDiff,
) -> AdapterResult<SourceAcquisition> {
    validate_adapter(plan)?;
    let manifest_items = diff
        .added
        .iter()
        .chain(diff.modified.iter())
        .cloned()
        .collect::<Vec<_>>();
    let resolved = resolve_for_acquire(plan).await?;
    let content = resolved
        .documents
        .first()
        .map(|doc| doc.content.clone())
        .unwrap_or_default();
    let document = resolved.documents.first();
    let tool_action = if resolved.tool_call_count > 0 {
        "call"
    } else {
        "metadata"
    };

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
            metadata: item_metadata(document, tool_action, resolved.redaction_status),
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
    let documents = acquisition
        .fetched_items
        .iter()
        .map(|item| {
            let tool_action = item
                .metadata
                .0
                .get("tool_action")
                .and_then(serde_json::Value::as_str)
                .filter(|action| *action == "call")
                .unwrap_or("metadata");
            mcp_tool_source_document(plan, &acquisition, item, &target, tool_action)
        })
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

fn item_metadata(
    document: Option<&McpToolDocument>,
    tool_action: &'static str,
    redaction_status: RedactionStatus,
) -> MetadataMap {
    let mut metadata = MetadataMap::new();
    metadata.insert("tool_action".to_string(), json!(tool_action));
    if document.is_some() {
        metadata.insert(
            "redaction_status".to_string(),
            json!(match redaction_status {
                RedactionStatus::Clean => "clean",
                RedactionStatus::Redacted => "redacted",
            }),
        );
    }
    metadata
}

fn execution_mode(plan: &SourcePlan) -> McpExecutionMode {
    let requested = option_string_any(plan, &["execution_mode", "tool_action"])
        .is_some_and(|mode| matches!(mode.as_str(), "call" | "invoke" | "execute" | "exec"))
        || bool_option(plan, "call").unwrap_or(false);
    if requested {
        McpExecutionMode::Execute
    } else {
        McpExecutionMode::MetadataOnly
    }
}

fn mcp_allowlist(plan: &SourcePlan) -> Vec<(String, String)> {
    policy_string_list(plan, "mcp_allowlist")
        .into_iter()
        .filter_map(|entry| {
            let (server, tool) = entry.split_once('/')?;
            Some((server.to_string(), tool.to_string()))
        })
        .collect()
}

fn command_caller(plan: &SourcePlan) -> Option<CommandMcpToolCaller> {
    Some(CommandMcpToolCaller {
        command: policy_string(plan, "mcp_caller_command")?,
        env_allowlist: policy_string_list(plan, "env_allowlist"),
        timeout_ms: policy_u64(plan, "timeout_ms").unwrap_or(5_000),
        output_cap_bytes: policy_u64(plan, "output_cap_bytes").unwrap_or(64 * 1024) as usize,
    })
}

fn execution_content_hash(plan: &SourcePlan) -> Option<String> {
    plan.request
        .metadata
        .0
        .get("tool_execute_authorized")
        .and_then(serde_json::Value::as_bool)
        .filter(|authorized| *authorized)
        .map(|_| format!("tool-execution:{}", plan.job_id.0))
}

fn policy_value<'a>(plan: &'a SourcePlan, key: &str) -> Option<&'a serde_json::Value> {
    plan.request
        .metadata
        .0
        .get("tool_execution_policy")?
        .as_object()?
        .get(key)
}

fn policy_string(plan: &SourcePlan, key: &str) -> Option<String> {
    policy_value(plan, key)?.as_str().map(str::to_string)
}

fn policy_string_list(plan: &SourcePlan, key: &str) -> Vec<String> {
    policy_value(plan, key)
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_string)
        .collect()
}

fn policy_u64(plan: &SourcePlan, key: &str) -> Option<u64> {
    policy_value(plan, key)?.as_u64()
}

fn option_string_any(plan: &SourcePlan, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| string_option(plan, key))
}

fn string_option(plan: &SourcePlan, key: &str) -> Option<String> {
    plan.request
        .options
        .values
        .0
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn bool_option(plan: &SourcePlan, key: &str) -> Option<bool> {
    plan.request
        .options
        .values
        .0
        .get(key)
        .and_then(serde_json::Value::as_bool)
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
