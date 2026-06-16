mod chrome_tasks;
mod manifest;
mod page;
mod util;

use chrome_tasks::{apply_thin_page_outcome, drain_chrome_tasks};
use manifest::write_page_to_manifest;
use util::{emit_progress, summary_with_adaptive, track_waf_block};

pub(super) use page::{CollectorConfig, PageOutcome, canonicalize_and_track_page, process_page};

use std::collections::HashSet;
use std::sync::Arc;

use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use super::thin_refetch::{RefetchResult, THIN_REFETCH_CONCURRENCY, write_refetch_results};
use super::{
    CrawlDiagnostic, CrawlSummary, PageEvent, canonicalize_url_for_dedupe, is_excluded_url_path,
    is_junk_discovered_url, normalize_map_candidate_url,
};
use crate::core::logging::log_warn;

/// Extract the host of a URL for the rate-limit banner; empty string on parse failure.
fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_string))
        .unwrap_or_default()
}

fn canonicalize_discovered_link(
    raw: &str,
    page_url: &str,
    col: &CollectorConfig,
) -> Option<String> {
    if is_junk_discovered_url(raw) || spider::utils::media_asset::is_media_asset_url(raw) {
        return None;
    }

    let base = url::Url::parse(page_url).ok()?;
    let resolved = base.join(raw).ok()?;
    let host = resolved.host_str()?;
    if !discovered_host_in_scope(host, base.host_str()?, col) {
        return None;
    }

    let resolved = resolved.as_str();
    if spider::utils::media_asset::is_media_asset_url(resolved) {
        return None;
    }
    if is_excluded_url_path(resolved, &col.exclude_path_prefix) {
        return None;
    }

    match col.scope.as_ref() {
        Some(scope) => normalize_map_candidate_url(resolved, scope, false),
        None => canonicalize_url_for_dedupe(resolved),
    }
}

fn discovered_host_in_scope(host: &str, page_host: &str, col: &CollectorConfig) -> bool {
    if let Some(scope) = col.scope.as_ref() {
        return host.eq_ignore_ascii_case(&scope.host);
    }

    let root = col.start_host.as_deref().unwrap_or(page_host);
    if host.eq_ignore_ascii_case(root) {
        return true;
    }
    col.include_subdomains
        && host
            .to_ascii_lowercase()
            .strip_suffix(&root.to_ascii_lowercase())
            .is_some_and(|rest| rest.ends_with('.'))
}

/// Apply the outcome of `process_page()`: update summary counters, spawn Chrome
/// renders for thin pages, write good pages to the manifest. Returns `true` when
/// the caller should `continue` (skip further per-page work).
#[expect(
    clippy::too_many_arguments,
    reason = "page outcome handling requires many mutable state refs"
)]
async fn apply_page_outcome(
    outcome: PageOutcome,
    html_bytes: Vec<u8>,
    url: &str,
    col: &CollectorConfig,
    summary: &mut CrawlSummary,
    manifest: &mut tokio::io::BufWriter<tokio::fs::File>,
    chrome_tasks: &mut JoinSet<RefetchResult>,
    chrome_semaphore: Arc<Semaphore>,
) -> Result<bool, String> {
    match outcome {
        PageOutcome::Thin {
            trimmed,
            content_hash,
        } => {
            return apply_thin_page_outcome(
                html_bytes,
                url,
                col,
                summary,
                manifest,
                chrome_tasks,
                chrome_semaphore,
                trimmed,
                content_hash,
            )
            .await;
        }
        PageOutcome::Empty => return Ok(true),
        PageOutcome::Challenged { ref vendor } => {
            tracing::warn!(
                vendor = %vendor,
                url = %url,
                "antibot.skipped: challenge page not embedded"
            );
            summary.push_diagnostic(
                CrawlDiagnostic::new(
                    "antibot",
                    "challenge_detected",
                    format!("challenge from {vendor}"),
                )
                .with_url(url.to_string()),
            );
            return Ok(true);
        }
        ref w @ (PageOutcome::Reused { .. } | PageOutcome::Write { .. }) => {
            apply_written_page_outcome(w, url, col, summary, manifest).await?;
        }
    }
    Ok(false)
}

