//! Service-layer orchestration for synchronous (--wait true) crawl.
//!
//! Owns the full crawl lifecycle: disk cache check, HTTP crawl, Chrome fallback,
//! sitemap backfill, embed queueing, audit diff, and manifest finalization.
//! All business logic lives here; the CLI command is a thin formatting wrapper.

pub mod chrome_fallback;
mod source_ledger;

use crate::types::CrawlSyncResult;
use axon_core::config::{Config, ScrapeFormat};
use axon_core::content::url_to_domain;
use axon_core::logging::{log_done, log_warn};
use axon_core::ui::{Spinner, color_enabled_public};
use axon_crawl::engine::{
    CrawlSummary, build_waf_diagnostics, run_crawl_once, run_sitemap_only, update_latest_reflink,
};
use axon_crawl::manifest::{
    ManifestEntry, manifest_cache_is_stale, read_manifest_data, read_manifest_urls,
    write_audit_diff,
};
use chrome_fallback::maybe_chrome_fallback;
use source_ledger::embed_and_commit_sync_crawl_manifest_to_ledger;
#[cfg(test)]
pub(crate) use source_ledger::{
    crawl_changed_manifest_keys, crawl_manifest_to_ledger_items, crawl_source_identity,
};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::sync::Arc;

const DEFAULT_CACHE_TTL_SECS: u64 = 60 * 60;

