//! Service-layer orchestration for synchronous (--wait true) crawl.
//!
//! Owns the full crawl lifecycle: disk cache check, HTTP crawl, Chrome fallback,
//! sitemap backfill, embed queueing, audit diff, and manifest finalization.
//! All business logic lives here; the CLI command is a thin formatting wrapper.

use crate::core::config::{Config, RenderMode};
use crate::core::content::url_to_domain;
use crate::core::logging::{log_done, log_info, log_warn};
use crate::core::ui::Spinner;
use crate::crawl::engine::{
    CrawlSummary, append_html_anchor_backfill, build_waf_diagnostics, chrome_refetch_thin_pages,
    run_crawl_once, run_sitemap_only, should_fallback_to_chrome, update_latest_reflink,
};
use crate::crawl::manifest::{
    manifest_cache_is_stale, read_manifest_data, read_manifest_urls, write_audit_diff,
};
use crate::services::embed::embed_now_with_source;
use crate::services::types::CrawlSyncResult;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::sync::Arc;

const DEFAULT_CACHE_TTL_SECS: u64 = 24 * 60 * 60;

/// Run a synchronous crawl for a single URL: cache check → HTTP crawl →
/// optional Chrome fallback → sitemap backfill → embed → audit diff.
///
/// Returns a typed result with aggregate stats. Intermediate progress is
/// emitted via Spinners (stderr) and structured logging.
pub async fn crawl_sync(cfg: &Config, start_url: &str) -> Result<CrawlSyncResult, Box<dyn Error>> {
    if cfg.sitemap_only {
        return run_sitemap_only_crawl(cfg, start_url).await;
    }

    let domain = url_to_domain(start_url);
    let mut sync_cfg = Config {
        output_dir: cfg.output_dir.join("domains").join(&domain).join("sync"),
        ..cfg.clone()
    };
    let cfg = &mut sync_cfg;

    let manifest_path = cfg.output_dir.join("manifest.jsonl");
    let previous_manifest = Arc::new(if cfg.cache {
        read_manifest_data(&manifest_path).await?
    } else {
        HashMap::new()
    });
    let previous_urls: HashSet<String> = previous_manifest.keys().cloned().collect();

    if maybe_return_cached_result(cfg, start_url, &manifest_path, &previous_urls).await? {
        return Ok(CrawlSyncResult {
            pages_seen: previous_urls.len() as u32,
            markdown_files: 0,
            thin_pages: 0,
            error_pages: 0,
            waf_blocked_pages: 0,
            waf_diagnostics: None,
            elapsed_ms: 0,
            cache_hit: true,
        });
    }

    let (mut final_summary, seen_urls) = run_crawl_phase(cfg, start_url, previous_manifest).await?;

    if cfg.discover_sitemaps {
        run_sitemap_backfill(
            cfg,
            start_url,
            &manifest_path,
            &seen_urls,
            &mut final_summary,
        )
        .await?;
    }

    finalize_crawl(
        cfg,
        start_url,
        &domain,
        &manifest_path,
        &previous_urls,
        &final_summary,
    )
    .await?;

    Ok(CrawlSyncResult {
        pages_seen: final_summary.pages_seen,
        markdown_files: final_summary.markdown_files,
        thin_pages: final_summary.thin_pages,
        error_pages: final_summary.error_pages,
        waf_blocked_pages: final_summary.waf_blocked_pages,
        waf_diagnostics: build_waf_diagnostics(&final_summary, &final_summary, false, None),
        elapsed_ms: final_summary.elapsed_ms,
        cache_hit: false,
    })
}

// ─── cache ─────────────────────────────────────────────────────────────────

async fn maybe_return_cached_result(
    cfg: &Config,
    start_url: &str,
    manifest_path: &std::path::Path,
    previous_urls: &HashSet<String>,
) -> Result<bool, Box<dyn Error>> {
    let cache_stale = manifest_cache_is_stale(manifest_path, DEFAULT_CACHE_TTL_SECS).await;
    if !cfg.cache || previous_urls.is_empty() || cache_stale {
        return Ok(false);
    }
    let (report_path, _) = write_audit_diff(
        &cfg.output_dir,
        start_url,
        previous_urls,
        previous_urls,
        true,
        Some(manifest_path.to_string_lossy().to_string()),
    )
    .await?;
    log_done(&format!(
        "command=crawl cache_hit=true cached_urls={} output_dir={} audit_report={}",
        previous_urls.len(),
        cfg.output_dir.to_string_lossy(),
        report_path.to_string_lossy()
    ));
    Ok(true)
}

// ─── sitemap-only mode ───────────────────────────��─────────────────────────

async fn run_sitemap_only_crawl(
    cfg: &Config,
    start_url: &str,
) -> Result<CrawlSyncResult, Box<dyn Error>> {
    let spinner = Spinner::new("running sitemap-only crawl");
    let (summary, _) =
        run_sitemap_only(cfg, start_url, &cfg.output_dir, Arc::new(HashMap::new())).await?;
    spinner.finish(&format!(
        "sitemap-only complete (pages={}, markdown={})",
        summary.pages_seen, summary.markdown_files
    ));
    log_done(&format!(
        "command=crawl sitemap_only=true pages_seen={} markdown_files={} elapsed_ms={} output_dir={}",
        summary.pages_seen,
        summary.markdown_files,
        summary.elapsed_ms,
        cfg.output_dir.to_string_lossy(),
    ));
    Ok(CrawlSyncResult {
        pages_seen: summary.pages_seen,
        markdown_files: summary.markdown_files,
        thin_pages: summary.thin_pages,
        error_pages: summary.error_pages,
        waf_blocked_pages: summary.waf_blocked_pages,
        waf_diagnostics: build_waf_diagnostics(&summary, &summary, false, None),
        elapsed_ms: summary.elapsed_ms,
        cache_hit: false,
    })
}

