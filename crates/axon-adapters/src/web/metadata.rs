use axon_api::source::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::url_parts::WebUrlParts;

const NORMALIZATION_VERSION: &str = "web-url-v1";

pub(super) fn web_metadata(plan: &SourcePlan, web: &WebUrlParts) -> MetadataMap {
    let mut metadata = MetadataMap::new();
    metadata.insert("source_family".to_string(), json!("web"));
    metadata.insert("source_kind".to_string(), json!("web"));
    metadata.insert("source_adapter".to_string(), json!(plan.route.adapter.name));
    metadata.insert("source_scope".to_string(), json!(plan.route.scope));
    metadata.insert(
        "source_id".to_string(),
        json!(plan.route.source.source_id.0.clone()),
    );
    metadata.insert(
        "source_canonical_uri".to_string(),
        json!(plan.route.source.canonical_uri.clone()),
    );
    metadata.insert(
        "item_canonical_uri".to_string(),
        json!(web.normalized_url.clone()),
    );
    metadata.insert(
        "normalization_version".to_string(),
        json!(NORMALIZATION_VERSION),
    );
    metadata.insert("web_url".to_string(), json!(web.normalized_url.clone()));
    metadata.insert(
        "web_seed_url".to_string(),
        json!(plan.route.source.canonical_uri.clone()),
    );
    metadata.insert("web_domain".to_string(), json!(web.domain.clone()));
    metadata.insert("web_origin".to_string(), json!(web.origin.clone()));
    metadata.insert("web_path".to_string(), json!(web.path.clone()));
    metadata.insert(
        "web_normalized_url".to_string(),
        json!(web.normalized_url.clone()),
    );
    metadata.insert("web_fetch_method".to_string(), json!("manifest"));
    metadata
}

pub(super) fn manifest_metadata(plan: &SourcePlan) -> MetadataMap {
    let mut metadata = MetadataMap::new();
    metadata.insert("source_kind".to_string(), json!("web"));
    metadata.insert("source_adapter".to_string(), json!(plan.route.adapter.name));
    metadata.insert("source_scope".to_string(), json!(plan.route.scope));
    metadata.insert(
        "embed_requested".to_string(),
        json!(plan.route.scope != SourceScope::Map),
    );
    metadata
}

pub(super) fn web_source_document(
    plan: &SourcePlan,
    acquisition: &SourceAcquisition,
    item: &AcquiredSourceItem,
) -> SourceDocument {
    let mut metadata = item.manifest_item.metadata.clone();
    merge_metadata(&mut metadata, &item.metadata);
    metadata.insert("source_family".to_string(), json!("web"));
    metadata.insert("source_kind".to_string(), json!("web"));
    metadata.insert("source_adapter".to_string(), json!(plan.route.adapter.name));
    metadata.insert("source_scope".to_string(), json!(plan.route.scope));
    metadata.insert(
        "source_id".to_string(),
        json!(acquisition.source_id.0.clone()),
    );
    metadata.insert(
        "source_canonical_uri".to_string(),
        json!(plan.route.source.canonical_uri.clone()),
    );
    metadata.insert(
        "source_item_key".to_string(),
        json!(item.manifest_item.source_item_key.0.clone()),
    );
    metadata.insert(
        "item_canonical_uri".to_string(),
        json!(item.manifest_item.canonical_uri.clone()),
    );
    metadata.insert(
        "source_generation".to_string(),
        json!(acquisition.generation.0.clone()),
    );
    metadata.insert("committed_generation".to_string(), json!("uncommitted"));
    metadata.insert(
        "normalization_version".to_string(),
        json!(NORMALIZATION_VERSION),
    );
    let structured_payload = metadata.remove("structured_payload");
    let title = structured_title(structured_payload.as_ref());
    let content_kind = item
        .manifest_item
        .content_kind
        .unwrap_or(ContentKind::Markdown);
    SourceDocument {
        document_id: web_document_id(&acquisition.source_id, &item.manifest_item.source_item_key),
        source_id: acquisition.source_id.clone(),
        source_item_key: item.manifest_item.source_item_key.clone(),
        canonical_uri: item.manifest_item.canonical_uri.clone(),
        content_kind,
        content: item.content_ref.clone(),
        metadata,
        title,
        language: None,
        path: item.manifest_item.display_path.clone(),
        mime_type: Some(mime_type_for_content_kind(content_kind).to_string()),
        structured_payload,
        artifact_id: item.raw_artifact_id.clone(),
        chunk_hints: plan.route.chunking_hints.clone(),
        parser_hints: plan.route.parser_hints.clone(),
    }
}

/// `acquire` (issue #298 Wave 1b) can now hand back raw HTML/JSON/etc. — not
/// only markdown — depending on the effective render mode, so the document's
/// `mime_type` follows the actually-acquired `content_kind` instead of always
/// stamping `text/markdown`.
pub(super) fn mime_type_for_content_kind(content_kind: ContentKind) -> &'static str {
    match content_kind {
        ContentKind::Html => "text/html",
        ContentKind::Json | ContentKind::Structured => "application/json",
        ContentKind::Xml => "application/xml",
        ContentKind::Yaml => "application/yaml",
        ContentKind::Toml => "application/toml",
        ContentKind::PlainText => "text/plain",
        ContentKind::BinaryMetadata => "application/octet-stream",
        ContentKind::Code | ContentKind::Transcript | ContentKind::Markdown => "text/markdown",
    }
}

fn structured_title(value: Option<&Value>) -> Option<String> {
    value
        .and_then(|value| value.get("title"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub(super) fn web_document_id(source_id: &SourceId, item_key: &SourceItemKey) -> DocumentId {
    DocumentId::from(format!(
        "doc_web_{}",
        stable_token(&format!("{}\0{}", source_id.0, item_key.0))
    ))
}

fn stable_token(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    let mut token = String::with_capacity(24);
    for byte in &digest[..12] {
        use std::fmt::Write as _;
        let _ = write!(&mut token, "{byte:02x}");
    }
    token
}

fn merge_metadata(target: &mut MetadataMap, source: &MetadataMap) {
    for (key, value) in source.iter() {
        target.insert(key.clone(), value.clone());
    }
}
