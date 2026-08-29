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
//! When `warc_path` is configured, acquisition preserves input order so the
//! services layer can build a deterministic WARC archive from the returned
//! items and store it through `ArtifactStore`. Without a WARC sink, returned
//! item order is **not** guaranteed to match the input `manifest_items` order —
//! safe today because every consumer of `fetched_items` keys off each item's
//! own embedded `manifest_item`, never positional correspondence.

use axon_api::source::*;
use axon_core::logging::log_warn;
use futures_util::stream::{self, StreamExt};
use serde_json::Value;
use std::time::{Duration, Instant};

use crate::adapter::{AcquisitionProgress, AcquisitionProgressSink, Result};
use crate::boundary::{FetchProvider, RenderProvider};

#[cfg(test)]
use super::binary::reject_binary_rendered_payload;
use super::binary::uri_has_pdf_path;
use super::fetch::acquire_via_fetch;
#[cfg(test)]
use super::fetch::build_fetch_request;
use super::options::{
    auto_dispatch_skip, automation_script_ref, cache_policy, effective_render_mode, headers,
    min_markdown_chars, render_metadata, user_agent, verticals_enabled, warc_path,
};
use super::render::{acquire_via_auto_switch, acquired_from_rendered, build_render_request};
use super::vertical::{VerticalAcquire, VerticalOptions};

/// Upper bound on in-flight `acquire_item` calls for [`acquire_concurrent`].
/// Chosen as a sane fixed default (matching `extract::sync`'s per-URL
/// concurrency) rather than wired to a perf profile — there is no existing
/// validated web-adapter option for it (see `axon-route::web_options`), and
/// adding one is a larger follow-up than this fix's scope.
const ACQUIRE_CONCURRENCY: usize = 16;

fn acquire_concurrency() -> usize {
    std::env::var("AXON_WEB_ACQUIRE_CONCURRENCY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(ACQUIRE_CONCURRENCY)
        .clamp(1, 128)
}

/// Options resolved once per [`acquire_changed_items`] call from
/// `plan.route.validated_options`, then threaded through every item so
/// per-item helpers stay free of `MetadataMap` lookups.
struct AcquireOptions {
    job_id: JobId,
    mode: RenderMode,
    min_markdown_chars: usize,
    automation_script: Option<ArtifactRef>,
    headers: Vec<RedactedHeader>,
    cache_policy: CachePolicy,
    render_metadata: MetadataMap,
    vertical: VerticalOptions,
}

/// Acquired items plus any non-fatal per-item warnings (isolated failures,
/// Chrome-fallback degradations).
pub(super) struct AcquireOutcome {
    pub(super) items: Vec<AcquiredSourceItem>,
    pub(super) warnings: Vec<SourceWarning>,
}

/// One item's acquisition outcome. `item` is `None` for a conditional-fetch
/// 304 skip. `warning` carries a non-fatal degradation alongside a
/// successful `item` (e.g. the `AutoSwitch` Chrome re-render failing, where
/// the HTTP render is kept as `item` and `warning` explains why).
#[derive(Debug)]
pub(super) struct AcquiredItem {
    pub(super) item: Option<AcquiredSourceItem>,
    pub(super) warnings: Vec<SourceWarning>,
}

#[derive(Clone, Copy, Debug)]
struct ItemTiming {
    elapsed: Duration,
    completed_at: Duration,
}

#[derive(Debug, PartialEq, Eq)]
struct AcquisitionTimingSummary {
    wall_ms: u128,
    first_completion_ms: u128,
    item_p50_ms: u128,
    item_p95_ms: u128,
    item_max_ms: u128,
    max_completion_gap_ms: u128,
    slot_occupancy_permille: u128,
}

fn percentile_ms(sorted: &[Duration], percentile: usize) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = sorted.len().saturating_mul(percentile).div_ceil(100);
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)].as_millis()
}

