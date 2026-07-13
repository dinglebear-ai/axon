//! Web vertical-extractor bridge.
//!
//! The extractor catalog lives in `axon-extract`; the web adapter owns when it
//! participates in acquisition. Automatic vertical dispatch is a per-item
//! optimization/enrichment stage: unsupported URLs return `None`, extractor
//! failures become visible warnings and the caller falls back to generic web
//! fetch/render.

use std::sync::Arc;

use axon_api::source::*;
use axon_core::config::Config;
use axon_extract::{ScrapedDoc, VerticalContext};
use serde_json::{Value, json};

use super::metadata::web_document_id;

pub(super) const VERTICAL_PARSE_FACTS_KEY: &str = "_axon_vertical_parse_facts";
pub(super) const VERTICAL_GRAPH_CANDIDATES_KEY: &str = "_axon_vertical_graph_candidates";

#[derive(Debug, Clone)]
pub(super) struct VerticalOptions {
    pub(super) enabled: bool,
    pub(super) auto_dispatch_skip: Vec<String>,
    pub(super) user_agent: Option<String>,
}

#[derive(Debug)]
pub(super) enum VerticalAcquire {
    Handled(AcquiredSourceItem),
    Degraded(SourceWarning),
    Unsupported,
}

pub(super) async fn try_acquire(
    item: &ManifestItem,
    opts: &VerticalOptions,
    job_id: JobId,
) -> VerticalAcquire {
    if !opts.enabled {
        return VerticalAcquire::Unsupported;
    }

    let ctx = VerticalContext::new(Arc::new(vertical_config(opts)));
    match axon_extract::dispatch_by_url(&item.canonical_uri, &ctx).await {
        None => VerticalAcquire::Unsupported,
        Some(Ok(doc)) => VerticalAcquire::Handled(acquired_from_doc(item, doc, job_id)),
        Some(Err(err)) => VerticalAcquire::Degraded(SourceWarning {
            code: "web.vertical.extractor_failed".to_string(),
            severity: Severity::Warning,
            message: format!(
                "vertical extractor failed for {}; falling back to generic web acquisition: {err}",
                item.canonical_uri
            ),
            source_item_key: Some(item.source_item_key.clone()),
            retryable: true,
        }),
    }
}

fn vertical_config(opts: &VerticalOptions) -> Config {
    Config {
        enable_verticals: opts.enabled,
        auto_dispatch_skip: opts.auto_dispatch_skip.clone(),
        user_agent: opts.user_agent.clone(),
        ..Config::default()
    }
}

fn acquired_from_doc(item: &ManifestItem, doc: ScrapedDoc, job_id: JobId) -> AcquiredSourceItem {
    let mut manifest_item = item.clone();
    manifest_item.content_kind = Some(ContentKind::Markdown);

    let document_id = web_document_id(&manifest_item.source_id, &manifest_item.source_item_key);
    let parse = doc.parse_artifacts(
        job_id,
        manifest_item.source_id.clone(),
        document_id,
        manifest_item.source_item_key.clone(),
    );

    let mut metadata = MetadataMap::new();
    metadata.insert("web_fetch_method".to_string(), json!("vertical_extractor"));
    metadata.insert("web_render_mode".to_string(), json!("vertical"));
    metadata.insert("extractor_name".to_string(), json!(doc.extractor_name));
    metadata.insert(
        "extractor_version".to_string(),
        json!(doc.extractor_version),
    );
    if let Some(title) = doc.title.as_deref().filter(|title| !title.is_empty()) {
        metadata.insert("web_title".to_string(), json!(title));
    }
    if !parse.facts.is_empty()
        && let Ok(value) = serde_json::to_value(&parse.facts)
    {
        metadata.insert(VERTICAL_PARSE_FACTS_KEY.to_string(), value);
    }
    if !parse.graph_candidates.is_empty()
        && let Ok(value) = serde_json::to_value(&parse.graph_candidates)
    {
        metadata.insert(VERTICAL_GRAPH_CANDIDATES_KEY.to_string(), value);
    }
    let structured = vertical_structured_payload(&doc);
    if structured != Value::Null {
        metadata.insert("structured_payload".to_string(), structured);
    }

    AcquiredSourceItem {
        manifest_item,
        fetch_status: LifecycleStatus::Completed,
        content_ref: ContentRef::InlineText { text: doc.markdown },
        raw_artifact_id: None,
        headers: RedactedHeaders {
            headers: Vec::new(),
        },
        fetched_at: super::timestamp(),
        metadata,
    }
}

