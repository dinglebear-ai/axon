//! Discover-side Chrome WAF/thin-refetch fallback chain for `Site`/`Docs`
//! scope (issue #298 Wave 2b regression fix).
//!
//! `crawl_manifest_items` (see `web/site_discovery.rs`) used to run a single
//! HTTP-or-Chrome pass with no fallback: a WAF-blocked page, a thin page, or a
//! JS-heavy site under `auto_switch` all silently shipped thinner/blocked
//! results than the legacy `axon crawl`/`axon map` path did. `crawl_sync`'s
//! `chrome_fallback` module (`crates/axon-services/src/crawl_sync/
//! chrome_fallback.rs`) still restores that quality for the CLI path, but
//! `axon-adapters` sits *below* `axon-services` in the crate dependency graph
//! (this crate depends only on `axon-api`/`axon-core`/`axon-error`/
//! `axon-observe`), so `discover` cannot import it — this module ports the
//! same sequencing against the relocated `web_engine::engine` functions
//! instead. Keep the two in sync by hand until #298 retires one of the two
//! callers.
//!
//! Sequencing (matches `crawl_sync::chrome_fallback::maybe_chrome_fallback`):
//! 1. WAF-blocked pages, if any: targeted stealth Chrome re-fetch.
//! 2. Otherwise thin pages, if any: targeted Chrome re-fetch.
//! 3. Otherwise (low overall coverage with neither WAF nor thin pages):
//!    HTTP-only HTML anchor backfill, then a full Chrome re-crawl if coverage
//!    is still low afterward. A Chrome failure keeps the backfilled HTTP
//!    result rather than failing discovery outright, unless the failure is a
//!    memory-guard abort, which must propagate.

use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::sync::Arc;

use axon_core::config::{Config, RenderMode};
use axon_core::logging::{log_info, log_warn};

use crate::web_engine::engine::{
    CrawlSummary, append_html_anchor_backfill, build_waf_diagnostics, chrome_refetch_thin_pages,
    memory_guard, run_crawl_once, should_fallback_to_chrome,
};
use crate::web_engine::manifest::ManifestEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChromeFallbackPlan {
    None,
    TargetedRefetch,
    HtmlBackfill,
}

fn plan_chrome_fallback(cfg: &Config, http_summary: &CrawlSummary) -> ChromeFallbackPlan {
    if !matches!(cfg.render_mode, RenderMode::AutoSwitch)
        || !should_fallback_to_chrome(http_summary, cfg.max_pages, cfg)
    {
        return ChromeFallbackPlan::None;
    }
    if http_summary.waf_blocked_pages > 0 && !http_summary.waf_blocked_urls.is_empty() {
        return ChromeFallbackPlan::TargetedRefetch;
    }
    if !http_summary.thin_urls.is_empty() {
        return ChromeFallbackPlan::TargetedRefetch;
    }
    ChromeFallbackPlan::HtmlBackfill
}

/// Apply the auto-switch Chrome fallback chain to an HTTP-mode `Site`/`Docs`
/// discover crawl. Returns the (possibly updated) summary and seen-URL set;
/// a `None` plan returns the input untouched.
pub(super) async fn maybe_chrome_fallback(
    cfg: &Config,
    start_url: &str,
    http_summary: CrawlSummary,
    mut http_seen_urls: HashSet<String>,
    previous_manifest: Arc<HashMap<String, ManifestEntry>>,
) -> Result<(CrawlSummary, HashSet<String>), Box<dyn Error>> {
    let plan = plan_chrome_fallback(cfg, &http_summary);
    if matches!(plan, ChromeFallbackPlan::None) {
        return Ok((http_summary, http_seen_urls));
    }

    log_auto_switch_warning(start_url, &http_summary);
    if let Some(updated) = maybe_refetch_waf_blocked(cfg, plan, &http_summary).await {
        return Ok((updated, http_seen_urls));
    }
    if let Some(updated) = maybe_refetch_thin_pages(cfg, plan, &http_summary).await {
        return Ok((updated, http_seen_urls));
    }
    let (html_summary, should_retry) =
        maybe_backfill_html_links(cfg, plan, start_url, &http_summary, &mut http_seen_urls).await;
    if !should_retry {
        return Ok((html_summary, http_seen_urls));
    }

    retry_full_chrome_crawl(
        cfg,
        start_url,
        html_summary,
        http_seen_urls,
        previous_manifest,
    )
    .await
}

