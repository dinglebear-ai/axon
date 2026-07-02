//! Reddit source adapter (subreddits / threads).
//!
//! Like the `git` and `local` adapters, this operates on already-materialized
//! content — a `reddit_dump_path` option pointing at a prepared JSON dump of
//! post (and flattened comment) data, produced by the caller (the services
//! bridge performs the Reddit OAuth API calls and comment-tree traversal).
//! Keeping the network out of the adapter makes it unit-testable with fixture
//! dumps and matches how the `git`/`web` adapters read prepared inputs.

mod dump;
mod metadata;
mod target;

use std::fs;
use std::path::PathBuf;

use async_trait::async_trait;
use axon_api::source::*;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::adapter::{Result, SourceAdapter};
use crate::capability::AdapterCapability;
use crate::manifest::item_identity;

use self::dump::{RedditDumpItem, parse_dump};
use self::metadata::reddit_source_document;
pub use self::target::{RedditTarget, parse_reddit_target};

pub const MODULE_NAME: &str = "reddit";

const ADAPTER_NAME: &str = "reddit";

#[derive(Debug, Clone, Default)]
pub struct RedditSourceAdapter;

impl RedditSourceAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SourceAdapter for RedditSourceAdapter {
    fn name(&self) -> &str {
        ADAPTER_NAME
    }

    fn version(&self) -> &str {
        env!("CARGO_PKG_VERSION")
    }

