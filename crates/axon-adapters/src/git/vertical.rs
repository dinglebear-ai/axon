//! GitHub sub-page vertical acquisition — issues, pull requests, releases.
//!
//! `Repo`/`Directory`/`Branch` scopes clone the repository; the GitHub
//! sub-page scopes (`Issue`/`PullRequest`/`Release`) never clone. Routing still
//! sends every `github.com` URL to the git family (canonical `github://…`, and
//! `axon-route`'s `github` adapter already declares these scopes); this module
//! is how the git adapter honors them. It resolves a single GitHub API document
//! through the `github_issue`/`github_pr`/`github_release` vertical extractors
//! in `axon-extract` and re-enters the shared document pipeline as one
//! inline-markdown document — no clone, no checkout.

use std::sync::Arc;

use axon_api::source::*;
use axon_core::config::Config;
use axon_error::ErrorStage;
use axon_extract::{ScrapedDoc, VerticalContext};
use axon_parse::vertical::{
    VERTICAL_GRAPH_CANDIDATES_METADATA_KEY, VERTICAL_PARSE_FACTS_METADATA_KEY, VerticalParseInput,
    parse_artifacts,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{hex_prefix, stage_header, timestamp};
use crate::adapter::Result;
use crate::manifest::item_identity;

const VERTICAL_TITLE_METADATA_KEY: &str = "git_title";

/// The named vertical extractor that serves a GitHub sub-page scope, or `None`
/// for clone scopes. Extend here as gitlab/gitea sub-page extractors land.
pub(super) fn extractor_for_scope(scope: SourceScope) -> Option<&'static str> {
    match scope {
        SourceScope::Issue => Some("github_issue"),
        SourceScope::PullRequest => Some("github_pr"),
        SourceScope::Release => Some("github_release"),
        _ => None,
    }
}

/// Resolve `(extractor_name, https_url)` for a GitHub sub-page plan, or `None`
/// when the plan is not a github vertical target.
///
/// Gated on the `github://` canonical scheme so only github.com sub-pages take
/// this path — a gitlab/gitea issue scope must not be handed to a `github_*`
/// extractor. The URL is rebuilt from the normalized canonical URI (not the raw
/// input) so shorthand inputs still produce the exact `/pull/`, `/issues/`, and
/// `/releases/tag/` forms each extractor's `matches()` requires.
pub(super) fn resolve(plan: &SourcePlan) -> Option<(&'static str, String)> {
    let extractor = extractor_for_scope(plan.route.scope)?;
    let rest = plan.route.source.canonical_uri.strip_prefix("github://")?;
    let parts: Vec<&str> = rest.split('/').filter(|part| !part.is_empty()).collect();
    if parts.len() < 2 {
        return None;
    }
    let (owner, repo) = (parts[0], parts[1]);
    let url = match plan.route.scope {
        SourceScope::PullRequest => {
            format!("https://github.com/{owner}/{repo}/pull/{}", parts.get(3)?)
        }
        SourceScope::Issue => {
            format!("https://github.com/{owner}/{repo}/issues/{}", parts.get(3)?)
        }
        SourceScope::Release => match (parts.get(2), parts.get(3), parts.get(4)) {
            (Some(&"releases"), Some(&"tag"), Some(tag)) => {
                format!("https://github.com/{owner}/{repo}/releases/tag/{tag}")
            }
            _ => format!("https://github.com/{owner}/{repo}/releases"),
        },
        _ => return None,
    };
    Some((extractor, url))
}

/// True when the routed scope is a GitHub sub-page vertical target.
pub(super) fn is_vertical(plan: &SourcePlan) -> bool {
    resolve(plan).is_some()
}

/// `(owner, repo)` from the `github://owner/repo/…` canonical URI.
fn owner_repo(plan: &SourcePlan) -> Result<(String, String)> {
    let rest = plan
        .route
        .source
        .canonical_uri
        .strip_prefix("github://")
        .ok_or_else(|| scope_mismatch(plan))?;
    let parts: Vec<&str> = rest.split('/').filter(|part| !part.is_empty()).collect();
    match parts.as_slice() {
        [owner, repo, ..] => Ok((owner.to_string(), repo.to_string())),
        _ => Err(scope_mismatch(plan)),
    }
}

