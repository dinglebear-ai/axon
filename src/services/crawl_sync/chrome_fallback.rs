use crate::core::config::{Config, RenderMode};
use crate::core::logging::{log_info, log_warn};
use crate::core::ui::Spinner;
use crate::crawl::engine::{
    CrawlSummary, append_html_anchor_backfill, build_waf_diagnostics, chrome_refetch_thin_pages,
    run_crawl_once, should_fallback_to_chrome,
};
use crate::crawl::manifest::ManifestEntry;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChromeFallbackPlan {
    None,
    TargetedRefetch,
    HtmlBackfill,
}

pub(crate) fn plan_chrome_fallback(
    cfg: &Config,
    http_summary: &CrawlSummary,
) -> ChromeFallbackPlan {
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

pub(super) async fn maybe_chrome_fallback(
    cfg: &Config,
    start_url: &str,
    http_summary: CrawlSummary,
    mut http_seen_urls: HashSet<String>,
    previous_manifest: Arc<HashMap<String, ManifestEntry>>,
) -> (CrawlSummary, HashSet<String>) {
    let plan = plan_chrome_fallback(cfg, &http_summary);
    if matches!(plan, ChromeFallbackPlan::None) {
        return (http_summary, http_seen_urls);
    }

    log_auto_switch_warning(start_url, &http_summary);
    if let Some(updated) = maybe_refetch_waf_blocked(cfg, plan, &http_summary).await {
        return (updated, http_seen_urls);
    }
    if let Some(updated) = maybe_refetch_thin_pages(cfg, plan, &http_summary).await {
        return (updated, http_seen_urls);
    }
    let (html_summary, should_retry) =
        maybe_backfill_html_links(cfg, plan, start_url, &http_summary, &mut http_seen_urls).await;
    if !should_retry {
        return (html_summary, http_seen_urls);
    }

    let spinner = Spinner::new("HTTP yielded low coverage; retrying full crawl with Chrome");
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
            spinner.finish(&format!(
                "Chrome fallback complete (pages={}, markdown={})",
                chrome_summary.pages_seen, chrome_summary.markdown_files
            ));
            (chrome_summary, chrome_urls)
        }
        Err(err) => {
            spinner.finish(&format!(
                "Chrome fallback failed ({err}), using HTTP result"
            ));
            (html_summary, http_seen_urls)
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
    let spinner = Spinner::new(&format!(
        "HTTP yielded thin results; re-fetching {thin_count} thin page(s) with Chrome"
    ));
    let updated = chrome_refetch_thin_pages(cfg, summary.clone(), &cfg.output_dir).await;
    spinner.finish(&format!(
        "Chrome targeted re-fetch complete (pages={}, markdown={}, thin_remaining={})",
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
    let spinner = Spinner::new("HTTP yielded low coverage; backfilling discovered HTML links");
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
            spinner.finish(&format!(
                "HTML backfill complete (pages={}, markdown={})",
                html_summary.pages_seen, html_summary.markdown_files
            ));
            let should_retry = should_fallback_to_chrome(&html_summary, cfg.max_pages, cfg);
            (html_summary, should_retry)
        }
        Err(err) => {
            spinner.finish(&format!(
                "HTML backfill failed ({err}), retrying with Chrome"
            ));
            (html_summary, true)
        }
    }
}