// ─── crawl phase (HTTP + Chrome fallback) ──────────────────────────────────

async fn run_crawl_phase(
    cfg: &mut Config,
    start_url: &str,
    previous_manifest: Arc<HashMap<String, crate::crawl::manifest::ManifestEntry>>,
) -> Result<(CrawlSummary, HashSet<String>), Box<dyn Error>> {
    let initial_mode = crate::crawl::chrome_bootstrap::resolve_initial_mode(cfg);
    let chrome_bootstrap = crate::crawl::chrome_bootstrap::bootstrap_chrome_runtime(cfg).await;
    for warning in &chrome_bootstrap.warnings {
        log_warn(&format!("[Chrome Bootstrap] {warning}"));
    }
    if let Some(ws_url) = chrome_bootstrap.resolved_ws_url {
        cfg.chrome_remote_url = Some(ws_url);
    }

    let spinner = Spinner::new("running crawl");
    let (http_summary, http_seen_urls) = run_crawl_once(
        cfg,
        start_url,
        initial_mode,
        &cfg.output_dir,
        None,
        false,
        Arc::clone(&previous_manifest),
        None,
    )
    .await?;
    spinner.finish(&format!(
        "crawl phase complete (pages={}, markdown={})",
        http_summary.pages_seen, http_summary.markdown_files
    ));

    let (summary, seen_urls) = maybe_chrome_fallback(
        cfg,
        start_url,
        http_summary,
        http_seen_urls,
        previous_manifest,
    )
    .await;

    Ok((summary, seen_urls))
}

// ─── Chrome fallback ──────────────��────────────────────────────────────────

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

async fn maybe_chrome_fallback(
    cfg: &Config,
    start_url: &str,
    http_summary: CrawlSummary,
    mut http_seen_urls: HashSet<String>,
    previous_manifest: Arc<HashMap<String, crate::crawl::manifest::ManifestEntry>>,
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

// ─── sitemap backfill ──────────────────────────────────────────────────────

async fn run_sitemap_backfill(
    cfg: &Config,
    start_url: &str,
    manifest_path: &std::path::Path,
    seen_urls: &HashSet<String>,
    final_summary: &mut CrawlSummary,
) -> Result<(), Box<dyn Error>> {
    let merged_seen = {
        let manifest_urls = read_manifest_urls(manifest_path).await.unwrap_or_default();
        seen_urls
            .iter()
            .cloned()
            .chain(manifest_urls)
            .collect::<HashSet<String>>()
    };
    let spinner = Spinner::new("running sitemap backfill");
    let backfill_stats = crate::crawl::engine::append_sitemap_backfill(
        cfg,
        start_url,
        &cfg.output_dir,
        &merged_seen,
        final_summary,
    )
    .await?;
    spinner.finish(&format!(
        "sitemap backfill complete (written={})",
        backfill_stats.written
    ));
    Ok(())
}

// ─── finalize ────────────────���─────────────────────────────────────────────

async fn finalize_crawl(
    cfg: &Config,
    start_url: &str,
    domain: &str,
    manifest_path: &std::path::Path,
    previous_urls: &HashSet<String>,
    final_summary: &CrawlSummary,
) -> Result<(), Box<dyn Error>> {
    if cfg.embed {
        let markdown_dir = cfg.output_dir.join("markdown");
        let input = markdown_dir.to_string_lossy().to_string();
        let spinner = Spinner::new("embedding crawl output");
        embed_now_with_source(cfg, &input, Some("crawl")).await?;
        spinner.finish("embedded into Qdrant");
    }

    let current_urls = read_manifest_urls(manifest_path).await?;

    let latest_dir = cfg
        .output_dir
        .parent()
        .ok_or_else(|| {
            format!(
                "output_dir '{}' has no parent directory",
                cfg.output_dir.display()
            )
        })?
        .join("latest");
    if let Err(err) = update_latest_reflink(&cfg.output_dir, &latest_dir).await {
        log_warn(&format!(
            "failed to update 'latest' reflink for domain {domain}: {err}"
        ));
    }

    let (report_path, _) = write_audit_diff(
        &cfg.output_dir,
        start_url,
        previous_urls,
        &current_urls,
        false,
        None,
    )
    .await?;
    log_done(&format!(
        "command=crawl pages_seen={} markdown_files={} thin_pages={} error_pages={} waf_blocked={} elapsed_ms={} output_dir={} audit_report={}",
        final_summary.pages_seen,
        final_summary.markdown_files,
        final_summary.thin_pages,
        final_summary.error_pages,
        final_summary.waf_blocked_pages,
        final_summary.elapsed_ms,
        cfg.output_dir.to_string_lossy(),
        report_path.to_string_lossy(),
    ));
    Ok(())
}