fn vertical_structured_payload(doc: &ScrapedDoc) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("kind".to_string(), json!("vertical_extractor"));
    object.insert("extractor_name".to_string(), json!(doc.extractor_name));
    object.insert(
        "extractor_version".to_string(),
        json!(doc.extractor_version),
    );
    if !doc.follow_crawl_urls.is_empty() {
        object.insert(
            "follow_crawl_urls".to_string(),
            json!(doc.follow_crawl_urls),
        );
    }
    if let Some(extra) = doc.extra.clone() {
        object.insert("extra".to_string(), extra);
    }
    if let Some(structured) = doc.structured.clone() {
        object.insert("structured".to_string(), structured);
    }
    Value::Object(object)
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn item() -> ManifestItem {
        ManifestItem {
            source_id: SourceId::from("src_web_vertical_test"),
            source_item_key: SourceItemKey::from("https://github.com/jmagar/axon"),
            canonical_uri: "https://github.com/jmagar/axon".to_string(),
            item_kind: ItemKind::WebPage,
            content_kind: None,
            display_path: Some("jmagar/axon".to_string()),
            parent_key: None,
            size_bytes: None,
            content_hash: None,
            mtime: None,
            version: None,
            fetch_plan: None,
            metadata: MetadataMap::new(),
            graph_hints: Vec::new(),
        }
    }

    #[test]
    fn acquired_doc_carries_vertical_metadata_and_artifacts() {
        let acquired = acquired_from_doc(
            &item(),
            ScrapedDoc {
                url: "https://github.com/jmagar/axon".to_string(),
                markdown: "# Axon\n\nRepository metadata.".to_string(),
                title: Some("jmagar/axon".to_string()),
                extractor_name: "github_repo",
                extractor_version: 3,
                structured: Some(json!({ "full_name": "jmagar/axon" })),
                follow_crawl_urls: vec!["https://github.com/jmagar/axon/wiki".to_string()],
                extra: Some(json!({ "git_host": "github.com" })),
            },
            JobId::new(Uuid::nil()),
        );

        assert_eq!(
            acquired.manifest_item.content_kind,
            Some(ContentKind::Markdown)
        );
        assert_eq!(
            acquired.metadata.get("extractor_name"),
            Some(&json!("github_repo"))
        );
        assert_eq!(acquired.metadata.get("extractor_version"), Some(&json!(3)));
        assert!(matches!(
            acquired.content_ref,
            ContentRef::InlineText { ref text } if text.contains("Repository metadata")
        ));

        let parse_facts = acquired
            .metadata
            .get(VERTICAL_PARSE_FACTS_KEY)
            .and_then(Value::as_array)
            .expect("vertical parse facts should be carried to prepare");
        assert_eq!(parse_facts.len(), 1);
        let graph_candidates = acquired
            .metadata
            .get(VERTICAL_GRAPH_CANDIDATES_KEY)
            .and_then(Value::as_array)
            .expect("vertical graph candidates should be carried to prepare");
        assert_eq!(graph_candidates.len(), 1);

        let structured = acquired
            .metadata
            .get("structured_payload")
            .expect("vertical structured payload");
        assert_eq!(structured["kind"], json!("vertical_extractor"));
        assert_eq!(structured["extra"]["git_host"], json!("github.com"));
        assert_eq!(
            structured["follow_crawl_urls"][0],
            json!("https://github.com/jmagar/axon/wiki")
        );
    }
}
