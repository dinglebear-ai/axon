//! Per-item acquisition: dispatches each changed manifest item to the
//! injected [`FetchProvider`]/[`RenderProvider`] boundary (issue #298 Wave
//! 1b), replacing the old markdown-root/manifest.jsonl disk read.
//!
//! Dispatch by the effective `render_mode`:
//! - `Http` — a single raw [`FetchProvider::fetch`] call. Content stays
//!   whatever the origin sent (typically raw HTML); `content_kind` is decided
//!   from the response `Content-Type` so downstream chunking picks the right
//!   profile (`ContentKind::Html` -> `ChunkingProfile::HtmlArticle`). When
//!   `etag_conditional` is set and a prior `web_prior_etag` is present on the
//!   incoming item's metadata, the request carries `If-None-Match` and a 304
//!   response is treated as unchanged (see [`acquire_via_fetch`]). The
//!   services layer overlays that prior validator from the previous committed
//!   manifest so current discovery metadata never masquerades as the prior
//!   representation's validator.
//! - `Chrome` — a single [`RenderProvider::render`] call in Chrome mode.
//! - `AutoSwitch` — render in `Http` mode first (this is the "fetch" step);
//!   if the resulting markdown is thin (`< min_markdown_chars`), re-render in
//!   `Chrome` mode and keep that result. A failed Chrome re-render falls back
//!   to keeping the original HTTP render, logs a warning, and records a
//!   [`SourceWarning`] so the degradation is visible to the caller rather
//!   than silently swallowed (mirrors the documented auto-switch gotcha:
//!   "Chrome requires a running Chrome instance — if none is available, the
//!   HTTP result is kept").
//!
//! `Chrome`/`AutoSwitch` render requests also carry `automation_script` (when
//! configured) through to the [`RenderProvider`] — see
//! `providers::chrome_render` and `web_engine::scrape::apply_automation_scripts`
//! for how it actually executes.
//!
//! ## Concurrency and per-item error isolation (PR #418 review)
//!
//! Items acquire with bounded concurrency (up to [`ACQUIRE_CONCURRENCY`] in
//! flight, see [`acquire_concurrent`]) rather than one at a time — each item
//! is an independent fetch/render round-trip (2 round-trips on `AutoSwitch`),
//! so serializing them wasted latency for no correctness benefit. A single
//! item's fetch/render failure is logged and turned into a [`SourceWarning`]
//! (see [`resolve_item_outcome`]) rather than propagated with `?` — one bad
//! item must not discard every already-succeeded sibling in the batch.
//!
//! When `warc_path` is configured, every successfully acquired item (HTTP or
//! Chrome) is archived as a WARC 1.1 `response` record — see [`super::warc`]
//! for the writer and its documented `ArtifactStore` follow-up. WARC archival
//! is a genuine serial dependency (one ordered on-disk log of records), so a
//! configured WARC sink falls back to the original one-item-at-a-time
//! acquisition path (see [`acquire_sequential`]) instead of the concurrent
//! path. Without a WARC sink, returned item order is **not** guaranteed to
//! match the input `manifest_items` order — safe today because every
//! consumer of `fetched_items` keys off each item's own embedded
//! `manifest_item`, never positional correspondence.

use std::path::Path;

use axon_api::source::*;
use axon_core::logging::{log_info, log_warn};
use futures_util::stream::{self, StreamExt};
use serde_json::Value;

use crate::adapter::Result;
use crate::boundary::{FetchProvider, RenderProvider};

use super::options::{
    auto_dispatch_skip, automation_script_ref, effective_render_mode, etag_conditional,
    min_markdown_chars, user_agent, verticals_enabled, warc_path,
};
use super::vertical::{VerticalAcquire, VerticalOptions};

/// Upper bound on in-flight `acquire_item` calls for [`acquire_concurrent`].
/// Chosen as a sane fixed default (matching `extract::sync`'s per-URL
/// concurrency) rather than wired to a perf profile — there is no existing
/// validated web-adapter option for it (see `axon-route::web_options`), and
/// adding one is a larger follow-up than this fix's scope.
const ACQUIRE_CONCURRENCY: usize = 16;

