//! YouTube source adapter (videos, playlists, channels + transcripts).
//!
//! [`YoutubeSourceAdapter::materialize`] owns target validation, the bounded
//! `yt-dlp` subprocess, and prepared-dump creation before discovery.

mod acquire;
pub mod dump;
mod metadata;
mod target;

use std::path::PathBuf;

use async_trait::async_trait;
use axon_api::source::*;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::adapter::{Result, SourceAdapter};
use crate::capability::AdapterCapability;
use crate::manifest::item_identity;

pub use self::acquire::fetch_youtube_dump;
use self::dump::{YoutubeVideoDump, read_youtube_dump};
use self::metadata::{youtube_manifest_metadata, youtube_source_document};
pub use self::target::{
    YoutubeTarget, extract_video_id, is_playlist_or_channel_url, parse_youtube_target,
};

pub const MODULE_NAME: &str = "youtube";

const ADAPTER_NAME: &str = "youtube";

#[derive(Debug, Clone, Default)]
pub struct YoutubeSourceAdapter;

impl YoutubeSourceAdapter {
    pub fn new() -> Self {
        Self
    }

    pub async fn materialize(
        &self,
        mut plan: SourcePlan,
    ) -> Result<crate::acquisition::MaterializedSource> {
        validate_adapter(&plan)?;
        let (temporary, path) = acquire::fetch_youtube_dump_to_temporary_file(&plan.request.source)
            .await
            .map_err(|err| {
                crate::acquisition::materialization_error(
                    "adapter.youtube.fetch_failed",
                    err.to_string(),
                )
            })?;
        plan.route.validated_options.values.insert(
            "youtube_dump_path".to_string(),
            json!(path.to_string_lossy()),
        );
        Ok(crate::acquisition::MaterializedSource::temporary_at(
            plan, temporary, path,
        ))
    }
}

