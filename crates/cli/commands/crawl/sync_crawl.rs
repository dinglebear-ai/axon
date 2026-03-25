//! Synchronous crawl orchestration with disk cache, sitemap-only mode, and HTTP-to-Chrome fallback.

use crate::crates::core::config::{Config, RenderMode};
use crate::crates::core::content::url_to_domain;
use crate::crates::core::logging::{log_done, log_warn};
use crate::crates::core::ui::{Spinner, accent, muted};
use crate::crates::crawl::engine::{
    CrawlSummary, append_html_anchor_backfill, chrome_refetch_thin_pages, run_crawl_once,
    run_sitemap_only, should_fallback_to_chrome, update_latest_reflink,
};
use crate::crates::crawl::manifest::{
    manifest_cache_is_stale, read_manifest_data, read_manifest_urls, write_audit_diff,
};
use crate::crates::services::embed::{embed_now, embed_start_with_input};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::sync::Arc;

const DEFAULT_CACHE_TTL_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChromeFallbackPlan {
    None,
    TargetedRefetch,
    HtmlBackfill,
}

pub(super) fn plan_chrome_fallback(
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

pub(super) async fn maybe_return_cached_result(
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

async fn run_sitemap_only_crawl(cfg: &Config, start_url: &str) -> Result<(), Box<dyn Error>> {
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
    Ok(())
}

async fn maybe_chrome_fallback(
    cfg: &Config,
    start_url: &str,
    http_summary: CrawlSummary,
    mut http_seen_urls: HashSet<String>,
    previous_manifest: Arc<HashMap<String, crate::crates::crawl::manifest::ManifestEntry>>,
) -> (CrawlSummary, HashSet<String>) {
    let plan = plan_chrome_fallback(cfg, &http_summary);
    if matches!(plan, ChromeFallbackPlan::None) {
        return (http_summary, http_seen_urls);
    }

    log_auto_switch_warning(start_url, &http_summary);
    if let Some(updated_summary) = maybe_refetch_waf_blocked(cfg, plan, &http_summary).await {
        return (updated_summary, http_seen_urls);
    }
    if let Some(updated_summary) = maybe_refetch_thin_pages(cfg, plan, &http_summary).await {
        return (updated_summary, http_seen_urls);
    }
    let (html_backfill_summary, should_retry_full_chrome) =
        maybe_backfill_html_links(cfg, plan, start_url, &http_summary, &mut http_seen_urls).await;
    if !should_retry_full_chrome {
        return (html_backfill_summary, http_seen_urls);
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
            (html_backfill_summary, http_seen_urls)
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
    let updated_summary = chrome_refetch_thin_pages(cfg, summary.clone(), &cfg.output_dir).await;
    spinner.finish(&format!(
        "Chrome targeted re-fetch complete (pages={}, markdown={}, thin_remaining={})",
        updated_summary.pages_seen, updated_summary.markdown_files, updated_summary.thin_pages,
    ));
    Some(updated_summary)
}

async fn maybe_backfill_html_links(
    cfg: &Config,
    plan: ChromeFallbackPlan,
    start_url: &str,
    summary: &CrawlSummary,
    seen_urls: &mut HashSet<String>,
) -> (CrawlSummary, bool) {
    let mut html_backfill_summary = summary.clone();
    if !matches!(plan, ChromeFallbackPlan::HtmlBackfill) {
        return (html_backfill_summary, true);
    }

    let spinner = Spinner::new("HTTP yielded low coverage; backfilling discovered HTML links");
    match append_html_anchor_backfill(
        cfg,
        start_url,
        &cfg.output_dir,
        seen_urls,
        &mut html_backfill_summary,
    )
    .await
    {
        Ok(added_urls) => {
            for url in added_urls {
                seen_urls.insert(url);
            }
            spinner.finish(&format!(
                "HTML backfill complete (pages={}, markdown={})",
                html_backfill_summary.pages_seen, html_backfill_summary.markdown_files
            ));
            let should_retry =
                should_fallback_to_chrome(&html_backfill_summary, cfg.max_pages, cfg);
            (html_backfill_summary, should_retry)
        }
        Err(err) => {
            spinner.finish(&format!(
                "HTML backfill failed ({err}), retrying with Chrome"
            ));
            (html_backfill_summary, true)
        }
    }
}

/// Bootstrap Chrome, run the initial HTTP crawl, and apply any Chrome fallback.
///
/// Returns `(summary, seen_urls, effective_cfg_holder)` — the caller owns the
/// `Config` holder so that `effective_cfg`'s lifetime extends past this call.
async fn run_crawl_phase(
    cfg: &mut Config,
    start_url: &str,
    previous_manifest: Arc<HashMap<String, crate::crates::crawl::manifest::ManifestEntry>>,
) -> Result<(CrawlSummary, HashSet<String>), Box<dyn Error>> {
    let initial_mode = super::runtime::resolve_initial_mode(cfg);
    let chrome_bootstrap = super::runtime::bootstrap_chrome_runtime(cfg).await;
    for warning in &chrome_bootstrap.warnings {
        println!("{} {}", muted("[Chrome Bootstrap]"), warning);
    }

    // Thread the pre-resolved WebSocket URL through cfg so configure_website
    // skips the redundant /json/version fetch on Chrome mode calls.
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

/// Queue an optional embed job, update the `latest` reflink, write the audit diff,
/// and emit the final structured log line.
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
        if cfg.lite_mode {
            let spinner = Spinner::new("embedding crawl output");
            embed_now(cfg, &input).await?;
            spinner.finish("embedded into Qdrant");
        } else {
            let embed_result = embed_start_with_input(cfg, &input, None, Some("crawl")).await?;
            let embed_job_id = embed_result.job_id;
            println!(
                "{} {}",
                muted("Queued embed job:"),
                accent(&embed_job_id.to_string())
            );
        }
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
        println!(
            "{} failed to update 'latest' reflink for domain {}: {err}",
            muted("[Warning]"),
            domain
        );
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

pub(super) async fn run_sync_crawl(cfg: &Config, start_url: &str) -> Result<(), Box<dyn Error>> {
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
        return Ok(());
    }

    let (mut final_summary, seen_urls) = run_crawl_phase(cfg, start_url, previous_manifest).await?;

    if cfg.discover_sitemaps {
        // Re-read the manifest to merge any URLs that were written to disk by run_crawl_once
        // but not surfaced in `seen_urls` (e.g. URLs discovered via Spider's own sitemap pass).
        // This prevents double-fetching pages that were already crawled.
        let merged_seen = {
            let manifest_urls = read_manifest_urls(&manifest_path).await.unwrap_or_default();
            seen_urls
                .iter()
                .cloned()
                .chain(manifest_urls)
                .collect::<HashSet<String>>()
        };
        let spinner = Spinner::new("running sitemap backfill");
        let backfill_stats = crate::crates::crawl::engine::append_sitemap_backfill(
            cfg,
            start_url,
            &cfg.output_dir,
            &merged_seen,
            &mut final_summary,
        )
        .await?;
        spinner.finish(&format!(
            "sitemap backfill complete (written={})",
            backfill_stats.written
        ));
    }

    finalize_crawl(
        cfg,
        start_url,
        &domain,
        &manifest_path,
        &previous_urls,
        &final_summary,
    )
    .await
}