fn summarize_acquisition_timings(
    wall: Duration,
    concurrency: usize,
    timings: &[ItemTiming],
) -> AcquisitionTimingSummary {
    let mut elapsed = timings
        .iter()
        .map(|timing| timing.elapsed)
        .collect::<Vec<_>>();
    elapsed.sort_unstable();
    let mut completions = timings
        .iter()
        .map(|timing| timing.completed_at)
        .collect::<Vec<_>>();
    completions.sort_unstable();
    let max_completion_gap_ms = completions
        .windows(2)
        .map(|pair| pair[1].saturating_sub(pair[0]).as_millis())
        .max()
        .unwrap_or(0);
    let occupied_ms = elapsed.iter().map(Duration::as_millis).sum::<u128>();
    let capacity_ms = wall.as_millis().saturating_mul(concurrency as u128);
    let slot_occupancy_permille = if capacity_ms == 0 {
        0
    } else {
        occupied_ms.saturating_mul(1_000) / capacity_ms
    };

    AcquisitionTimingSummary {
        wall_ms: wall.as_millis(),
        first_completion_ms: completions.first().map(Duration::as_millis).unwrap_or(0),
        item_p50_ms: percentile_ms(&elapsed, 50),
        item_p95_ms: percentile_ms(&elapsed, 95),
        item_max_ms: elapsed.last().map(Duration::as_millis).unwrap_or(0),
        max_completion_gap_ms,
        slot_occupancy_permille,
    }
}

fn log_acquisition_timings(
    lane: &'static str,
    item_count: usize,
    concurrency: usize,
    wall: Duration,
    timings: &[ItemTiming],
) {
    let summary = summarize_acquisition_timings(wall, concurrency, timings);
    tracing::info!(
        lane,
        item_count,
        concurrency,
        wall_ms = summary.wall_ms,
        first_completion_ms = summary.first_completion_ms,
        item_p50_ms = summary.item_p50_ms,
        item_p95_ms = summary.item_p95_ms,
        item_max_ms = summary.item_max_ms,
        max_completion_gap_ms = summary.max_completion_gap_ms,
        slot_occupancy_permille = summary.slot_occupancy_permille,
        "web acquisition batch timing"
    );
}

pub(super) async fn acquire_changed_items(
    plan: &SourcePlan,
    manifest_items: &[ManifestItem],
    fetch: &dyn FetchProvider,
    render: &dyn RenderProvider,
    progress: Option<&dyn AcquisitionProgressSink>,
) -> Result<AcquireOutcome> {
    let values = &plan.route.validated_options.values;
    let opts = AcquireOptions {
        job_id: plan.job_id,
        mode: effective_render_mode(values),
        min_markdown_chars: min_markdown_chars(values),
        automation_script: automation_script_ref(values),
        headers: headers(values),
        cache_policy: cache_policy(values),
        render_metadata: render_metadata(values),
        vertical: VerticalOptions {
            enabled: verticals_enabled(values),
            auto_dispatch_skip: auto_dispatch_skip(values),
            user_agent: user_agent(values),
            cache_ttl_secs: values
                .get("vertical_cache_ttl_secs")
                .and_then(Value::as_object)
                .map(|entries| {
                    entries
                        .iter()
                        .filter_map(|(name, value)| value.as_u64().map(|ttl| (name.clone(), ttl)))
                        .collect()
                })
                .unwrap_or_default(),
        },
    };
    if opts.cache_policy == CachePolicy::Offline && !manifest_items.is_empty() {
        return Err(ApiError::new(
            "web.cache.offline_miss",
            ErrorStage::Fetching,
            "offline cache policy cannot acquire changed web items",
        )
        .with_context("changed_items", manifest_items.len().to_string()));
    }
    let warc_path = warc_path(values);

    let (items, warnings) = match warc_path.as_deref() {
        Some(_) => acquire_sequential(fetch, render, manifest_items, &opts, progress).await,
        None => acquire_concurrent(fetch, render, manifest_items, &opts, progress).await,
    };

    Ok(AcquireOutcome { items, warnings })
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
    progress: Option<&dyn AcquisitionProgressSink>,
) -> (Vec<AcquiredSourceItem>, Vec<SourceWarning>) {
    let batch_started = Instant::now();
    let mut items = Vec::with_capacity(manifest_items.len());
    let mut warnings = Vec::new();
    let mut timings = Vec::with_capacity(manifest_items.len());
    for (index, item) in manifest_items.iter().enumerate() {
        let item_started = Instant::now();
        let outcome = acquire_item(fetch, render, item, opts).await;
        timings.push(ItemTiming {
            elapsed: item_started.elapsed(),
            completed_at: batch_started.elapsed(),
        });
        if let Some(acquired) = resolve_item_outcome(
            outcome,
            item.source_item_key.clone(),
            &item.canonical_uri,
            &mut warnings,
        ) {
            items.push(acquired);
        }
        report_progress(progress, manifest_items.len(), index + 1, items.len()).await;
    }
    log_acquisition_timings(
        "sequential",
        manifest_items.len(),
        1,
        batch_started.elapsed(),
        &timings,
    );
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
    progress: Option<&dyn AcquisitionProgressSink>,
) -> (Vec<AcquiredSourceItem>, Vec<SourceWarning>) {
    let batch_started = Instant::now();
    let concurrency = acquire_concurrency().min(manifest_items.len().max(1));
    let mut pending = stream::iter(manifest_items.iter().cloned())
        .map(|item| {
            let source_item_key = item.source_item_key.clone();
            let canonical_uri = item.canonical_uri.clone();
            async move {
                let item_started = Instant::now();
                let outcome = acquire_item(fetch, render, &item, opts).await;
                let timing = ItemTiming {
                    elapsed: item_started.elapsed(),
                    completed_at: batch_started.elapsed(),
                };
                (source_item_key, canonical_uri, outcome, timing)
            }
        })
        .buffer_unordered(concurrency);

    let mut items = Vec::new();
    let mut warnings = Vec::new();
    let mut timings = Vec::with_capacity(manifest_items.len());
    let mut completed = 0usize;
    while let Some((source_item_key, canonical_uri, outcome, timing)) = pending.next().await {
        timings.push(timing);
        if let Some(acquired) =
            resolve_item_outcome(outcome, source_item_key, &canonical_uri, &mut warnings)
        {
            items.push(acquired);
        }
        completed += 1;
        report_progress(progress, manifest_items.len(), completed, items.len()).await;
    }
    log_acquisition_timings(
        "concurrent",
        manifest_items.len(),
        concurrency,
        batch_started.elapsed(),
        &timings,
    );
    (items, warnings)
}