    async fn capabilities(&self) -> Result<AdapterCapability> {
        Ok(reddit_capability(self.version()))
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
        let dump_items = load_dump_items(plan)?;
        let documents = acquisition
            .fetched_items
            .iter()
            .map(|item| {
                let dump_item = dump_item_for(&dump_items, &item.manifest_item)?;
                Ok(reddit_source_document(plan, &acquisition, item, dump_item))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(StageExecutionResult {
            header: stage_header(
                plan.job_id,
                "reddit_normalize",
                PipelinePhase::Normalizing,
                documents.len(),
            ),
            data: documents,
        })
    }
}

fn reddit_capability(version: &str) -> AdapterCapability {
    AdapterCapability::new(
        AdapterRef {
            name: ADAPTER_NAME.to_string(),
            version: version.to_string(),
        },
        SourceKind::Reddit,
        SourceScope::Subreddit,
    )
    .with_scope(SourceScope::Thread)
}

fn discover_sync(plan: &SourcePlan) -> Result<SourceManifest> {
    reddit_capability(env!("CARGO_PKG_VERSION")).validate_scope(plan.route.scope)?;
    validate_adapter(plan)?;
    let target = reddit_target(plan)?;
    let dump_items = load_dump_items(plan)?;

    let mut items = Vec::with_capacity(dump_items.len());
    for dump_item in &dump_items {
        let key = item_key_for(dump_item)?;
        let identity = item_identity(SourceKind::Reddit, "", &key)?;
        let content_hash = content_fingerprint(dump_item);
        let mut item_metadata = MetadataMap::new();
        item_metadata.insert(
            "reddit_permalink".to_string(),
            json!(dump_item.permalink.clone().unwrap_or_default()),
        );
        items.push(ManifestItem {
            source_id: plan.route.source.source_id.clone(),
            source_item_key: identity.source_item_key,
            canonical_uri: dump_item.canonical_url(),
            item_kind: ItemKind::FeedEntry,
            content_kind: Some(ContentKind::PlainText),
            display_path: Some(key),
            parent_key: None,
            size_bytes: Some(dump_item.render_content().len() as u64),
            content_hash: Some(content_hash),
            mtime: None,
            version: None,
            fetch_plan: None,
            metadata: item_metadata,
            graph_hints: Vec::new(),
        });
    }
    items.sort_by(|left, right| left.source_item_key.cmp(&right.source_item_key));

    Ok(SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: SourceGenerationId::from("gen_reddit_discovery"),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items,
        created_at: timestamp(),
        metadata: manifest_metadata(&target),
    })
}

fn acquire_sync(plan: &SourcePlan, diff: &SourceManifestDiff) -> Result<SourceAcquisition> {
    validate_adapter(plan)?;
    let dump_items = load_dump_items(plan)?;
    let manifest_items = diff
        .added
        .iter()
        .chain(diff.modified.iter())
        .cloned()
        .collect::<Vec<_>>();

    let mut fetched_items = Vec::with_capacity(manifest_items.len());
    for item in &manifest_items {
        let dump_item = dump_item_for(&dump_items, item)?;
        fetched_items.push(AcquiredSourceItem {
            manifest_item: item.clone(),
            fetch_status: LifecycleStatus::Completed,
            content_ref: ContentRef::InlineText {
                text: dump_item.render_content(),
            },
            raw_artifact_id: None,
            headers: RedactedHeaders {
                headers: Vec::new(),
            },
            fetched_at: timestamp(),
            metadata: MetadataMap::new(),
        });
    }

    let target = reddit_target(plan)?;
    let manifest = SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: diff.next_generation.clone(),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items: manifest_items,
        created_at: timestamp(),
        metadata: manifest_metadata(&target),
    };
    Ok(SourceAcquisition {
        header: stage_header(
            plan.job_id,
            "reddit_fetch",
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

fn manifest_metadata(target: &RedditTarget) -> MetadataMap {
    let mut metadata = MetadataMap::new();
    match target {
        RedditTarget::Subreddit(name) => {
            metadata.insert("reddit_target_kind".to_string(), json!("subreddit"));
            metadata.insert("reddit_subreddit".to_string(), json!(name));
        }
        RedditTarget::Thread(permalink) => {
            metadata.insert("reddit_target_kind".to_string(), json!("thread"));
            metadata.insert("reddit_permalink".to_string(), json!(permalink));
        }
    }
    metadata
}

/// The prepared JSON dump path, passed by the services bridge as a validated
/// option (mirrors the git adapter's `repo_root` option).
fn dump_path(plan: &SourcePlan) -> Result<PathBuf> {
    plan.route
        .validated_options
        .values
        .get("reddit_dump_path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| {
            ApiError::new(
                "adapter.reddit.dump_path.required",
                axon_error::ErrorStage::Planning,
                "reddit adapter requires a reddit_dump_path option pointing at a prepared JSON dump",
            )
        })
}

fn load_dump_items(plan: &SourcePlan) -> Result<Vec<RedditDumpItem>> {
    let path = dump_path(plan)?;
    let bytes = fs::read(&path).map_err(|err| {
        ApiError::new(
            "adapter.reddit.dump_read_failed",
            axon_error::ErrorStage::Discovering,
            err.to_string(),
        )
        .with_context("path", path.display().to_string())
    })?;
    parse_dump(&bytes)
}

fn item_key_for(item: &RedditDumpItem) -> Result<String> {
    let permalink = item.permalink.clone().unwrap_or_default();
    let trimmed = permalink.trim_matches('/');
    if trimmed.is_empty() {
        return Err(ApiError::new(
            "adapter.reddit.item_key.invalid",
            axon_error::ErrorStage::Discovering,
            "reddit dump item is missing a permalink",
        ));
    }
    Ok(trimmed.to_string())
}

fn dump_item_for<'a>(
    dump_items: &'a [RedditDumpItem],
    manifest_item: &ManifestItem,
) -> Result<&'a RedditDumpItem> {
    let key = manifest_item
        .display_path
        .clone()
        .unwrap_or_else(|| manifest_item.source_item_key.0.clone());
    dump_items
        .iter()
        .find(|candidate| {
            item_key_for(candidate)
                .map(|candidate_key| candidate_key == key)
                .unwrap_or(false)
        })
        .ok_or_else(|| {
            ApiError::new(
                "adapter.reddit.item_missing",
                axon_error::ErrorStage::Fetching,
                "reddit manifest item has no matching dump entry",
            )
            .with_context("key", key)
        })
}

fn content_fingerprint(item: &RedditDumpItem) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(item.render_content().as_bytes());
    self::metadata::hex_prefix(&hasher.finalize(), 16)
}

fn reddit_target(plan: &SourcePlan) -> Result<RedditTarget> {
    parse_reddit_target(&plan.request.source)
}

fn validate_adapter(plan: &SourcePlan) -> Result<()> {
    if plan.route.adapter.name == ADAPTER_NAME {
        return Ok(());
    }
    Err(ApiError::new(
        "adapter.reddit.mismatch",
        axon_error::ErrorStage::Routing,
        "route selected a different adapter",
    )
    .with_context("adapter", plan.route.adapter.name.clone()))
}

fn blocking_join_error(err: tokio::task::JoinError) -> ApiError {
    ApiError::new(
        "adapter.reddit.blocking_task_failed",
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

#[cfg(test)]
#[path = "reddit_tests.rs"]
mod tests;
