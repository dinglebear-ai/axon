use super::super::deterministic::{DeterministicExtractionEngine, ExtractRun};
use super::{
    ExtractWebConfig, FallbackConfig, PageCollectResult, collect_page_results,
    run_single_url_extract,
};
use crate::crates::core::http::{http_client, ssrf_blacklist_patterns};
use spider::features::chrome_common::RequestInterceptConfiguration;
use spider::website::Website;
use std::error::Error;
use std::sync::Arc;

/// Build a spider `Website` configured for single-page Chrome extraction.
///
/// Applies Chrome stealth, fingerprint patching, network intercept, and CDP
/// connection. Returns `None` when `chrome_remote_url` is absent — callers
/// must fall back to the HTTP path in that case.
fn build_chrome_extract_website(url: &str, wcfg: &ExtractWebConfig) -> Option<Website> {
    let chrome_url = wcfg.chrome_remote_url.as_deref()?;

    let ssrf_patterns: Vec<spider::compact_str::CompactString> = ssrf_blacklist_patterns()
        .iter()
        .copied()
        .map(Into::into)
        .collect();

    let mut website = Website::new(url);
    website
        .with_limit(1)
        // with_depth(0) prevents spider from discovering/queuing outbound links on the
        // seed page even when limit=1. Without it, spider still runs link-find callbacks
        // for every href on the page — wasted work for a single-page fetch.
        .with_depth(0)
        .with_blacklist_url(Some(ssrf_patterns))
        .with_stealth(wcfg.chrome_stealth || wcfg.chrome_anti_bot)
        .with_fingerprint(true)
        .with_dismiss_dialogs(true)
        .with_chrome_intercept(RequestInterceptConfiguration::new(wcfg.chrome_intercept))
        .with_chrome_connection(Some(chrome_url.to_string()));

    if wcfg.bypass_csp {
        website.with_csp_bypass(true);
    }
    if wcfg.accept_invalid_certs {
        website.with_danger_accept_invalid_certs(true);
    }
    if let Some(ua) = wcfg.user_agent.as_deref() {
        website.with_user_agent(Some(ua));
    }
    if let Some(timeout_ms) = wcfg.request_timeout_ms {
        website.with_request_timeout(Some(std::time::Duration::from_millis(timeout_ms)));
    }
    let retries = wcfg.fetch_retries.min(255) as u8;
    website.with_retry(retries);

    website.configuration.disable_log = true;

    Some(website)
}

/// Fetch a single URL via headless Chrome and extract structured data from it.
///
/// Uses spider's Chrome path (`website.crawl()`) with stealth and fingerprint
/// patching. Falls back to the HTTP path when Chrome is not configured
/// (`chrome_remote_url` is `None`).
pub(super) async fn run_single_url_extract_chrome(
    url: &str,
    engine: Arc<DeterministicExtractionEngine>,
    cfg: &ExtractWebConfig,
    fallback_cfg: FallbackConfig,
) -> Result<ExtractRun, Box<dyn Error>> {
    let Some(mut website) = build_chrome_extract_website(url, cfg) else {
        // No Chrome configured — delegate to the HTTP path.
        return run_single_url_extract(url, http_client()?.clone(), engine, fallback_cfg).await;
    };

    let mut rx = website.subscribe(16).ok_or("subscribe failed")?;

    // Spider's canonical single-page Chrome fetch pattern:
    // tokio::join! + oneshot avoids tokio::spawn (no Send bound required).
    // The biased select! checks done_rx first — exits the collect loop
    // immediately when crawl signals done, even if the channel hasn't closed.
    let (done_tx, mut done_rx) = tokio::sync::oneshot::channel::<()>();

    let crawl = async move {
        website.crawl().await;
        website.unsubscribe();
        let _ = done_tx.send(());
    };

    // Collect pages into a Vec; return it so tokio::join! can hand it back.
    let sub = async move {
        let mut pages: Vec<spider::page::Page> = Vec::new();
        loop {
            tokio::select! {
                biased;
                _ = &mut done_rx => break,
                result = rx.recv() => {
                    match result {
                        Ok(page) => pages.push(page),
                        Err(_) => break,
                    }
                }
            }
        }
        pages
    };

    let ((), pages) = tokio::join!(crawl, sub);

    // Feed collected pages back through collect_page_results via a replay channel.
    // For a single-URL Chrome extract we expect exactly 1 page.
    let http = http_client()?.clone();
    let (replay_tx, replay_rx) =
        tokio::sync::broadcast::channel::<spider::page::Page>(pages.len().max(1));
    for page in pages {
        let _ = replay_tx.send(page);
    }
    drop(replay_tx); // signal EOF immediately

    let PageCollectResult {
        results,
        pages_visited,
        pages_with_data,
        metrics,
        parser_hits,
    } = collect_page_results(replay_rx, http, Arc::clone(&engine), fallback_cfg).await;

    Ok(ExtractRun {
        start_url: url.to_string(),
        pages_visited,
        pages_with_data,
        results,
        metrics,
        parser_hits,
    })
}