/// Options resolved once per [`acquire_changed_items`] call from
/// `plan.route.validated_options`, then threaded through every item so
/// per-item helpers stay free of `MetadataMap` lookups.
struct AcquireOptions {
    job_id: JobId,
    mode: RenderMode,
    min_markdown_chars: usize,
    automation_script: Option<ArtifactRef>,
    etag_conditional: bool,
    vertical: VerticalOptions,
}

/// Acquired items plus any side-effect artifacts produced by this run (today,
/// at most one WARC archive — see [`super::warc`]) and any non-fatal
/// per-item warnings (isolated failures, Chrome-fallback degradations).
pub(super) struct AcquireOutcome {
    pub(super) items: Vec<AcquiredSourceItem>,
    pub(super) warnings: Vec<SourceWarning>,
    pub(super) artifacts: Vec<ArtifactRef>,
}

/// One item's acquisition outcome. `item` is `None` for a conditional-fetch
/// 304 skip. `warning` carries a non-fatal degradation alongside a
/// successful `item` (e.g. the `AutoSwitch` Chrome re-render failing, where
/// the HTTP render is kept as `item` and `warning` explains why).
#[derive(Debug)]
struct AcquiredItem {
    item: Option<AcquiredSourceItem>,
    warnings: Vec<SourceWarning>,
}

pub(super) async fn acquire_changed_items(
    plan: &SourcePlan,
    manifest_items: &[ManifestItem],
    fetch: &dyn FetchProvider,
    render: &dyn RenderProvider,
) -> Result<AcquireOutcome> {
    let values = &plan.route.validated_options.values;
    let opts = AcquireOptions {
        job_id: plan.job_id,
        mode: effective_render_mode(values),
        min_markdown_chars: min_markdown_chars(values),
        automation_script: automation_script_ref(values),
        etag_conditional: etag_conditional(values),
        vertical: VerticalOptions {
            enabled: verticals_enabled(values),
            auto_dispatch_skip: auto_dispatch_skip(values),
            user_agent: user_agent(values),
        },
    };
    let warc_path = warc_path(values);

    let (items, warnings) = match warc_path.as_deref() {
        Some(path) => {
            let mut warc_file = open_warc_archive(Some(path)).await?;
            acquire_sequential(fetch, render, manifest_items, &opts, &mut warc_file).await
        }
        None => acquire_concurrent(fetch, render, manifest_items, &opts).await,
    };

    let artifacts = match warc_path {
        Some(path) => vec![super::warc::artifact_ref(&path).await],
        None => Vec::new(),
    };
    Ok(AcquireOutcome {
        items,
        warnings,
        artifacts,
    })
}

/// One-at-a-time acquisition, used only when a WARC sink is configured (WARC
/// archival is an ordered on-disk log, so records must be written in
/// acquisition order). A failed item is logged and recorded as a
/// [`SourceWarning`] via [`resolve_item_outcome`] rather than aborting the
/// remaining items.
async fn acquire_sequential(
    fetch: &dyn FetchProvider,
    render: &dyn RenderProvider,
    manifest_items: &[ManifestItem],
    opts: &AcquireOptions,
    warc_file: &mut Option<tokio::fs::File>,
) -> (Vec<AcquiredSourceItem>, Vec<SourceWarning>) {
    let mut items = Vec::with_capacity(manifest_items.len());
    let mut warnings = Vec::new();
    for item in manifest_items {
        let outcome = acquire_item(fetch, render, item, opts).await;
        if let Some(acquired) = resolve_item_outcome(
            outcome,
            item.source_item_key.clone(),
            &item.canonical_uri,
            &mut warnings,
        ) {
            archive_to_warc(warc_file, &acquired).await;
            items.push(acquired);
        }
    }
    (items, warnings)
}