#[async_trait]
impl SourceAdapter for YoutubeSourceAdapter {
    fn name(&self) -> &'static str {
        ADAPTER_NAME
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    async fn capabilities(&self) -> Result<SourceAdapterCapability> {
        Ok(youtube_capability(self.version()).into())
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
        let videos = dump_by_video_id(plan)?;
        let documents = acquisition
            .fetched_items
            .iter()
            .map(|item| {
                let video_id = video_id_for_item(item)?;
                let video = videos.get(&video_id).ok_or_else(|| {
                    ApiError::new(
                        "adapter.youtube.normalize.video_missing",
                        axon_error::ErrorStage::Normalizing,
                        "acquired item has no matching video in the youtube dump",
                    )
                    .with_context("video_id", video_id.clone())
                })?;
                Ok(youtube_source_document(plan, &acquisition, item, video))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(StageExecutionResult {
            header: stage_header(
                plan.job_id,
                "youtube_normalize",
                PipelinePhase::Normalizing,
                documents.len(),
            ),
            data: documents,
        })
    }
}

fn youtube_capability(version: &str) -> AdapterCapability {
    AdapterCapability::new(
        AdapterRef {
            name: ADAPTER_NAME.to_string(),
            version: version.to_string(),
        },
        SourceKind::Youtube,
        SourceScope::Video,
    )
    .with_scope(SourceScope::Channel)
}

fn discover_sync(plan: &SourcePlan) -> Result<SourceManifest> {
    youtube_capability(env!("CARGO_PKG_VERSION")).validate_scope(plan.route.scope)?;
    validate_adapter(plan)?;
    let path = dump_path(plan)?;
    let videos = read_youtube_dump(&path)?;

    let mut items = Vec::with_capacity(videos.len());
    for video in &videos {
        let canonical_video_url = format!("https://www.youtube.com/watch?v={}", video.video_id);
        let identity = item_identity(
            SourceKind::Youtube,
            "https://www.youtube.com",
            &video.video_id,
        )?;
        let mut item_metadata = MetadataMap::new();
        item_metadata.insert("youtube_video_id".to_string(), json!(video.video_id));
        item_metadata.insert(
            "youtube_canonical_url".to_string(),
            json!(canonical_video_url),
        );
        let content_len = video.transcript.len() as u64 + video.description.len() as u64;
        items.push(ManifestItem {
            source_id: plan.route.source.source_id.clone(),
            source_item_key: identity.source_item_key,
            canonical_uri: canonical_video_url,
            item_kind: ItemKind::Transcript,
            content_kind: Some(ContentKind::Transcript),
            display_path: Some(video.video_id.clone()),
            parent_key: None,
            size_bytes: Some(content_len),
            content_hash: None,
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
        generation: SourceGenerationId::from("gen_youtube_discovery"),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items,
        created_at: timestamp(),
        metadata: youtube_manifest_metadata(plan.route.scope),
    })
}

fn acquire_sync(plan: &SourcePlan, diff: &SourceManifestDiff) -> Result<SourceAcquisition> {
    validate_adapter(plan)?;
    let path = dump_path(plan)?;
    let videos = dump_index(&read_youtube_dump(&path)?);
    let manifest_items = diff
        .added
        .iter()
        .chain(diff.modified.iter())
        .cloned()
        .collect::<Vec<_>>();
    let mut fetched_items = Vec::with_capacity(manifest_items.len());
    for item in &manifest_items {
        let video_id = video_id_for_manifest_item(item)?;
        let video = videos.get(&video_id).ok_or_else(|| {
            ApiError::new(
                "adapter.youtube.acquire.video_missing",
                axon_error::ErrorStage::Fetching,
                "manifest item has no matching video in the youtube dump",
            )
            .with_context("video_id", video_id.clone())
        })?;
        fetched_items.push(AcquiredSourceItem {
            manifest_item: item.clone(),
            fetch_status: LifecycleStatus::Completed,
            content_ref: ContentRef::InlineText {
                text: video.transcript.clone(),
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
        metadata: youtube_manifest_metadata(plan.route.scope),
    };
    Ok(SourceAcquisition {
        header: stage_header(
            plan.job_id,
            "youtube_fetch",
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

fn video_id_for_item(item: &AcquiredSourceItem) -> Result<String> {
    video_id_for_manifest_item(&item.manifest_item)
}

fn video_id_for_manifest_item(item: &ManifestItem) -> Result<String> {
    item.metadata
        .get("youtube_video_id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| item.display_path.clone())
        .ok_or_else(|| {
            ApiError::new(
                "adapter.youtube.video_id.missing",
                axon_error::ErrorStage::Normalizing,
                "manifest item is missing a youtube_video_id",
            )
        })
}

fn dump_index(videos: &[YoutubeVideoDump]) -> std::collections::HashMap<String, YoutubeVideoDump> {
    videos
        .iter()
        .cloned()
        .map(|video| (video.video_id.clone(), video))
        .collect()
}

fn dump_by_video_id(
    plan: &SourcePlan,
) -> Result<std::collections::HashMap<String, YoutubeVideoDump>> {
    let path = dump_path(plan)?;
    Ok(dump_index(&read_youtube_dump(&path)?))
}

/// The prepared dump file, passed by the services bridge as a validated
/// option — mirrors the git adapter's `repo_root` option.
fn dump_path(plan: &SourcePlan) -> Result<PathBuf> {
    plan.route
        .validated_options
        .values
        .get("youtube_dump_path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| {
            ApiError::new(
                "adapter.youtube.youtube_dump_path.required",
                axon_error::ErrorStage::Planning,
                "youtube adapter requires a youtube_dump_path option pointing at a prepared metadata+transcript dump",
            )
        })
}

fn validate_adapter(plan: &SourcePlan) -> Result<()> {
    if plan.route.adapter.name == ADAPTER_NAME {
        return Ok(());
    }
    Err(ApiError::new(
        "adapter.youtube.mismatch",
        axon_error::ErrorStage::Routing,
        "route selected a different adapter",
    )
    .with_context("adapter", plan.route.adapter.name.clone()))
}

fn blocking_join_error(err: tokio::task::JoinError) -> ApiError {
    ApiError::new(
        "adapter.youtube.blocking_task_failed",
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
#[path = "youtube_tests.rs"]
mod tests;
