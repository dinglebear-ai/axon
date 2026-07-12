//! Per-item acquisition: dispatches each changed manifest item to the
//! injected [`FetchProvider`]/[`RenderProvider`] boundary (issue #298 Wave
//! 1b), replacing the old markdown-root/manifest.jsonl disk read.
//!
//! Dispatch by the effective `render_mode`:
//! - `Http` — a single raw [`FetchProvider::fetch`] call. Content stays
//!   whatever the origin sent (typically raw HTML); `content_kind` is decided
//!   from the response `Content-Type` so downstream chunking picks the right
//!   profile (`ContentKind::Html` -> `ChunkingProfile::HtmlArticle`). When
//!   `etag_conditional` is set and a prior `web_etag` is present on the
//!   incoming item's metadata, the request carries `If-None-Match` and a 304
//!   response is treated as unchanged (see [`acquire_via_fetch`]).
//! - `Chrome` — a single [`RenderProvider::render`] call in Chrome mode.
//! - `AutoSwitch` — render in `Http` mode first (this is the "fetch" step);
//!   if the resulting markdown is thin (`< min_markdown_chars`), re-render in
//!   `Chrome` mode and keep that result. A failed Chrome re-render falls back
//!   to keeping the original HTTP render, mirroring the documented
//!   auto-switch gotcha ("Chrome requires a running Chrome instance — if none
//!   is available, the HTTP result is kept").
//!
//! `Chrome`/`AutoSwitch` render requests also carry `automation_script` (when
//! configured) through to the [`RenderProvider`] — see
//! `providers::chrome_render` and `web_engine::scrape::apply_automation_scripts`
//! for how it actually executes.
//!
//! When `warc_path` is configured, every successfully acquired item (HTTP or
//! Chrome) is archived as a WARC 1.1 `response` record — see [`super::warc`]
//! for the writer and its documented `ArtifactStore` follow-up.

use std::path::Path;

use axon_api::source::*;
use axon_core::logging::{log_info, log_warn};
use serde_json::Value;

use crate::adapter::Result;
use crate::boundary::{FetchProvider, RenderProvider};

use super::options::{
    automation_script_ref, effective_render_mode, etag_conditional, min_markdown_chars, warc_path,
};

/// Options resolved once per [`acquire_changed_items`] call from
/// `plan.route.validated_options`, then threaded through every item so
/// per-item helpers stay free of `MetadataMap` lookups.
struct AcquireOptions {
    mode: RenderMode,
    min_markdown_chars: usize,
    automation_script: Option<ArtifactRef>,
    etag_conditional: bool,
}

/// Acquired items plus any side-effect artifacts produced by this run (today,
/// at most one WARC archive — see [`super::warc`]).
pub(super) struct AcquireOutcome {
    pub(super) items: Vec<AcquiredSourceItem>,
    pub(super) artifacts: Vec<ArtifactRef>,
}

pub(super) async fn acquire_changed_items(
    plan: &SourcePlan,
    manifest_items: &[ManifestItem],
    fetch: &dyn FetchProvider,
    render: &dyn RenderProvider,
) -> Result<AcquireOutcome> {
    let values = &plan.route.validated_options.values;
    let opts = AcquireOptions {
        mode: effective_render_mode(values),
        min_markdown_chars: min_markdown_chars(values),
        automation_script: automation_script_ref(values),
        etag_conditional: etag_conditional(values),
    };
    let warc_path = warc_path(values);
    let mut warc_file = open_warc_archive(warc_path.as_deref()).await?;

    let mut items = Vec::with_capacity(manifest_items.len());
    for item in manifest_items {
        let Some(acquired) = acquire_item(fetch, render, item, &opts).await? else {
            continue;
        };
        archive_to_warc(&mut warc_file, &acquired).await;
        items.push(acquired);
    }

    let artifacts = match warc_path {
        Some(path) => vec![super::warc::artifact_ref(&path).await],
        None => Vec::new(),
    };
    Ok(AcquireOutcome { items, artifacts })
}

async fn open_warc_archive(warc_path: Option<&Path>) -> Result<Option<tokio::fs::File>> {
    let Some(path) = warc_path else {
        return Ok(None);
    };
    super::warc::open(path).await.map(Some).map_err(|err| {
        ApiError::new(
            "web.warc.open_failed",
            axon_error::ErrorStage::Fetching,
            format!("failed to open WARC archive at {}: {err}", path.display()),
        )
    })
}

/// Append `acquired` to the WARC archive when one is open. A write failure is
/// logged, not propagated — archival is a best-effort side effect and must
/// not fail the acquisition of otherwise-good content.
async fn archive_to_warc(warc_file: &mut Option<tokio::fs::File>, acquired: &AcquiredSourceItem) {
    let Some(file) = warc_file.as_mut() else {
        return;
    };
    if let Err(err) = super::warc::append_item(file, acquired).await {
        log_warn(&format!(
            "warc: failed to append record for {}: {err}",
            acquired.manifest_item.canonical_uri
        ));
    }
}

