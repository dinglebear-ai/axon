//! Reddit `SourceDocument` construction — stamps only approved reddit
//! metadata fields onto normalized documents. Field names are ported from the
//! legacy `axon-ingest::reddit::meta::build_reddit_post_extra_payload`.

use axon_api::source::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::dump::RedditDumpItem;

/// Build the approved `reddit_*` metadata fields for a single dump item.
/// Mirrors the legacy `reddit_author`, `reddit_created_utc`, `reddit_score`,
/// `reddit_num_comments`, `reddit_upvote_ratio`, `reddit_subreddit`,
/// `reddit_domain`, `reddit_is_video`, `reddit_distinguished`,
/// `reddit_gilded`, `reddit_flair` fields plus permalink/kind identifiers.
pub(super) fn reddit_item_metadata(item: &RedditDumpItem) -> MetadataMap {
    let mut metadata = MetadataMap::new();
    metadata.insert("reddit_author".to_string(), json!(item.author_or_deleted()));
    metadata.insert(
        "reddit_created_utc".to_string(),
        json!(item.created_utc.unwrap_or(0)),
    );
    metadata.insert("reddit_score".to_string(), json!(item.score.unwrap_or(0)));
    metadata.insert(
        "reddit_num_comments".to_string(),
        json!(item.num_comments.unwrap_or(0)),
    );
    metadata.insert(
        "reddit_upvote_ratio".to_string(),
        json!(item.upvote_ratio.unwrap_or(0.0)),
    );
    metadata.insert(
        "reddit_subreddit".to_string(),
        json!(item.subreddit.clone().unwrap_or_default()),
    );
    metadata.insert(
        "reddit_domain".to_string(),
        json!(item.domain.clone().unwrap_or_default()),
    );
    metadata.insert(
        "reddit_is_video".to_string(),
        json!(item.is_video.unwrap_or(false)),
    );
    if let Some(distinguished) = &item.distinguished {
        metadata.insert("reddit_distinguished".to_string(), json!(distinguished));
    } else {
        metadata.insert("reddit_distinguished".to_string(), Value::Null);
    }
    metadata.insert("reddit_gilded".to_string(), json!(item.gilded.unwrap_or(0)));
    if let Some(flair) = &item.link_flair_text {
        metadata.insert("reddit_flair".to_string(), json!(flair));
    } else {
        metadata.insert("reddit_flair".to_string(), Value::Null);
    }
    metadata.insert(
        "reddit_permalink".to_string(),
        json!(item.permalink.clone().unwrap_or_default()),
    );
    metadata.insert("reddit_kind".to_string(), json!(item.kind_label()));
    metadata
}

pub(super) fn reddit_source_document(
    plan: &SourcePlan,
    acquisition: &SourceAcquisition,
    item: &AcquiredSourceItem,
    dump_item: &RedditDumpItem,
) -> SourceDocument {
    let mut metadata = reddit_item_metadata(dump_item);
    metadata.insert("source_family".to_string(), json!("social"));
    metadata.insert("source_kind".to_string(), json!("reddit"));
    metadata.insert("source_adapter".to_string(), json!(plan.route.adapter.name));
    metadata.insert("source_scope".to_string(), json!(plan.route.scope));
    metadata.insert(
        "item_canonical_uri".to_string(),
        json!(item.manifest_item.canonical_uri),
    );
    metadata.insert("committed_generation".to_string(), json!("uncommitted"));
    metadata.insert("visibility".to_string(), json!("internal"));
    metadata.insert("redaction_status".to_string(), json!("clean"));

    SourceDocument {
        document_id: reddit_document_id(
            &acquisition.source_id,
            &item.manifest_item.source_item_key,
        ),
        source_id: acquisition.source_id.clone(),
        source_item_key: item.manifest_item.source_item_key.clone(),
        canonical_uri: item.manifest_item.canonical_uri.clone(),
        content_kind: item
            .manifest_item
            .content_kind
            .unwrap_or(ContentKind::PlainText),
        content: item.content_ref.clone(),
        metadata,
        title: dump_item.title.clone(),
        language: None,
        path: item.manifest_item.display_path.clone(),
        mime_type: None,
        structured_payload: None,
        artifact_id: item.raw_artifact_id.clone(),
        chunk_hints: plan.route.chunking_hints.clone(),
        parser_hints: plan.route.parser_hints.clone(),
    }
}

fn reddit_document_id(source_id: &SourceId, item_key: &SourceItemKey) -> DocumentId {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}\0{}", source_id.0, item_key.0).as_bytes());
    DocumentId::from(format!("doc_reddit_{}", hex_prefix(&hasher.finalize(), 24)))
}

pub(super) fn hex_prefix(digest: &[u8], hex_chars: usize) -> String {
    use std::fmt::Write as _;
    let mut token = String::with_capacity(hex_chars);
    for byte in &digest[..(hex_chars / 2).min(digest.len())] {
        let _ = write!(&mut token, "{byte:02x}");
    }
    token
}