/// One-item manifest for the sub-page — the vertical produces a single document.
pub(super) fn discover(plan: &SourcePlan) -> Result<SourceManifest> {
    let (owner, repo) = owner_repo(plan)?;
    let base_uri = format!("github://{owner}/{repo}");
    let sub_key = plan
        .route
        .source
        .canonical_uri
        .strip_prefix(&format!("{base_uri}/"))
        .filter(|key| !key.is_empty())
        .unwrap_or("self");
    let identity = item_identity(SourceKind::Git, &base_uri, sub_key)?;

    let mut item_metadata = MetadataMap::new();
    item_metadata.insert("git_provider".to_string(), json!("github"));
    item_metadata.insert("git_owner".to_string(), json!(owner));
    item_metadata.insert("git_repo".to_string(), json!(repo));

    let item = ManifestItem {
        source_id: plan.route.source.source_id.clone(),
        source_item_key: identity.source_item_key,
        canonical_uri: identity.canonical_uri,
        item_kind: ItemKind::WebPage,
        content_kind: Some(ContentKind::Markdown),
        display_path: Some(sub_key.to_string()),
        parent_key: None,
        size_bytes: None,
        content_hash: None,
        mtime: None,
        version: None,
        fetch_plan: None,
        metadata: item_metadata,
        graph_hints: Vec::new(),
    };

    Ok(SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: SourceGenerationId::from("gen_git_vertical_discovery"),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items: vec![item],
        created_at: timestamp(),
        metadata: manifest_metadata(plan),
    })
}

