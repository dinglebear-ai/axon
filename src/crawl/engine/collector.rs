mod chrome_tasks;
mod manifest;
mod page;
mod util;

use chrome_tasks::{apply_thin_page_outcome, drain_chrome_tasks};
use manifest::write_page_to_manifest;
use util::{emit_progress, track_waf_block};

pub(super) use page::{CollectorConfig, PageOutcome, canonicalize_and_track_page, process_page};

use std::collections::HashSet;
use std::sync::Arc;

use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use super::thin_refetch::{RefetchResult, THIN_REFETCH_CONCURRENCY, write_refetch_results};
use super::{CrawlDiagnostic, CrawlSummary, PageEvent, canonicalize_url_for_dedupe};
use crate::core::logging::log_warn;

/// Extract the host of a URL for the rate-limit banner; empty string on parse failure.
fn host_of(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_string))
        .unwrap_or_default()
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
    let mut summary = CrawlSummary::default();
    summary.depth_max = col.max_depth;
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
        tx.send(summary.clone()).await.ok();
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
    let page_host = host_of(&url);
    let mut link_count: Option<u32> = None;
    if let Some(links) = page.page_links.as_ref() {
        link_count = Some(links.len() as u32);
        for link in links.iter() {
            let raw = link.as_ref();
            let link_host = host_of(raw);
            if (link_host.is_empty() || link_host == page_host)
                && let Some(canon) = canonicalize_url_for_dedupe(raw)
            {
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
    track_waf_block(
        page.waf_check,
        page.blocked_crawl,
        &url,
        &page.anti_bot_tech,
        summary,
    );

    let html_bytes: Vec<u8> = page.get_html_bytes_u8().to_vec();
    let outcome = process_page(&html_bytes, &url, col);
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
mod tests {
    use super::super::url_utils::MapScope;
    use super::*;
    use std::collections::HashMap;

    fn test_collector_config(scope: Option<MapScope>) -> CollectorConfig {
        CollectorConfig {
            markdown_dir: std::env::temp_dir(),
            manifest_path: std::env::temp_dir().join("collector-manifest.jsonl"),
            min_chars: 10,
            drop_thin: false,
            exclude_path_prefix: Vec::new(),
            scope,
            progress_tx: None,
            previous_manifest: Arc::new(HashMap::new()),
            selector_config: None,
            chrome_ws_url: None,
            chrome_timeout_secs: 1,
            output_dir: std::env::temp_dir(),
            ladder_thresholds: crate::core::content::LadderThresholds {
                strategy1: 30,
                strategy2: 200,
                body_multiplier: 2.0,
            },
            antibot_max_scan_bytes: 150_000,
            structured_max_bytes: 65_536,
            max_depth: 5,
            retry_backoff_ms: 250,
        }
    }

    #[test]
    fn canonicalize_and_track_page_rejects_same_host_root_outside_project_scope() {
        let col = test_collector_config(Some(MapScope {
            host: "example.github.io".to_string(),
            path_prefix: Some("/project".to_string()),
        }));
        let mut summary = CrawlSummary::default();
        let mut urls = HashSet::new();
        let mut seen = HashSet::new();

        let url = canonicalize_and_track_page(
            "https://example.github.io/",
            &col,
            &mut summary,
            &mut urls,
            &mut seen,
        );

        assert!(url.is_none());
        assert_eq!(summary.pages_seen, 0);
        assert!(urls.is_empty());
    }

    #[test]
    fn canonicalize_and_track_page_accepts_in_scope_project_page() {
        let col = test_collector_config(Some(MapScope {
            host: "example.github.io".to_string(),
            path_prefix: Some("/project".to_string()),
        }));
        let mut summary = CrawlSummary::default();
        let mut urls = HashSet::new();
        let mut seen = HashSet::new();

        let url = canonicalize_and_track_page(
            "https://example.github.io/project/docs/",
            &col,
            &mut summary,
            &mut urls,
            &mut seen,
        );

        assert_eq!(
            url.as_deref(),
            Some("https://example.github.io/project/docs")
        );
        assert_eq!(summary.pages_seen, 1);
        assert!(urls.contains("https://example.github.io/project/docs"));
    }
}
