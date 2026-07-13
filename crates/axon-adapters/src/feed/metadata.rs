//! Feed `SourceDocument` construction — stamps only approved feed metadata
//! fields onto normalized documents.

use axon_api::source::*;
use serde_json::json;
use sha2::{Digest, Sha256};

use super::hex_prefix;

pub(super) fn feed_source_document(
    plan: &SourcePlan,
    feed_title: Option<&str>,
    feed_link: Option<&str>,
    acquisition: &SourceAcquisition,
    item: &AcquiredSourceItem,
) -> SourceDocument {
    let mut metadata = MetadataMap::new();
    metadata.insert("source_family".to_string(), json!("feed"));
    metadata.insert("source_kind".to_string(), json!("feed"));
    metadata.insert("source_adapter".to_string(), json!(plan.route.adapter.name));
    metadata.insert("source_scope".to_string(), json!(plan.route.scope));
    if let Some(title) = feed_title {
        metadata.insert("feed_title".to_string(), json!(title));
    }
    if let Some(link) = feed_link {
        metadata.insert("feed_link".to_string(), json!(link));
    }
    for (key, value) in item.manifest_item.metadata.iter() {
        metadata.insert(key.clone(), value.clone());
    }
    for (key, value) in item.metadata.iter() {
        metadata.insert(key.clone(), value.clone());
    }
    metadata.insert(
        "item_canonical_uri".to_string(),
        json!(item.manifest_item.canonical_uri),
    );
    metadata.insert("committed_generation".to_string(), json!("uncommitted"));
    metadata.insert("visibility".to_string(), json!("internal"));
    metadata.insert("redaction_status".to_string(), json!("clean"));
    SourceDocument {
        document_id: feed_document_id(&acquisition.source_id, &item.manifest_item.source_item_key),
        source_id: acquisition.source_id.clone(),
        source_item_key: item.manifest_item.source_item_key.clone(),
        canonical_uri: item.manifest_item.canonical_uri.clone(),
        content_kind: item
            .manifest_item
            .content_kind
            .unwrap_or(ContentKind::PlainText),
        content: item.content_ref.clone(),
        metadata,
        title: item.manifest_item.display_path.clone(),
        language: None,
        path: item.manifest_item.display_path.clone(),
        mime_type: None,
        structured_payload: None,
        artifact_id: item.raw_artifact_id.clone(),
        chunk_hints: plan.route.chunking_hints.clone(),
        parser_hints: plan.route.parser_hints.clone(),
    }
}

fn feed_document_id(source_id: &SourceId, item_key: &SourceItemKey) -> DocumentId {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}\0{}", source_id.0, item_key.0).as_bytes());
    DocumentId::from(format!("doc_feed_{}", hex_prefix(&hasher.finalize(), 24)))
}