/// Fetch the sub-page through its vertical extractor and re-shape the resulting
/// `ScrapedDoc` into an acquired item. Unlike the web path there is no generic
/// fallback here — a git checkout cannot represent a single PR/issue/release —
/// so an extractor failure surfaces as an acquisition error (retryable for rate
/// limits), never a silent clone attempt.
pub(super) async fn acquire(
    plan: &SourcePlan,
    diff: &SourceManifestDiff,
) -> Result<SourceAcquisition> {
    let (extractor, url) = resolve(plan).ok_or_else(|| scope_mismatch(plan))?;
    let manifest_items = diff
        .added
        .iter()
        .chain(diff.modified.iter())
        .cloned()
        .collect::<Vec<_>>();

    let mut fetched_items = Vec::with_capacity(manifest_items.len());
    if let Some(item) = manifest_items.first() {
        let ctx = VerticalContext::new(Arc::new(vertical_config()));
        let doc = crate::vertical_registry::dispatch_by_name(extractor, &url, &ctx)
            .await
            .map_err(|err| vertical_error(extractor, &url, &err.to_string()))?;
        fetched_items.push(acquired_from_doc(plan, item, doc));
    }

    let manifest = SourceManifest {
        source_id: plan.route.source.source_id.clone(),
        generation: diff.next_generation.clone(),
        adapter: plan.route.adapter.clone(),
        scope: plan.route.scope,
        items: manifest_items,
        created_at: timestamp(),
        metadata: manifest_metadata(plan),
    };
    Ok(SourceAcquisition {
        header: stage_header(
            plan.job_id,
            "git_vertical_fetch",
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

/// Turn the acquired vertical items into `SourceDocument`s, preserving the
/// extractor's structured payload, parse facts, and graph candidates for parity
/// with the web vertical path.
pub(super) fn normalize(
    plan: &SourcePlan,
    acquisition: SourceAcquisition,
) -> Result<StageExecutionResult<Vec<SourceDocument>>> {
    let documents = acquisition
        .fetched_items
        .iter()
        .map(|item| vertical_source_document(plan, &acquisition, item))
        .collect::<Vec<_>>();
    Ok(StageExecutionResult {
        header: stage_header(
            plan.job_id,
            "git_vertical_normalize",
            PipelinePhase::Normalizing,
            documents.len(),
        ),
        data: documents,
    })
}

fn vertical_config() -> Config {
    Config {
        enable_verticals: true,
        ..Config::default()
    }
}

fn acquired_from_doc(
    plan: &SourcePlan,
    item: &ManifestItem,
    doc: ScrapedDoc,
) -> AcquiredSourceItem {
    let mut manifest_item = item.clone();
    manifest_item.content_kind = Some(ContentKind::Markdown);

    let document_id =
        vertical_document_id(&manifest_item.source_id, &manifest_item.source_item_key);
    let parse = parse_artifacts(VerticalParseInput {
        url: &doc.url,
        title: doc.title.as_deref(),
        extractor_name: doc.extractor_name,
        extractor_version: doc.extractor_version,
        job_id: plan.job_id,
        source_id: &manifest_item.source_id,
        document_id: &document_id,
        source_item_key: &manifest_item.source_item_key,
    });

    let mut metadata = MetadataMap::new();
    metadata.insert("git_fetch_method".to_string(), json!("vertical_extractor"));
    metadata.insert("extractor_name".to_string(), json!(doc.extractor_name));
    metadata.insert(
        "extractor_version".to_string(),
        json!(doc.extractor_version),
    );
    if let Some(title) = doc.title.as_deref().filter(|title| !title.is_empty()) {
        metadata.insert(VERTICAL_TITLE_METADATA_KEY.to_string(), json!(title));
    }
    if !parse.facts.is_empty()
        && let Ok(value) = serde_json::to_value(&parse.facts)
    {
        metadata.insert(VERTICAL_PARSE_FACTS_METADATA_KEY.to_string(), value);
    }
    if !parse.graph_candidates.is_empty()
        && let Ok(value) = serde_json::to_value(&parse.graph_candidates)
    {
        metadata.insert(VERTICAL_GRAPH_CANDIDATES_METADATA_KEY.to_string(), value);
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
        fetched_at: timestamp(),
        metadata,
    }
}

fn vertical_source_document(
    plan: &SourcePlan,
    acquisition: &SourceAcquisition,
    item: &AcquiredSourceItem,
) -> SourceDocument {
    let mut metadata = item.manifest_item.metadata.clone();
    merge_metadata(&mut metadata, &item.metadata);
    metadata.insert("source_family".to_string(), json!("code"));
    metadata.insert("source_kind".to_string(), json!("git"));
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
        "item_canonical_uri".to_string(),
        json!(item.manifest_item.canonical_uri.clone()),
    );
    metadata.insert(
        "source_generation".to_string(),
        json!(acquisition.generation.0.clone()),
    );
    metadata.insert("committed_generation".to_string(), json!("uncommitted"));
    metadata.insert("visibility".to_string(), json!("internal"));
    metadata.insert("redaction_status".to_string(), json!("clean"));

    let title = metadata
        .get(VERTICAL_TITLE_METADATA_KEY)
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| item.manifest_item.display_path.clone());
    let structured_payload = metadata.remove("structured_payload");
    let content_kind = item
        .manifest_item
        .content_kind
        .unwrap_or(ContentKind::Markdown);

    SourceDocument {
        document_id: vertical_document_id(
            &acquisition.source_id,
            &item.manifest_item.source_item_key,
        ),
        source_id: acquisition.source_id.clone(),
        source_item_key: item.manifest_item.source_item_key.clone(),
        canonical_uri: item.manifest_item.canonical_uri.clone(),
        content_kind,
        content: item.content_ref.clone(),
        metadata,
        title,
        language: None,
        path: item.manifest_item.display_path.clone(),
        mime_type: Some("text/markdown".to_string()),
        structured_payload,
        artifact_id: item.raw_artifact_id.clone(),
        chunk_hints: plan.route.chunking_hints.clone(),
        parser_hints: plan.route.parser_hints.clone(),
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

fn merge_metadata(target: &mut MetadataMap, source: &MetadataMap) {
    for (key, value) in source.iter() {
        target.insert(key.clone(), value.clone());
    }
}

fn manifest_metadata(plan: &SourcePlan) -> MetadataMap {
    let mut metadata = MetadataMap::new();
    metadata.insert("git_provider".to_string(), json!("github"));
    metadata.insert("source_scope".to_string(), json!(plan.route.scope));
    metadata.insert(
        "git_web_url".to_string(),
        json!(github_web_url(&plan.route.source.canonical_uri)),
    );
    metadata
}

fn github_web_url(canonical_uri: &str) -> String {
    canonical_uri
        .strip_prefix("github://")
        .map(|rest| format!("https://github.com/{rest}"))
        .unwrap_or_else(|| canonical_uri.to_string())
}

fn vertical_document_id(source_id: &SourceId, item_key: &SourceItemKey) -> DocumentId {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}\0{}", source_id.0, item_key.0).as_bytes());
    DocumentId::from(format!(
        "doc_git_vertical_{}",
        hex_prefix(&hasher.finalize(), 24)
    ))
}

fn scope_mismatch(plan: &SourcePlan) -> ApiError {
    ApiError::new(
        "adapter.git.vertical.scope_mismatch",
        ErrorStage::Routing,
        "git vertical path invoked for a non-github sub-page scope",
    )
    .with_context("scope", format!("{:?}", plan.route.scope))
    .with_context("canonical_uri", plan.route.source.canonical_uri.clone())
}

fn vertical_error(extractor: &str, url: &str, message: &str) -> ApiError {
    ApiError::new(
        "adapter.git.vertical.extract_failed",
        ErrorStage::Fetching,
        format!("github vertical extractor '{extractor}' failed for {url}: {message}"),
    )
}

#[cfg(test)]
#[path = "vertical_tests.rs"]
mod tests;
