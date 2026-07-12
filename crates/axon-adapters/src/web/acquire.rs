//! Per-item acquisition: dispatches each changed manifest item to the
//! injected [`FetchProvider`]/[`RenderProvider`] boundary (issue #298 Wave
//! 1b), replacing the old markdown-root/manifest.jsonl disk read.
//!
//! Dispatch by the effective `render_mode`:
//! - `Http` — a single raw [`FetchProvider::fetch`] call. Content stays
//!   whatever the origin sent (typically raw HTML); `content_kind` is decided
//!   from the response `Content-Type` so downstream chunking picks the right
//!   profile (`ContentKind::Html` -> `ChunkingProfile::HtmlArticle`).
//! - `Chrome` — a single [`RenderProvider::render`] call in Chrome mode.
//! - `AutoSwitch` — render in `Http` mode first (this is the "fetch" step);
//!   if the resulting markdown is thin (`< min_markdown_chars`), re-render in
//!   `Chrome` mode and keep that result. A failed Chrome re-render falls back
//!   to keeping the original HTTP render, mirroring the documented
//!   auto-switch gotcha ("Chrome requires a running Chrome instance — if none
//!   is available, the HTTP result is kept").

use axon_api::source::*;

use crate::adapter::Result;
use crate::boundary::{FetchProvider, RenderProvider};

use super::options::{effective_render_mode, min_markdown_chars};

pub(super) async fn acquire_changed_items(
    plan: &SourcePlan,
    manifest_items: &[ManifestItem],
    fetch: &dyn FetchProvider,
    render: &dyn RenderProvider,
) -> Result<Vec<AcquiredSourceItem>> {
    let values = &plan.route.validated_options.values;
    let mode = effective_render_mode(values);
    let min_chars = min_markdown_chars(values);
    let mut fetched_items = Vec::with_capacity(manifest_items.len());
    for item in manifest_items {
        fetched_items.push(acquire_item(fetch, render, item, mode, min_chars).await?);
    }
    Ok(fetched_items)
}

async fn acquire_item(
    fetch: &dyn FetchProvider,
    render: &dyn RenderProvider,
    item: &ManifestItem,
    mode: RenderMode,
    min_markdown_chars: usize,
) -> Result<AcquiredSourceItem> {
    match mode {
        RenderMode::Http => acquire_via_fetch(fetch, item).await,
        RenderMode::Chrome => {
            let rendered = render
                .render(build_render_request(item, RenderMode::Chrome))
                .await?;
            Ok(acquired_from_rendered(item, rendered, "chrome_render"))
        }
        RenderMode::AutoSwitch => acquire_via_auto_switch(render, item, min_markdown_chars).await,
    }
}

async fn acquire_via_fetch(fetch: &dyn FetchProvider, item: &ManifestItem) -> Result<AcquiredSourceItem> {
    let fetched = fetch.fetch(build_fetch_request(item)).await?;
    let content_kind = content_kind_for_fetch(&fetched);
    let mut manifest_item = item.clone();
    manifest_item.content_kind = Some(content_kind);

    let mut metadata = MetadataMap::new();
    metadata.insert(
        "web_fetch_method".to_string(),
        serde_json::json!("http_fetch"),
    );
    metadata.insert("web_render_mode".to_string(), serde_json::json!("http"));
    metadata.insert("web_status".to_string(), serde_json::json!(fetched.status));
    if let Some(etag) = &fetched.etag {
        metadata.insert("web_etag".to_string(), serde_json::json!(etag));
    }

    Ok(AcquiredSourceItem {
        manifest_item,
        fetch_status: LifecycleStatus::Completed,
        content_ref: fetched.content,
        raw_artifact_id: None,
        headers: fetched.headers,
        fetched_at: fetched.fetched_at,
        metadata,
    })
}

