use axon_api::source::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::url_parts::WebUrlParts;

const NORMALIZATION_VERSION: &str = "web-url-v1";
const TITLE_SCAN_BYTES: usize = 128 * 1024;
const TITLE_MAX_CHARS: usize = 512;

pub(super) fn title_from_html(html: &str) -> Option<String> {
    let mut end = html.len().min(TITLE_SCAN_BYTES);
    while !html.is_char_boundary(end) {
        end -= 1;
    }
    let head = &html[..end];
    let lower = head.to_ascii_lowercase();
    let mut from = 0;
    let title_start = loop {
        let start = from + lower[from..].find("<title")?;
        let after = lower.as_bytes().get(start + "<title".len()).copied()?;
        if after == b'>' || after.is_ascii_whitespace() {
            break start;
        }
        from = start + "<title".len();
    };
    let content_start = title_start + lower[title_start..].find('>')? + 1;
    let content_end = content_start + lower[content_start..].find("</title")?;
    let title = head[content_start..content_end]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let title = title
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    let title: String = title.chars().take(TITLE_MAX_CHARS).collect();
    (!title.is_empty()).then_some(title)
}

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
    copy_provider_execution_metadata(&plan.request.metadata, &mut metadata);
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
    source_id: &SourceId,
    generation: &SourceGenerationId,
    item: AcquiredSourceItem,
) -> SourceDocument {
    let mut metadata = item.manifest_item.metadata;
    for (key, value) in item.metadata.0 {
        metadata.insert(key, value);
    }
    strip_provider_execution_metadata(&mut metadata);
    metadata.insert("source_family".to_string(), json!("web"));
    metadata.insert("source_kind".to_string(), json!("web"));
    metadata.insert("source_adapter".to_string(), json!(plan.route.adapter.name));
    metadata.insert("source_scope".to_string(), json!(plan.route.scope));
    metadata.insert("source_id".to_string(), json!(source_id.0.clone()));
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
    metadata.insert("source_generation".to_string(), json!(generation.0.clone()));
    metadata.insert("committed_generation".to_string(), json!("uncommitted"));
    metadata.insert(
        "normalization_version".to_string(),
        json!(NORMALIZATION_VERSION),
    );
    let structured_payload = metadata.remove("structured_payload");
    let title = structured_title(structured_payload.as_ref()).or_else(|| {
        metadata
            .get("web_title")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    let content_kind = item
        .manifest_item
        .content_kind
        .unwrap_or(ContentKind::Markdown);
    SourceDocument {
        document_id: web_document_id(source_id, &item.manifest_item.source_item_key),
        source_id: source_id.clone(),
        source_item_key: item.manifest_item.source_item_key,
        canonical_uri: item.manifest_item.canonical_uri,
        content_kind,
        content: item.content_ref,
        metadata,
        title,
        language: None,
        path: item.manifest_item.display_path,
        mime_type: Some(mime_type_for_content_kind(content_kind).to_string()),
        structured_payload,
        artifact_id: item.raw_artifact_id,
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

#[cfg(test)]
mod title_tests {
    use super::title_from_html;

    #[test]
    fn html_title_uses_head_title_and_ignores_similar_tags() {
        assert_eq!(
            title_from_html("<html><titlecase>Wrong</titlecase><TITLE data-x='1'>  A   &amp;   B </TITLE></html>").as_deref(),
            Some("A & B")
        );
        assert_eq!(title_from_html("<html><body>No title</body></html>"), None);
    }
}