/// Bounded-concurrency acquisition (up to [`ACQUIRE_CONCURRENCY`] items in
/// flight at once), used whenever no WARC sink is configured. Each item is
/// an independent fetch/render round-trip, so returned item order is not
/// guaranteed to match `manifest_items`' order — see this module's doc
/// comment for why that's safe. A failed item is logged and recorded as a
/// [`SourceWarning`] rather than aborting the batch or discarding
/// already-succeeded siblings.
async fn acquire_concurrent(
    fetch: &dyn FetchProvider,
    render: &dyn RenderProvider,
    manifest_items: &[ManifestItem],
    opts: &AcquireOptions,
) -> (Vec<AcquiredSourceItem>, Vec<SourceWarning>) {
    let mut pending = stream::iter(manifest_items.to_vec())
        .map(|item| {
            let source_item_key = item.source_item_key.clone();
            let canonical_uri = item.canonical_uri.clone();
            async move {
                let outcome = acquire_item(fetch, render, &item, opts).await;
                (source_item_key, canonical_uri, outcome)
            }
        })
        .buffer_unordered(ACQUIRE_CONCURRENCY);

    let mut items = Vec::new();
    let mut warnings = Vec::new();
    while let Some((source_item_key, canonical_uri, outcome)) = pending.next().await {
        if let Some(acquired) =
            resolve_item_outcome(outcome, source_item_key, &canonical_uri, &mut warnings)
        {
            items.push(acquired);
        }
    }
    (items, warnings)
}

/// Shared per-item error isolation for both acquisition paths. A hard
/// per-item error (fetch/render failure propagated by [`acquire_item`]) is
/// logged and turned into a [`SourceWarning`] instead of aborting the batch.
/// A soft degradation warning carried alongside a successful item (e.g. the
/// `AutoSwitch` Chrome fallback failing) is also collected here. Returns the
/// acquired item, if any, for the caller to keep.
fn resolve_item_outcome(
    outcome: Result<AcquiredItem>,
    source_item_key: SourceItemKey,
    canonical_uri: &str,
    warnings: &mut Vec<SourceWarning>,
) -> Option<AcquiredSourceItem> {
    match outcome {
        Ok(AcquiredItem {
            item,
            warnings: item_warnings,
        }) => {
            warnings.extend(item_warnings);
            item
        }
        Err(err) => {
            log_warn(&format!(
                "web acquire_item_failed uri={canonical_uri} err={err}"
            ));
            warnings.push(SourceWarning {
                code: err.code.to_string(),
                severity: Severity::Warning,
                message: format!("failed to acquire {canonical_uri}: {err}"),
                source_item_key: Some(source_item_key),
                retryable: err.retryable,
            });
            None
        }
    }
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
) -> Result<AcquiredItem> {
    let mut warnings = Vec::new();
    match super::vertical::try_acquire(item, &opts.vertical, opts.job_id).await {
        VerticalAcquire::Handled(item) => {
            return Ok(AcquiredItem {
                item: Some(item),
                warnings,
            });
        }
        VerticalAcquire::Degraded(warning) => warnings.push(warning),
        VerticalAcquire::Unsupported => {}
    }

    match opts.mode {
        RenderMode::Http => {
            let fetched = acquire_via_fetch(fetch, item, opts.etag_conditional).await?;
            Ok(AcquiredItem {
                item: fetched,
                warnings,
            })
        }
        RenderMode::Chrome => {
            let rendered = render
                .render(build_render_request(
                    item,
                    RenderMode::Chrome,
                    opts.automation_script.clone(),
                ))
                .await?;
            Ok(AcquiredItem {
                item: Some(acquired_from_rendered(item, rendered, "chrome_render")),
                warnings,
            })
        }
        RenderMode::AutoSwitch => {
            acquire_via_auto_switch(
                render,
                item,
                opts.min_markdown_chars,
                opts.automation_script.clone(),
                warnings,
            )
            .await
        }
    }
}

