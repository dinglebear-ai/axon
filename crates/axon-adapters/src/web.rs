//! Web page/site/docs source adapter.

mod metadata;
mod url_parts;

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use axon_api::source::*;
use serde_json::Value;
use url_parts::WebUrlParts;
use uuid::Uuid;

use crate::adapter::{Result, SourceAdapter};
use crate::capability::AdapterCapability;
use crate::manifest::item_identity;

use self::metadata::{manifest_metadata, web_metadata, web_source_document};

pub const MODULE_NAME: &str = "web";

const ADAPTER_NAME: &str = "web";

#[derive(Debug, Clone, Default)]
pub struct WebSourceAdapter;

impl WebSourceAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SourceAdapter for WebSourceAdapter {
    fn name(&self) -> &'static str {
        ADAPTER_NAME
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    async fn capabilities(&self) -> Result<SourceAdapterCapability> {
        Ok(web_capability(self.version()).into())
    }

    async fn discover(&self, plan: &SourcePlan) -> Result<SourceManifest> {
        let plan = plan.clone();
        tokio::task::spawn_blocking(move || discover_sync(&plan))
            .await
            .map_err(blocking_join_error)?
    }

    async fn acquire(
        &self,
        plan: &SourcePlan,
        diff: &SourceManifestDiff,
    ) -> Result<SourceAcquisition> {
        let plan = plan.clone();
        let diff = diff.clone();
        tokio::task::spawn_blocking(move || acquire_sync(&plan, &diff))
            .await
            .map_err(blocking_join_error)?
    }

    async fn normalize(
        &self,
        plan: &SourcePlan,
        acquisition: SourceAcquisition,
    ) -> Result<StageExecutionResult<Vec<SourceDocument>>> {
        validate_adapter(plan)?;
        let documents = acquisition
            .fetched_items
            .iter()
            .map(|item| web_source_document(plan, &acquisition, item))
            .collect::<Vec<_>>();
        Ok(StageExecutionResult {
            header: stage_header(
                plan.job_id,
                "web_normalize",
                PipelinePhase::Normalizing,
                documents.len(),
            ),
            data: documents,
        })
    }
}

fn web_capability(version: &str) -> AdapterCapability {
    AdapterCapability::new(
        AdapterRef {
            name: ADAPTER_NAME.to_string(),
            version: version.to_string(),
        },
        SourceKind::Web,
        SourceScope::Site,
    )
    .with_scope(SourceScope::Page)
    .with_scope(SourceScope::Docs)
    .with_scope(SourceScope::Map)
}

fn discover_sync(plan: &SourcePlan) -> Result<SourceManifest> {
    web_capability(env!("CARGO_PKG_VERSION")).validate_scope(plan.route.scope)?;
    validate_adapter(plan)?;
    let items = if plan.route.scope == SourceScope::Map {
        map_manifest_items(plan)?
    } else {
        crawl_manifest_items(plan)?
    };
    Ok(SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: SourceGenerationId::from("gen_web_discovery"),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items,
        created_at: timestamp(),
        metadata: manifest_metadata(plan),
    })
}