async fn acquire_item(
    fetch: &dyn FetchProvider,
    render: &dyn RenderProvider,
    item: &ManifestItem,
    opts: &AcquireOptions,
) -> Result<Option<AcquiredSourceItem>> {
    match opts.mode {
        RenderMode::Http => acquire_via_fetch(fetch, item, opts.etag_conditional).await,
        RenderMode::Chrome => {
            let rendered = render
                .render(build_render_request(
                    item,
                    RenderMode::Chrome,
                    opts.automation_script.clone(),
                ))
                .await?;
            Ok(Some(acquired_from_rendered(
                item,
                rendered,
                "chrome_render",
            )))
        }
        RenderMode::AutoSwitch => {
            acquire_via_auto_switch(
                render,
                item,
                opts.min_markdown_chars,
                opts.automation_script.clone(),
            )
            .await
        }
    }
}

/// `Http`-mode acquisition. Returns `Ok(None)` when a conditional request
/// comes back `304 Not Modified` — the item is skipped rather than re-embedded
/// with empty content (issue #298 Wave 2b regression 3).
///
/// Reusing the *previous* content across generations (rather than simply
/// skipping this generation's re-embed) needs the ledger to hand the prior
/// `SourceDocument`/content back in — out of scope here; see [`super::warc`]'s
/// sibling module doc pattern and this crate's `CLAUDE.md` for the general
/// #298 follow-up shape. What this function does restore: a real conditional
/// GET (`If-None-Match` from `item.metadata["web_etag"]`, gated by
/// `etag_conditional`) and correct 304 recognition — `HttpFetchProvider`
/// already forwards arbitrary request headers and passes any non-5xx/429
/// status straight through, so no provider-side change was needed for either.
async fn acquire_via_fetch(
    fetch: &dyn FetchProvider,
    item: &ManifestItem,
    etag_conditional: bool,
) -> Result<Option<AcquiredSourceItem>> {
    let prior_etag = if etag_conditional {
        item.metadata.get("web_etag").and_then(Value::as_str)
    } else {
        None
    };
    let fetched = fetch.fetch(build_fetch_request(item, prior_etag)).await?;
    if fetched.status == 304 {
        log_info(&format!(
            "web_etag_conditional: 304 Not Modified for {} — skipping re-embed this generation \
             (reusing prior content across generations needs ledger persistence; not yet wired)",
            item.canonical_uri
        ));
        return Ok(None);
    }

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
    // Carry the etag forward on the item-level metadata (already the value
    // that flows into the persisted `SourceDocument.metadata` via
    // `web::metadata::web_source_document`'s merge) so a future generation
    // could attempt a conditional request — once the ledger/discover side
    // copies this key onto the next generation's `ManifestItem.metadata`.
    if let Some(etag) = fetched.etag.as_deref().or(prior_etag) {
        metadata.insert("web_etag".to_string(), serde_json::json!(etag));
    }

    Ok(Some(AcquiredSourceItem {
        manifest_item,
        fetch_status: LifecycleStatus::Completed,
        content_ref: fetched.content,
        raw_artifact_id: None,
        headers: fetched.headers,
        fetched_at: fetched.fetched_at,
        metadata,
    }))
}

/// `AutoSwitch`: render in `Http` mode (the "fetch" step), and if the
/// resulting markdown is thin, re-render in `Chrome` mode. A Chrome failure
/// keeps the original HTTP render rather than failing the whole item.
async fn acquire_via_auto_switch(
    render: &dyn RenderProvider,
    item: &ManifestItem,
    min_markdown_chars: usize,
    automation_script: Option<ArtifactRef>,
) -> Result<Option<AcquiredSourceItem>> {
    let first = render
        .render(build_render_request(
            item,
            RenderMode::Http,
            automation_script.clone(),
        ))
        .await?;
    if first.markdown.chars().count() >= min_markdown_chars {
        return Ok(Some(acquired_from_rendered(
            item,
            first,
            "auto_switch_http",
        )));
    }
    match render
        .render(build_render_request(
            item,
            RenderMode::Chrome,
            automation_script,
        ))
        .await
    {
        Ok(rendered) => Ok(Some(acquired_from_rendered(
            item,
            rendered,
            "auto_switch_chrome",
        ))),
        Err(_) => Ok(Some(acquired_from_rendered(
            item,
            first,
            "auto_switch_http_fallback",
        ))),
    }
}

/// `prior_etag`, when present, is sent as `If-None-Match` — the caller
/// decides whether one applies (gated by `etag_conditional` and whether the
/// incoming item carries a `web_etag`; see [`acquire_via_fetch`]).
fn build_fetch_request(item: &ManifestItem, prior_etag: Option<&str>) -> FetchRequest {
    let mut headers = Vec::new();
    if let Some(etag) = prior_etag {
        headers.push(RedactedHeader {
            name: "If-None-Match".to_string(),
            value: etag.to_string(),
            redacted: false,
        });
    }
    FetchRequest {
        uri: item.canonical_uri.clone(),
        method: "GET".to_string(),
        headers: RedactedHeaders { headers },
        body: None,
        timeout_ms: None,
        max_bytes: None,
        credential_refs: Vec::new(),
        metadata: MetadataMap::new(),
    }
}

fn build_render_request(
    item: &ManifestItem,
    mode: RenderMode,
    automation_script: Option<ArtifactRef>,
) -> RenderRequest {
    RenderRequest {
        uri: item.canonical_uri.clone(),
        mode,
        timeout_ms: None,
        wait_ms: None,
        automation_script,
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