/// Last resort of the chain: a full Chrome re-crawl when HTTP (even after
/// HTML backfill) still yields low coverage. A Chrome failure keeps the
/// backfilled HTTP result unless it is a memory-guard abort, which propagates.
async fn retry_full_chrome_crawl(
    cfg: &Config,
    start_url: &str,
    html_summary: CrawlSummary,
    http_seen_urls: HashSet<String>,
    previous_manifest: Arc<HashMap<String, ManifestEntry>>,
) -> Result<(CrawlSummary, HashSet<String>), Box<dyn Error>> {
    log_info(&format!(
        "crawl chrome_full_retry_start url={start_url} reason=low_http_coverage"
    ));
    match run_crawl_once(
        cfg,
        start_url,
        RenderMode::Chrome,
        &cfg.output_dir,
        None,
        cfg.discover_sitemaps,
        previous_manifest,
        None,
    )
    .await
    {
        Ok((chrome_summary, chrome_urls)) => {
            log_info(&format!(
                "crawl chrome_full_retry_complete url={start_url} pages={} markdown={}",
                chrome_summary.pages_seen, chrome_summary.markdown_files
            ));
            Ok((chrome_summary, chrome_urls))
        }
        Err(err) => {
            let err_msg = err.to_string();
            if memory_guard::is_memory_abort_message(&err_msg) {
                log_warn(&format!(
                    "crawl chrome_full_retry_aborted url={start_url} err={err_msg}"
                ));
                return Err(err);
            }
            log_warn(&format!(
                "crawl chrome_full_retry_failed url={start_url} err={err_msg}; using HTTP result"
            ));
            Ok((html_summary, http_seen_urls))
        }
    }
}

fn log_auto_switch_warning(start_url: &str, summary: &CrawlSummary) {
    let thin_ratio = if summary.pages_seen == 0 {
        1.0f64
    } else {
        summary.thin_pages as f64 / summary.pages_seen as f64
    };
    log_warn(&format!(
        "crawl auto_switch_to_chrome url={start_url} thin_ratio={thin_ratio:.2}"
    ));
}

async fn maybe_refetch_waf_blocked(
    cfg: &Config,
    plan: ChromeFallbackPlan,
    summary: &CrawlSummary,
) -> Option<CrawlSummary> {
    if !matches!(plan, ChromeFallbackPlan::TargetedRefetch)
        || summary.waf_blocked_pages == 0
        || summary.waf_blocked_urls.is_empty()
    {
        return None;
    }
    log_warn(&format!(
        "waf: {} page(s) blocked — retrying with stealth Chrome",
        summary.waf_blocked_pages
    ));
    let mut waf_summary = summary.clone();
    waf_summary.thin_urls = summary.waf_blocked_urls.clone();
    let updated = chrome_refetch_thin_pages(cfg, waf_summary, &cfg.output_dir).await;
    let remaining_urls = updated.thin_urls.clone();
    if let Some(diagnostics) = build_waf_diagnostics(summary, &updated, true, Some(&remaining_urls))
    {
        let message = format!(
            "waf: detected={} recovered={} remaining={}",
            diagnostics.detected_pages, diagnostics.recovered_pages, diagnostics.remaining_pages
        );
        if diagnostics.remaining_pages == 0 {
            log_info(&message);
        } else {
            log_warn(&message);
        }
    }
    Some(updated)
}

async fn maybe_refetch_thin_pages(
    cfg: &Config,
    plan: ChromeFallbackPlan,
    summary: &CrawlSummary,
) -> Option<CrawlSummary> {
    if !matches!(plan, ChromeFallbackPlan::TargetedRefetch) || summary.thin_urls.is_empty() {
        return None;
    }
    let thin_count = summary.thin_urls.len();
    log_info(&format!("crawl thin_refetch_start url_count={thin_count}"));
    let updated = chrome_refetch_thin_pages(cfg, summary.clone(), &cfg.output_dir).await;
    log_info(&format!(
        "crawl thin_refetch_complete pages={} markdown={} thin_remaining={}",
        updated.pages_seen, updated.markdown_files, updated.thin_pages,
    ));
    Some(updated)
}

async fn maybe_backfill_html_links(
    cfg: &Config,
    plan: ChromeFallbackPlan,
    start_url: &str,
    summary: &CrawlSummary,
    seen_urls: &mut HashSet<String>,
) -> (CrawlSummary, bool) {
    let mut html_summary = summary.clone();
    if !matches!(plan, ChromeFallbackPlan::HtmlBackfill) {
        return (html_summary, true);
    }
    log_info(&format!("crawl html_backfill_start url={start_url}"));
    match append_html_anchor_backfill(
        cfg,
        start_url,
        &cfg.output_dir,
        seen_urls,
        &mut html_summary,
    )
    .await
    {
        Ok(added_urls) => {
            for url in added_urls {
                seen_urls.insert(url);
            }
            log_info(&format!(
                "crawl html_backfill_complete pages={} markdown={}",
                html_summary.pages_seen, html_summary.markdown_files
            ));
            let should_retry = should_fallback_to_chrome(&html_summary, cfg.max_pages, cfg);
            (html_summary, should_retry)
        }
        Err(err) => {
            log_warn(&format!(
                "crawl html_backfill_failed url={start_url} err={err}; retrying with Chrome"
            ));
            (html_summary, true)
        }
    }
}

#[cfg(test)]
#[path = "chrome_fallback_tests.rs"]
mod tests;
