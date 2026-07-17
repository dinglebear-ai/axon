//! YouTube `SourceDocument` construction — stamps only approved YouTube
//! metadata fields onto normalized documents, mirroring the legacy
//! `yt_*` payload fields built by `axon-ingest::youtube::meta`.

use axon_api::source::*;
use serde_json::json;
use sha2::{Digest, Sha256};

use super::dump::YoutubeVideoDump;
use super::hex_prefix;

pub(super) fn youtube_manifest_metadata(target_scope: SourceScope) -> MetadataMap {
    let mut metadata = MetadataMap::new();
    metadata.insert("source_kind".to_string(), json!("youtube"));
    metadata.insert("source_scope".to_string(), json!(target_scope));
    metadata
}

pub(super) fn youtube_source_document(
    plan: &SourcePlan,
    acquisition: &SourceAcquisition,
    item: &AcquiredSourceItem,
    video: &YoutubeVideoDump,
) -> SourceDocument {
    let video_url = format!("https://www.youtube.com/watch?v={}", video.video_id);
    let mut metadata = MetadataMap::new();
    metadata.insert("source_family".to_string(), json!("media"));
    metadata.insert("source_kind".to_string(), json!("youtube"));
    metadata.insert("source_adapter".to_string(), json!(plan.route.adapter.name));
    metadata.insert("source_scope".to_string(), json!(plan.route.scope));
    metadata.insert("video_id".to_string(), json!(video.video_id));
    metadata.insert("title".to_string(), json!(video.title));
    metadata.insert("media_url".to_string(), json!(video_url));
    if !video.channel.is_empty() {
        metadata.insert("channel".to_string(), json!(video.channel));
    }
    if !video.channel_url.is_empty() {
        metadata.insert("channel_url".to_string(), json!(video.channel_url));
    }
    if !video.uploader_id.is_empty() {
        metadata.insert("yt_uploader_id".to_string(), json!(video.uploader_id));
    }
    if !video.upload_date.is_empty() {
        metadata.insert("yt_upload_date".to_string(), json!(video.upload_date));
    }
    if !video.duration_string.is_empty() {
        metadata.insert("yt_duration".to_string(), json!(video.duration_string));
    }
    if let Some(view_count) = video.view_count {
        metadata.insert("yt_view_count".to_string(), json!(view_count));
    }
    if let Some(like_count) = video.like_count {
        metadata.insert("yt_like_count".to_string(), json!(like_count));
    }
    if !video.tags.is_empty() {
        metadata.insert("yt_tags".to_string(), json!(video.tags));
    }
    if !video.categories.is_empty() {
        metadata.insert("yt_categories".to_string(), json!(video.categories));
    }
    if !video.thumbnail.is_empty() {
        metadata.insert("yt_thumbnail".to_string(), json!(video.thumbnail));
    }
    metadata.insert(
        "item_canonical_uri".to_string(),
        json!(item.manifest_item.canonical_uri),
    );
    metadata.insert("committed_generation".to_string(), json!("uncommitted"));
    metadata.insert("visibility".to_string(), json!("public"));
    metadata.insert("redaction_status".to_string(), json!("clean"));

    SourceDocument {
        document_id: youtube_document_id(
            &acquisition.source_id,
            &item.manifest_item.source_item_key,
        ),
        source_id: acquisition.source_id.clone(),
        source_item_key: item.manifest_item.source_item_key.clone(),
        canonical_uri: item.manifest_item.canonical_uri.clone(),
        content_kind: item
            .manifest_item
            .content_kind
            .unwrap_or(ContentKind::Transcript),
        content: item.content_ref.clone(),
        metadata,
        title: if video.title.is_empty() {
            None
        } else {
            Some(video.title.clone())
        },
        language: None,
        path: item.manifest_item.display_path.clone(),
        mime_type: None,
        structured_payload: None,
        artifact_id: item.raw_artifact_id.clone(),
        chunk_hints: plan.route.chunking_hints.clone(),
        parser_hints: plan.route.parser_hints.clone(),
    }
}

fn youtube_document_id(source_id: &SourceId, item_key: &SourceItemKey) -> DocumentId {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}\0{}", source_id.0, item_key.0).as_bytes());
    DocumentId::from(format!(
        "doc_youtube_{}",
        hex_prefix(&hasher.finalize(), 24)
    ))
}
