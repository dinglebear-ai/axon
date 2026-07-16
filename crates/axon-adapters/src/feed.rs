//! RSS/Atom/JSON feed source adapter.
//!
//! [`FeedSourceAdapter::materialize`] owns the SSRF-guarded, bounded fetch and
//! stamps the prepared feed path on the routed plan before discovery.

mod acquire;
mod metadata;
mod parse;
mod target;

use std::fs;

use async_trait::async_trait;
use axon_api::source::*;
use feed_rs::model::Feed;
use serde_json::json;
use uuid::Uuid;

use crate::adapter::{Result, SourceAdapter};
use crate::capability::AdapterCapability;
use crate::manifest::item_identity;

pub use self::acquire::fetch_feed_to_file;
use self::metadata::feed_source_document;
use self::parse::{FeedEntry, extract_entries, parse_feed_bytes};
pub use self::target::{feed_path, html_to_text};

pub const MODULE_NAME: &str = "feed";

const ADAPTER_NAME: &str = "feed";

#[derive(Debug, Clone, Default)]
pub struct FeedSourceAdapter;

impl FeedSourceAdapter {
    pub fn new() -> Self {
        Self
    }

    pub async fn materialize(
        &self,
        mut plan: SourcePlan,
    ) -> Result<crate::acquisition::MaterializedSource> {
        validate_adapter(&plan)?;
        let path = fetch_feed_to_file(&plan.request.source)
            .await
            .map_err(|err| {
                crate::acquisition::materialization_error(
                    "adapter.feed.fetch_failed",
                    err.to_string(),
                )
            })?;
        plan.route
            .validated_options
            .values
            .insert("feed_path".to_string(), json!(path.to_string_lossy()));
        Ok(crate::acquisition::MaterializedSource::persistent(
            plan, path,
        ))
    }
}

#[async_trait]
impl SourceAdapter for FeedSourceAdapter {
    fn name(&self) -> &'static str {
        ADAPTER_NAME
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    async fn capabilities(&self) -> Result<SourceAdapterCapability> {
        Ok(feed_capability(self.version()).into())
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
        let feed_title = acquisition
            .manifest
            .metadata
            .get("feed_title")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let feed_link = acquisition
            .manifest
            .metadata
            .get("feed_link")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let documents = acquisition
            .fetched_items
            .iter()
            .map(|item| {
                feed_source_document(
                    plan,
                    feed_title.as_deref(),
                    feed_link.as_deref(),
                    &acquisition,
                    item,
                )
            })
            .collect::<Vec<_>>();
        Ok(StageExecutionResult {
            header: stage_header(
                plan.job_id,
                "feed_normalize",
                PipelinePhase::Normalizing,
                documents.len(),
            ),
            data: documents,
        })
    }
}

fn feed_capability(version: &str) -> AdapterCapability {
    AdapterCapability::new(
        AdapterRef {
            name: ADAPTER_NAME.to_string(),
            version: version.to_string(),
        },
        SourceKind::Feed,
        SourceScope::Feed,
    )
}

fn discover_sync(plan: &SourcePlan) -> Result<SourceManifest> {
    feed_capability(env!("CARGO_PKG_VERSION")).validate_scope(plan.route.scope)?;
    validate_adapter(plan)?;
    let path = feed_path(plan)?;
    let feed = read_and_parse_feed(&path)?;

    let base_uri = plan
        .route
        .source
        .canonical_uri
        .trim_end_matches('/')
        .to_string();
    let entries = extract_entries(&feed);
    let mut items = Vec::with_capacity(entries.len());
    for entry in &entries {
        items.push(manifest_item_for_entry(plan, &base_uri, entry)?);
    }
    items.sort_by(|left, right| left.source_item_key.cmp(&right.source_item_key));

    Ok(SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: SourceGenerationId::from("gen_feed_discovery"),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items,
        created_at: timestamp(),
        metadata: manifest_metadata(&feed),
    })
}