async fn report_progress(
    sink: Option<&dyn AcquisitionProgressSink>,
    items_total: usize,
    items_done: usize,
    documents_done: usize,
) {
    let Some(sink) = sink else {
        return;
    };
    sink.report(AcquisitionProgress {
        items_total: items_total as u64,
        items_done: items_done as u64,
        documents_done: documents_done as u64,
    })
    .await;
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

async fn acquire_item(
    fetch: &dyn FetchProvider,
    render: &dyn RenderProvider,
    item: &ManifestItem,
    opts: &AcquireOptions,
) -> Result<AcquiredItem> {
    axon_core::http::validate_url(&item.canonical_uri).map_err(|err| {
        ApiError::new(
            "web.acquire.invalid_uri",
            ErrorStage::Resolving,
            format!("web target rejected by SSRF policy: {err}"),
        )
        .with_source_id(item.source_id.0.clone())
        .with_context("uri", item.canonical_uri.clone())
    })?;
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

    if uri_has_pdf_path(&item.canonical_uri) {
        let fetched = acquire_via_fetch(fetch, item, opts.cache_policy, &opts.headers).await?;
        let fetched = fetched.map(|mut acquired| {
            acquired.metadata.insert(
                "web_render_bypass_reason".to_string(),
                serde_json::json!("pdf_uri"),
            );
            acquired
        });
        return Ok(AcquiredItem {
            item: fetched,
            warnings,
        });
    }

    match opts.mode {
        RenderMode::Http => {
            let fetched = acquire_via_fetch(fetch, item, opts.cache_policy, &opts.headers).await?;
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
                    opts.render_metadata.clone(),
                ))
                .await?;
            Ok(AcquiredItem {
                item: Some(acquired_from_rendered(item, rendered, "chrome_render")?),
                warnings,
            })
        }
        RenderMode::AutoSwitch => {
            acquire_via_auto_switch(
                render,
                item,
                opts.min_markdown_chars,
                opts.automation_script.clone(),
                opts.render_metadata.clone(),
                warnings,
            )
            .await
        }
    }
}

/// `Http`-mode acquisition. A conditional `304 Not Modified` returns a
/// sentinel acquired item so the services layer can reuse the previous
/// committed representation or refetch before publish.
#[cfg(test)]
#[path = "acquire_tests.rs"]
mod tests;