/// Run a synchronous crawl for a single URL: cache check → HTTP crawl →
/// optional Chrome fallback → sitemap backfill → embed → audit diff.
///
/// Returns a typed result with aggregate stats. Intermediate progress is
/// emitted via Spinners (stderr) and structured logging.
pub async fn crawl_sync(cfg: &Config, start_url: &str) -> Result<CrawlSyncResult, Box<dyn Error>> {
    let mut sync_cfg = crawl_sync_effective_config(cfg, start_url);
    if cfg.sitemap_only {
        return run_sitemap_only_crawl(&sync_cfg, start_url).await;
    }

    let cfg = &mut sync_cfg;
    let domain = url_to_domain(start_url);

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

    if cfg.format == ScrapeFormat::Llm {
        stream_llm_output(cfg, &manifest_path).await;
    }

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

fn crawl_sync_effective_config(cfg: &Config, start_url: &str) -> Config {
    let domain = url_to_domain(start_url);
    let mut sync_cfg = Config {
        output_dir: cfg.output_dir.join("domains").join(&domain).join("sync"),
        ..cfg.clone()
    };
    // Same services-layer page-cap policy as the async path — keep them identical,
    // including sitemap-only sync crawls.
    sync_cfg.max_pages =
        crate::crawl::resolve_crawl_max_pages(cfg.max_pages, cfg.allow_unbounded_broad_crawl);
    sync_cfg
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

// ─── sitemap-only mode ────────────────────────────────────────────────────

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

// ─── crawl phase (HTTP + Chrome fallback) ─────────────────────────────────

async fn run_crawl_phase(
    cfg: &mut Config,
    start_url: &str,
    previous_manifest: Arc<HashMap<String, ManifestEntry>>,
) -> Result<(CrawlSummary, HashSet<String>), Box<dyn Error>> {
    let initial_mode = axon_crawl::chrome_bootstrap::resolve_initial_mode(cfg);
    let chrome_bootstrap = axon_crawl::chrome_bootstrap::bootstrap_chrome_runtime(cfg).await;
    for warning in &chrome_bootstrap.warnings {
        log_warn(&format!("[Chrome Bootstrap] {warning}"));
    }
    if let Some(ws_url) = chrome_bootstrap.resolved_ws_url {
        cfg.chrome_remote_url = Some(ws_url);
    }

    let (bar, progress_tx) = make_crawl_progress_bar(cfg, start_url);
    let (http_summary, http_seen_urls) = run_crawl_once(
        cfg,
        start_url,
        initial_mode,
        &cfg.output_dir,
        progress_tx,
        false,
        Arc::clone(&previous_manifest),
        None,
    )
    .await?;
    if let Some(pb) = bar {
        pb.finish_with_message(format!(
            "✓ Crawled {} pages · {} markdown",
            http_summary.pages_seen, http_summary.markdown_files
        ));
    }

    let (summary, seen_urls) = maybe_chrome_fallback(
        cfg,
        start_url,
        http_summary,
        http_seen_urls,
        previous_manifest,
    )
    .await?;

    Ok((summary, seen_urls))
}

// ─── sitemap backfill ─────────────────────────────────────────────────────

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
    let backfill_stats = axon_crawl::engine::append_sitemap_backfill(
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

// ─── finalize ────────────────────────────────────────────────────────────

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
        embed_and_commit_sync_crawl_manifest_to_ledger(cfg, start_url, manifest_path, &input)
            .await?;
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

// ─── LLM stream pass ──────────────────────────────────────────────────────

/// Read the crawl manifest and apply `to_llm_text()` to each markdown file,
/// streaming the results to stdout with `---` page delimiters.
///
/// Called when `cfg.format == ScrapeFormat::Llm` after the crawl completes.
/// Raw markdown on disk is left untouched — the embed pipeline continues to
/// read `output_dir/markdown/` and embeds the original text, not the LLM form.
///
/// Errors during individual page reads are logged and skipped so a single
/// unreadable file does not abort the entire stream.
async fn stream_llm_output(cfg: &Config, manifest_path: &std::path::Path) {
    let manifest = match read_manifest_data(manifest_path).await {
        Ok(m) => m,
        Err(err) => {
            log_warn(&format!(
                "crawl --format llm: failed to read manifest for LLM stream: {err}"
            ));
            return;
        }
    };
    if manifest.is_empty() {
        return;
    }

    // Sort by URL for deterministic output order.
    let mut entries: Vec<_> = manifest.values().collect();
    entries.sort_by(|a, b| a.url.cmp(&b.url));

    let markdown_dir = cfg.output_dir.join("markdown");
    let mut first = true;

    for entry in entries {
        let rel = std::path::Path::new(&entry.relative_path);
        // relative_path is stored as "markdown/<filename>" — strip the prefix.
        let rel_file = rel.strip_prefix("markdown").unwrap_or(rel);
        let path = markdown_dir.join(rel_file);

        let markdown = match tokio::fs::read_to_string(&path).await {
            Ok(md) => md,
            Err(err) => {
                log_warn(&format!(
                    "crawl --format llm: skipping unreadable file {}: {err}",
                    path.display()
                ));
                continue;
            }
        };

        let llm_text = axon_core::content::to_llm_text(&markdown, &entry.url);
        if !first {
            println!("\n---");
        }
        println!("{llm_text}");
        first = false;
    }
}

// ─── live progress helpers ────────────────────────────────────────────────

fn make_crawl_progress_bar(
    cfg: &Config,
    start_url: &str,
) -> (
    Option<indicatif::ProgressBar>,
    Option<tokio::sync::mpsc::Sender<CrawlSummary>>,
) {
    use indicatif::{ProgressBar, ProgressStyle};
    use std::io::IsTerminal;
    use std::time::Duration;
    use tokio::sync::mpsc;

    if cfg.json_output || cfg.quiet || !std::io::stderr().is_terminal() {
        return (None, None);
    }

    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(100));
    let tmpl = if color_enabled_public() {
        "{spinner:.cyan} {msg}"
    } else {
        "{spinner} {msg}"
    };
    pb.set_style(
        ProgressStyle::with_template(tmpl).unwrap_or_else(|_| ProgressStyle::default_spinner()),
    );
    pb.set_message(format!("Crawling {}…", start_url));

    let (tx, mut rx) = mpsc::channel::<CrawlSummary>(32);
    let pb2 = pb.clone();
    tokio::spawn(async move {
        while let Some(snap) = rx.recv().await {
            pb2.set_message(format!(
                "Crawling… {} pages · {} markdown · {} thin",
                snap.pages_seen, snap.markdown_files, snap.thin_pages
            ));
        }
    });

    (Some(pb), Some(tx))
}

#[cfg(test)]
#[path = "crawl_sync_tests.rs"]
mod tests;