/// `Http`-mode acquisition. A conditional `304 Not Modified` returns a
/// sentinel acquired item so the services layer can reuse the previous
/// committed representation or refetch before publish.
pub(crate) async fn acquire_via_fetch(
    fetch: &dyn FetchProvider,
    item: &ManifestItem,
    etag_conditional: bool,
) -> Result<Option<AcquiredSourceItem>> {
    let prior_etag = if etag_conditional {
        item.metadata.get("web_prior_etag").and_then(Value::as_str)
    } else {
        None
    };
    let sent_prior_validator = prior_etag.is_some();
    let fetched = fetch.fetch(build_fetch_request(item, prior_etag)).await?;
    if fetched.status == 304 {
        if !sent_prior_validator {
            return Err(ApiError::new(
                "web.fetch.invalid_304_without_validator",
                axon_error::ErrorStage::Fetching,
                format!(
                    "received 304 Not Modified for {} without sending a prior validator",
                    item.canonical_uri
                ),
            )
            .with_source_id(item.source_id.0.clone())
            .with_context("uri", item.canonical_uri.clone())
            .with_context(
                "etag_conditional",
                if etag_conditional { "true" } else { "false" },
            )
            .with_context(
                "has_web_prior_etag",
                if item.metadata.contains_key("web_prior_etag") {
                    "true"
                } else {
                    "false"
                },
            ));
        }
        let mut metadata = MetadataMap::new();
        metadata.insert(
            "web_fetch_method".to_string(),
            serde_json::json!("http_fetch_reuse"),
        );
        metadata.insert("web_render_mode".to_string(), serde_json::json!("http"));
        metadata.insert("web_status".to_string(), serde_json::json!(304));
        metadata.insert("web_reuse_required".to_string(), serde_json::json!(true));
        if let Some(etag) = prior_etag {
            metadata.insert("web_etag".to_string(), serde_json::json!(etag));
        }
        log_info(&format!(
            "web_etag_conditional: 304 Not Modified for {} — reusing prior committed content if available",
            item.canonical_uri,
        ));
        return Ok(Some(AcquiredSourceItem {
            manifest_item: item.clone(),
            fetch_status: LifecycleStatus::Completed,
            content_ref: ContentRef::External {
                uri: format!("reuse://{}", item.source_item_key.0),
                integrity: item.content_hash.clone(),
            },
            raw_artifact_id: None,
            headers: fetched.headers,
            fetched_at: fetched.fetched_at,
            metadata,
        }));
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
/// keeps the original HTTP render rather than failing the whole item, but is
/// no longer silent: it's logged and returned as a [`SourceWarning`] so
/// operators see the degradation to thin HTTP (PR #418 review).
async fn acquire_via_auto_switch(
    render: &dyn RenderProvider,
    item: &ManifestItem,
    min_markdown_chars: usize,
    automation_script: Option<ArtifactRef>,
    mut warnings: Vec<SourceWarning>,
) -> Result<AcquiredItem> {
    let first = render
        .render(build_render_request(
            item,
            RenderMode::Http,
            automation_script.clone(),
        ))
        .await?;
    if first.markdown.chars().count() >= min_markdown_chars {
        return Ok(AcquiredItem {
            item: Some(acquired_from_rendered(item, first, "auto_switch_http")),
            warnings,
        });
    }
    match render
        .render(build_render_request(
            item,
            RenderMode::Chrome,
            automation_script,
        ))
        .await
    {
        Ok(rendered) => Ok(AcquiredItem {
            item: Some(acquired_from_rendered(item, rendered, "auto_switch_chrome")),
            warnings,
        }),
        Err(err) => {
            log_warn(&format!(
                "auto_switch: chrome re-render failed for {} — keeping HTTP result: {err}",
                item.canonical_uri
            ));
            warnings.push(SourceWarning {
                code: "web.auto_switch.chrome_fallback_failed".to_string(),
                severity: Severity::Warning,
                message: format!(
                    "chrome re-render failed for {} — kept HTTP result: {err}",
                    item.canonical_uri
                ),
                source_item_key: Some(item.source_item_key.clone()),
                retryable: err.retryable,
            });
            Ok(AcquiredItem {
                item: Some(acquired_from_rendered(
                    item,
                    first,
                    "auto_switch_http_fallback",
                )),
                warnings,
            })
        }
    }
}

/// `prior_etag`, when present, is sent as `If-None-Match` — the caller
/// decides whether one applies (gated by `etag_conditional` and whether the
/// incoming item carries a `web_prior_etag`; see [`acquire_via_fetch`]).
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