fn record_adaptive_content_outcome(
    col: &CollectorConfig,
    outcome: &PageOutcome,
    waf_blocked: bool,
) {
    let Some(adaptive) = col.adaptive.as_ref() else {
        return;
    };
    if waf_blocked {
        adaptive.record_content_failure();
        return;
    }
    match outcome {
        PageOutcome::Reused { .. } | PageOutcome::Write { .. } => {
            adaptive.record_content_success();
        }
        PageOutcome::Challenged { .. } => adaptive.record_content_failure(),
        PageOutcome::Thin { .. } | PageOutcome::Empty => {}
    }
}

async fn apply_written_page_outcome(
    outcome: &PageOutcome,
    url: &str,
    col: &CollectorConfig,
    summary: &mut CrawlSummary,
    manifest: &mut tokio::io::BufWriter<tokio::fs::File>,
) -> Result<(), String> {
    let wrote = write_page_to_manifest(
        manifest,
        outcome,
        &col.markdown_dir,
        &col.previous_manifest,
        url,
    )
    .await?;
    if wrote {
        summary.markdown_files += 1;
        if matches!(outcome, PageOutcome::Reused { .. }) {
            summary.reused_pages += 1;
        }
    }
    Ok(())
}

/// Drives the spider broadcast subscription to collect, filter, render, and
/// persist crawled pages.
pub(super) async fn collect_crawl_pages(
    mut rx: tokio::sync::broadcast::Receiver<spider::page::Page>,
    col: CollectorConfig,
) -> Result<(CrawlSummary, HashSet<String>), String> {
    let manifest_file = tokio::fs::File::create(&col.manifest_path)
        .await
        .map_err(|e| format!("manifest create failed: {e}"))?;
    let mut manifest = tokio::io::BufWriter::new(manifest_file);
    let mut summary = CrawlSummary {
        depth_max: col.max_depth,
        ..Default::default()
    };
    let crawl_started = std::time::Instant::now();
    let mut urls = HashSet::new();
    let mut seen_canonical = HashSet::new();
    // Cumulative set of in-scope canonical links discovered across all pages.
    // `pages_discovered = discovered.len()`, so `queued = discovered − crawled`.
    let mut discovered: HashSet<String> = HashSet::new();
    let mut chrome_tasks: JoinSet<RefetchResult> = JoinSet::new();
    let mut chrome_results: Vec<RefetchResult> = Vec::new();
    let chrome_semaphore: Arc<Semaphore> = Arc::new(Semaphore::new(THIN_REFETCH_CONCURRENCY));
    let mut last_progress = std::time::Instant::now();

    loop {
        while let Some(r) = chrome_tasks.try_join_next() {
            match r {
                Ok(res) => chrome_results.push(res),
                Err(e) => log_warn(&format!("thin_refetch: Chrome task panicked: {e}")),
            }
        }

        let page = match rx.recv().await {
            Ok(p) => p,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                if let Some(adaptive) = col.adaptive.as_ref() {
                    adaptive.record_broadcast_lag(n);
                }
                log_warn(&format!(
                    "crawl broadcast lagged: {n} pages dropped — increase subscribe buffer or reduce concurrency"
                ));
                summary.push_diagnostic(
                    CrawlDiagnostic::new(
                        "collector",
                        "broadcast_lag",
                        format!("crawl broadcast lagged: {n} pages dropped"),
                    )
                    .with_dropped(n),
                );
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        };
        process_received_page(
            page,
            &col,
            &mut summary,
            &mut urls,
            &mut seen_canonical,
            &mut manifest,
            &mut chrome_tasks,
            chrome_semaphore.clone(),
            &mut last_progress,
            crawl_started,
            &mut discovered,
        )
        .await?;
    }

    drain_chrome_tasks(&mut chrome_tasks, &mut chrome_results).await;
    manifest
        .flush()
        .await
        .map_err(|e| format!("manifest flush failed: {e}"))?;
    if !chrome_results.is_empty() {
        summary = write_refetch_results(summary, chrome_results, &col.output_dir).await;
    }
    if let Some(tx) = col.progress_tx.as_ref() {
        tx.send(summary_with_adaptive(&col, &summary)).await.ok();
    }
    Ok((summary, urls))
}