fn acquire_sync(plan: &SourcePlan, diff: &SourceManifestDiff) -> Result<SourceAcquisition> {
    validate_adapter(plan)?;
    if plan.route.scope == SourceScope::Map {
        return Ok(SourceAcquisition {
            header: stage_header(plan.job_id, "web_fetch", PipelinePhase::Fetching, 0),
            source_id: plan.route.source.source_id.clone(),
            generation: diff.next_generation.clone(),
            adapter: plan.route.adapter.clone(),
            scope: plan.route.scope,
            manifest: diff_manifest(plan, diff, Vec::new()),
            fetched_items: Vec::new(),
            artifacts: Vec::new(),
        });
    }

    let manifest_items = diff
        .added
        .iter()
        .chain(diff.modified.iter())
        .cloned()
        .collect::<Vec<_>>();
    let markdown_root = option_path(plan, "markdown_root")?;
    let mut fetched_items = Vec::with_capacity(manifest_items.len());
    for item in &manifest_items {
        let relative_path = string_metadata(&item.metadata, "crawl_relative_path")?;
        let path = safe_join(&markdown_root, relative_path)?;
        let text = fs::read_to_string(&path).map_err(|err| {
            ApiError::new(
                "adapter.web.read_failed",
                axon_error::ErrorStage::Fetching,
                err.to_string(),
            )
            .with_context("path", relative_path.to_string())
        })?;
        let mut metadata = item.metadata.clone();
        metadata.insert(
            "web_fetch_method".to_string(),
            serde_json::json!("crawl_manifest"),
        );
        fetched_items.push(AcquiredSourceItem {
            manifest_item: item.clone(),
            fetch_status: LifecycleStatus::Completed,
            content_ref: ContentRef::InlineText { text },
            raw_artifact_id: None,
            headers: RedactedHeaders {
                headers: Vec::new(),
            },
            fetched_at: timestamp(),
            metadata,
        });
    }

    Ok(SourceAcquisition {
        header: stage_header(
            plan.job_id,
            "web_fetch",
            PipelinePhase::Fetching,
            fetched_items.len(),
        ),
        source_id: plan.route.source.source_id.clone(),
        generation: diff.next_generation.clone(),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        manifest: diff_manifest(plan, diff, manifest_items),
        fetched_items,
        artifacts: Vec::new(),
    })
}

fn map_manifest_items(plan: &SourcePlan) -> Result<Vec<ManifestItem>> {
    let urls = plan
        .route
        .validated_options
        .values
        .get("map_urls")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ApiError::new(
                "adapter.web.map_urls.required",
                axon_error::ErrorStage::Discovering,
                "web map scope requires map_urls acquisition results",
            )
        })?;
    let mut items = Vec::with_capacity(urls.len());
    for url in urls {
        let raw = url.as_str().ok_or_else(|| {
            ApiError::new(
                "adapter.web.map_url.invalid",
                axon_error::ErrorStage::Discovering,
                "map_urls entries must be strings",
            )
        })?;
        let web = WebUrlParts::parse(raw)?;
        items.push(web_manifest_item(plan, &web, None, None, None, None));
    }
    items.sort_by(|left, right| left.source_item_key.cmp(&right.source_item_key));
    Ok(items)
}

fn crawl_manifest_items(plan: &SourcePlan) -> Result<Vec<ManifestItem>> {
    let manifest_path = option_path(plan, "manifest_path")?;
    let file = fs::File::open(&manifest_path).map_err(|err| {
        ApiError::new(
            "adapter.web.manifest_read_failed",
            axon_error::ErrorStage::Discovering,
            err.to_string(),
        )
        .with_context("path", manifest_path.display().to_string())
    })?;
    let mut items = Vec::new();
    for (idx, line) in BufReader::new(file).lines().enumerate() {
        let line = line.map_err(|err| {
            ApiError::new(
                "adapter.web.manifest_read_failed",
                axon_error::ErrorStage::Discovering,
                err.to_string(),
            )
            .with_context("line", (idx + 1).to_string())
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: Value = serde_json::from_str(&line).map_err(|err| {
            ApiError::new(
                "adapter.web.manifest_invalid",
                axon_error::ErrorStage::Discovering,
                err.to_string(),
            )
            .with_context("line", (idx + 1).to_string())
        })?;
        let raw_url = entry
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| manifest_field_error(idx, "url"))?;
        let relative_path = entry
            .get("relative_path")
            .and_then(Value::as_str)
            .ok_or_else(|| manifest_field_error(idx, "relative_path"))?;
        let content_hash = entry
            .get("content_hash")
            .and_then(Value::as_str)
            .map(str::to_string);
        let size_bytes = entry.get("markdown_chars").and_then(Value::as_u64);
        let structured = entry.get("structured").cloned();
        let web = WebUrlParts::parse(raw_url)?;
        items.push(web_manifest_item(
            plan,
            &web,
            Some(relative_path),
            content_hash,
            size_bytes,
            structured,
        ));
    }
    items.sort_by(|left, right| left.source_item_key.cmp(&right.source_item_key));
    Ok(items)
}

fn web_manifest_item(
    plan: &SourcePlan,
    web: &WebUrlParts,
    relative_path: Option<&str>,
    content_hash: Option<String>,
    size_bytes: Option<u64>,
    structured: Option<Value>,
) -> ManifestItem {
    let identity = item_identity(SourceKind::Web, "", &web.item_key)
        .expect("web item key is derived from a validated URL");
    let mut metadata = web_metadata(plan, web);
    metadata.insert("content_hash".to_string(), serde_json::json!(content_hash));
    if let Some(relative_path) = relative_path {
        metadata.insert(
            "crawl_relative_path".to_string(),
            serde_json::json!(relative_path),
        );
    }
    if let Some(structured) =
        structured.and_then(|payload| bounded_structured_payload(payload, &mut metadata))
    {
        metadata.insert("structured_payload".to_string(), structured);
    }
    ManifestItem {
        source_id: plan.route.source.source_id.clone(),
        source_item_key: identity.source_item_key,
        canonical_uri: web.normalized_url.clone(),
        item_kind: ItemKind::WebPage,
        content_kind: Some(ContentKind::Markdown),
        display_path: Some(web.item_key.clone()),
        parent_key: None,
        size_bytes,
        content_hash,
        mtime: None,
        version: None,
        fetch_plan: None,
        metadata,
        graph_hints: Vec::new(),
    }
}

fn bounded_structured_payload(structured: Value, metadata: &mut MetadataMap) -> Option<Value> {
    const MAX_STRUCTURED_PAYLOAD_BYTES: usize = 64 * 1024;
    let size = serde_json::to_vec(&structured)
        .map(|bytes| bytes.len())
        .unwrap_or(MAX_STRUCTURED_PAYLOAD_BYTES + 1);
    if size <= MAX_STRUCTURED_PAYLOAD_BYTES {
        Some(structured)
    } else {
        metadata.insert(
            "structured_payload_omitted".to_string(),
            serde_json::json!("too_large"),
        );
        None
    }
}

fn diff_manifest(
    plan: &SourcePlan,
    diff: &SourceManifestDiff,
    items: Vec<ManifestItem>,
) -> SourceManifest {
    SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: diff.next_generation.clone(),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items,
        created_at: timestamp(),
        metadata: manifest_metadata(plan),
    }
}