fn acquire_sync(plan: &SourcePlan, diff: &SourceManifestDiff) -> Result<SourceAcquisition> {
    validate_adapter(plan)?;
    let path = feed_path(plan)?;
    let feed = read_and_parse_feed(&path)?;
    let entries_by_link = index_entries_by_link(&feed);

    let manifest_items = diff
        .added
        .iter()
        .chain(diff.modified.iter())
        .cloned()
        .collect::<Vec<_>>();
    let mut fetched_items = Vec::with_capacity(manifest_items.len());
    for item in &manifest_items {
        let link = item.canonical_uri.clone();
        let Some(entry) = entries_by_link.get(link.as_str()) else {
            continue;
        };
        let text = html_to_text(&entry.body_html);
        fetched_items.push(AcquiredSourceItem {
            manifest_item: item.clone(),
            fetch_status: LifecycleStatus::Completed,
            content_ref: ContentRef::InlineText { text },
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
        metadata: manifest_metadata(&feed),
    };
    Ok(SourceAcquisition {
        header: stage_header(
            plan.job_id,
            "feed_fetch",
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

fn read_and_parse_feed(path: &std::path::Path) -> Result<Feed> {
    let bytes = fs::read(path).map_err(|err| {
        ApiError::new(
            "adapter.feed.read_failed",
            axon_error::ErrorStage::Fetching,
            err.to_string(),
        )
        .with_context("path", path.display().to_string())
    })?;
    parse_feed_bytes(&bytes).map_err(|err| {
        ApiError::new(
            "adapter.feed.parse_failed",
            axon_error::ErrorStage::Discovering,
            err,
        )
        .with_context("path", path.display().to_string())
    })
}

fn manifest_item_for_entry(
    plan: &SourcePlan,
    base_uri: &str,
    entry: &FeedEntry,
) -> Result<ManifestItem> {
    let identity = item_identity(SourceKind::Feed, base_uri, &entry.link)?;
    let mut item_metadata = MetadataMap::new();
    item_metadata.insert("feed_entry_id".to_string(), json!(entry.entry_id));
    item_metadata.insert("feed_entry_link".to_string(), json!(entry.link));
    if let Some(published) = &entry.published {
        item_metadata.insert("feed_entry_published".to_string(), json!(published));
    }
    if let Some(author) = &entry.author {
        item_metadata.insert("feed_entry_author".to_string(), json!(author));
    }
    Ok(ManifestItem {
        source_id: plan.route.source.source_id.clone(),
        source_item_key: identity.source_item_key,
        // The entry's own link is the canonical identity of a feed entry, not
        // a path joined onto the feed's canonical URI.
        canonical_uri: entry.link.clone(),
        item_kind: ItemKind::FeedEntry,
        content_kind: Some(ContentKind::PlainText),
        display_path: entry.title.clone(),
        parent_key: None,
        size_bytes: Some(entry.body_html.len() as u64),
        content_hash: None,
        mtime: None,
        version: None,
        fetch_plan: None,
        metadata: item_metadata,
        graph_hints: Vec::new(),
    })
}

fn index_entries_by_link(feed: &Feed) -> std::collections::HashMap<String, FeedEntry> {
    extract_entries(feed)
        .into_iter()
        .map(|entry| (entry.link.clone(), entry))
        .collect()
}

fn manifest_metadata(feed: &Feed) -> MetadataMap {
    let mut metadata = MetadataMap::new();
    metadata.insert("source_kind".to_string(), json!("feed"));
    if let Some(title) = feed.title.as_ref().map(|t| t.content.clone()) {
        metadata.insert("feed_title".to_string(), json!(title));
    }
    if let Some(link) = feed.links.first().map(|l| l.href.clone()) {
        metadata.insert("feed_link".to_string(), json!(link));
    }
    metadata
}

fn validate_adapter(plan: &SourcePlan) -> Result<()> {
    if plan.route.adapter.name == ADAPTER_NAME {
        return Ok(());
    }
    Err(ApiError::new(
        "adapter.feed.mismatch",
        axon_error::ErrorStage::Routing,
        "route selected a different adapter",
    )
    .with_context("adapter", plan.route.adapter.name.clone()))
}

fn blocking_join_error(err: tokio::task::JoinError) -> ApiError {
    ApiError::new(
        "adapter.feed.blocking_task_failed",
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

pub(crate) fn hex_prefix(digest: &[u8], hex_chars: usize) -> String {
    use std::fmt::Write as _;
    let mut token = String::with_capacity(hex_chars);
    for byte in &digest[..(hex_chars / 2).min(digest.len())] {
        let _ = write!(&mut token, "{byte:02x}");
    }
    token
}

#[cfg(test)]
#[path = "feed_tests.rs"]
mod tests;