/// `AutoSwitch`: render in `Http` mode (the "fetch" step), and if the
/// resulting markdown is thin, re-render in `Chrome` mode. A Chrome failure
/// keeps the original HTTP render rather than failing the whole item.
async fn acquire_via_auto_switch(
    render: &dyn RenderProvider,
    item: &ManifestItem,
    min_markdown_chars: usize,
) -> Result<AcquiredSourceItem> {
    let first = render
        .render(build_render_request(item, RenderMode::Http))
        .await?;
    if first.markdown.chars().count() >= min_markdown_chars {
        return Ok(acquired_from_rendered(item, first, "auto_switch_http"));
    }
    match render
        .render(build_render_request(item, RenderMode::Chrome))
        .await
    {
        Ok(rendered) => Ok(acquired_from_rendered(item, rendered, "auto_switch_chrome")),
        Err(_) => Ok(acquired_from_rendered(item, first, "auto_switch_http_fallback")),
    }
}

fn build_fetch_request(item: &ManifestItem) -> FetchRequest {
    FetchRequest {
        uri: item.canonical_uri.clone(),
        method: "GET".to_string(),
        headers: RedactedHeaders {
            headers: Vec::new(),
        },
        body: None,
        timeout_ms: None,
        max_bytes: None,
        credential_refs: Vec::new(),
        metadata: MetadataMap::new(),
    }
}

fn build_render_request(item: &ManifestItem, mode: RenderMode) -> RenderRequest {
    RenderRequest {
        uri: item.canonical_uri.clone(),
        mode,
        timeout_ms: None,
        wait_ms: None,
        automation_script: None,
        credential_refs: Vec::new(),
        metadata: MetadataMap::new(),
    }
}

fn acquired_from_rendered(
    item: &ManifestItem,
    rendered: RenderedResource,
    method_tag: &'static str,
) -> AcquiredSourceItem {
    let mut manifest_item = item.clone();
    manifest_item.content_kind = Some(ContentKind::Markdown);

    let mut metadata = MetadataMap::new();
    metadata.insert(
        "web_fetch_method".to_string(),
        serde_json::json!(method_tag),
    );
    metadata.insert(
        "web_render_mode".to_string(),
        serde_json::json!(render_mode_tag(rendered.render_mode)),
    );

    AcquiredSourceItem {
        manifest_item,
        fetch_status: LifecycleStatus::Completed,
        content_ref: ContentRef::InlineText {
            text: rendered.markdown,
        },
        raw_artifact_id: None,
        headers: RedactedHeaders {
            headers: Vec::new(),
        },
        fetched_at: rendered.captured_at,
        metadata,
    }
}

fn render_mode_tag(mode: RenderMode) -> &'static str {
    match mode {
        RenderMode::Http => "http",
        RenderMode::Chrome => "chrome",
        RenderMode::AutoSwitch => "auto_switch",
    }
}

/// Decide `ContentKind` from a raw [`FetchedResource`]. Binary payloads get
/// `BinaryMetadata`; text payloads are classified from `Content-Type`,
/// defaulting to `Html` (the common case for a generic web fetch) so
/// `axon-document`'s `HtmlArticle` chunking profile handles the readability
/// extraction that used to happen implicitly via the crawl's HTML->markdown
/// transform.
fn content_kind_for_fetch(fetched: &FetchedResource) -> ContentKind {
    if matches!(fetched.content, ContentRef::InlineBytes { .. }) {
        return ContentKind::BinaryMetadata;
    }
    let content_type = fetched
        .headers
        .headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("content-type"))
        .map(|header| header.value.to_ascii_lowercase())
        .unwrap_or_default();
    if content_type.contains("json") {
        ContentKind::Json
    } else if content_type.contains("xml") {
        ContentKind::Xml
    } else if content_type.contains("markdown") {
        ContentKind::Markdown
    } else if content_type.contains("text/plain") {
        ContentKind::PlainText
    } else {
        ContentKind::Html
    }
}

#[cfg(test)]
#[path = "acquire_tests.rs"]
mod tests;