fn validate_adapter(plan: &SourcePlan) -> Result<()> {
    if plan.route.adapter.name == ADAPTER_NAME {
        return Ok(());
    }
    Err(ApiError::new(
        "adapter.web.mismatch",
        axon_error::ErrorStage::Routing,
        "route selected a different adapter",
    )
    .with_context("adapter", plan.route.adapter.name.clone()))
}

fn option_path(plan: &SourcePlan, key: &'static str) -> Result<PathBuf> {
    plan.route
        .validated_options
        .values
        .get(key)
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| {
            ApiError::new(
                format!("adapter.web.{key}.required"),
                axon_error::ErrorStage::Planning,
                format!("web adapter requires {key} option"),
            )
        })
}

fn string_metadata<'a>(metadata: &'a MetadataMap, key: &'static str) -> Result<&'a str> {
    metadata.get(key).and_then(Value::as_str).ok_or_else(|| {
        ApiError::new(
            format!("adapter.web.{key}.missing"),
            axon_error::ErrorStage::Fetching,
            format!("web manifest item is missing {key} metadata"),
        )
    })
}

fn manifest_field_error(line: usize, field: &'static str) -> ApiError {
    ApiError::new(
        "adapter.web.manifest_field_missing",
        axon_error::ErrorStage::Discovering,
        format!("crawl manifest entry missing {field}"),
    )
    .with_context("line", (line + 1).to_string())
}

fn safe_join(root: &Path, relative_path: &str) -> Result<PathBuf> {
    let path = Path::new(relative_path);
    if path.is_absolute() || relative_path.split('/').any(|part| part == "..") {
        return Err(ApiError::new(
            "adapter.web.path.escape",
            axon_error::ErrorStage::Fetching,
            "crawl manifest relative_path must stay inside markdown_root",
        )
        .with_context("relative_path", relative_path.to_string()));
    }
    Ok(root.join(path))
}

fn blocking_join_error(err: tokio::task::JoinError) -> ApiError {
    ApiError::new(
        "adapter.web.blocking_task_failed",
        axon_error::ErrorStage::Planning,
        err.to_string(),
    )
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

pub(crate) fn timestamp() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}