#[allow(
    clippy::too_many_arguments,
    reason = "Collector step threads mutable crawl state and async task handles; kept explicit for clarity"
)]
async fn process_received_page(
    page: spider::page::Page,
    col: &CollectorConfig,
    summary: &mut CrawlSummary,
    urls: &mut HashSet<String>,
    seen_canonical: &mut HashSet<String>,
    manifest: &mut tokio::io::BufWriter<tokio::fs::File>,
    chrome_tasks: &mut JoinSet<RefetchResult>,
    chrome_semaphore: Arc<Semaphore>,
    last_progress: &mut std::time::Instant,
    crawl_started: std::time::Instant,
    discovered: &mut HashSet<String>,
) -> Result<(), String> {
    let Some(url) = canonicalize_and_track_page(page.get_url(), col, summary, urls, seen_canonical)
    else {
        return Ok(());
    };
    // Accumulate same-host discovered links into the cumulative backlog. Relative
    // links (no host) resolve same-site, so they count too.
    let mut link_count: Option<u32> = None;
    if let Some(links) = page.page_links.as_ref() {
        link_count = Some(links.len() as u32);
        for link in links.iter() {
            if let Some(canon) = canonicalize_discovered_link(link.as_ref(), &url, col) {
                discovered.insert(canon);
            }
        }
        summary.pages_discovered = (discovered.len() as u32).max(seen_canonical.len() as u32);
    }
    // Record a live per-page event for the palette's tailing log (every page —
    // success, error, or 429). 429s also register their host on the rate-limit banner.
    let status = page.status_code.as_u16();
    summary.push_event(PageEvent {
        t: crawl_started.elapsed().as_millis() as u64,
        url: url.clone(),
        status,
        links: link_count,
    });
    if status == 429 {
        summary.note_rate_limited(&host_of(&url), col.retry_backoff_ms);
    }
    if !page.status_code.is_success() {
        if let Some(adaptive) = col.adaptive.as_ref() {
            adaptive.record_status(status);
        }
        crate::core::logging::log_info(&format!(
            "skip: {} (HTTP {})",
            url,
            page.status_code.as_u16()
        ));
        summary.error_pages += 1;
        summary.push_diagnostic(
            CrawlDiagnostic::new(
                "http_fetch",
                "http_status",
                format!("skipped page with HTTP {}", page.status_code.as_u16()),
            )
            .with_url(url.clone())
            .with_http_status(page.status_code.as_u16()),
        );
        emit_progress(col, summary, last_progress).await;
        return Ok(());
    }
    let waf_blocked = page.waf_check || page.blocked_crawl;
    track_waf_block(
        page.waf_check,
        page.blocked_crawl,
        &url,
        &page.anti_bot_tech,
        summary,
    );

    let html_bytes: Vec<u8> = page.get_html_bytes_u8().to_vec();
    let outcome = process_page(&html_bytes, &url, col);
    record_adaptive_content_outcome(col, &outcome, waf_blocked);
    let _skip = apply_page_outcome(
        outcome,
        html_bytes,
        &url,
        col,
        summary,
        manifest,
        chrome_tasks,
        chrome_semaphore,
    )
    .await?;
    emit_progress(col, summary, last_progress).await;
    Ok(())
}

#[cfg(test)]
#[path = "collector_tests.rs"]
mod tests;
