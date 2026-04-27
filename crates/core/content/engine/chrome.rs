use super::super::deterministic::{DeterministicExtractionEngine, ExtractRun};
use super::{
    ExtractWebConfig, FallbackConfig, PageCollectResult, collect_page_results,
    run_single_url_extract,
};
use crate::crates::core::config::parse::is_docker_service_host;
use crate::crates::core::http::{cdp_discovery_url, http_client, ssrf_blacklist_patterns};
use spider::features::chrome_common::RequestInterceptConfiguration;
use spider::url::Url;
use spider::website::Website;
use std::error::Error;
use std::sync::Arc;

/// Pre-resolve the Chrome CDP WebSocket URL, mirroring the logic in
/// `crates/crawl/engine/runtime.rs::resolve_cdp_ws_url`.
///
/// - Already a `ws://`/`wss://` URL → return as-is (no extra round-trip).
/// - Inside Docker (`/.dockerenv` exists) → return the raw management URL;
///   spider resolves the WS URL itself on the Docker bridge network.
/// - Otherwise → fetch `/json/version`, extract `webSocketDebuggerUrl`, and
///   rewrite any Docker service hostname to `127.0.0.1` so the host CLI can
///   reach the Chrome proxy.
///
/// Falls back to `remote_url` unchanged when any step fails, so callers always
/// get a usable string even when the probe errors.
async fn resolve_chrome_url(remote_url: &str) -> String {
    if remote_url.starts_with("ws://") || remote_url.starts_with("wss://") {
        return remote_url.to_string();
    }

    if tokio::fs::try_exists("/.dockerenv").await.unwrap_or(false) {
        return remote_url.to_string();
    }

    let Some(discovery_url) = cdp_discovery_url(remote_url) else {
        return remote_url.to_string();
    };

    let Ok(client) = http_client() else {
        return remote_url.to_string();
    };

    let Ok(resp) = client.get(&discovery_url).send().await else {
        return remote_url.to_string();
    };

    let Ok(body) = resp.json::<serde_json::Value>().await else {
        return remote_url.to_string();
    };

    let Some(ws_url) = body.get("webSocketDebuggerUrl").and_then(|v| v.as_str()) else {
        return remote_url.to_string();
    };

    let Ok(mut parsed) = Url::parse(ws_url) else {
        return ws_url.to_string();
    };

    if let Some(host) = parsed.host_str() {
        let host = host.to_string();
        if is_docker_service_host(&host) {
            let _ = parsed.set_host(Some("127.0.0.1"));
        }
    }

    parsed.to_string()
}

/// Build a spider `Website` configured for single-page Chrome extraction.
///
/// Applies Chrome stealth, fingerprint patching, network intercept, CDP
/// connection, and network-idle wait. Returns `None` when `chrome_remote_url`
/// is absent — callers must fall back to the HTTP path in that case.
async fn build_chrome_extract_website(url: &str, wcfg: &ExtractWebConfig) -> Option<Website> {
    let chrome_url = wcfg.chrome_remote_url.as_deref()?;

    let resolved_url = resolve_chrome_url(chrome_url).await;

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
        // `idle_network0` waits until the network is fully quiet for 500 ms —
        // essential for CSR frameworks (React/Vue) that run XHR/fetch after the
        // initial HTML load. `idle_network` (EventLoadingFinished) fires too early.
        .with_wait_for_idle_network0(Some(spider::configuration::WaitForIdleNetwork::new(Some(
            std::time::Duration::from_secs(wcfg.chrome_network_idle_timeout_secs),
        ))))
        .with_chrome_intercept(RequestInterceptConfiguration::new(wcfg.chrome_intercept))
        .with_chrome_connection(Some(resolved_url));

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
    let Some(mut website) = build_chrome_extract_website(url, cfg).await else {
        // No Chrome configured — delegate to the HTTP path.
        return run_single_url_extract(
            url,
            http_client()?.clone(),
            engine,
            fallback_cfg,
            &cfg.custom_headers,
            cfg.user_agent.as_deref(),
        )
        .await;
    };

    let mut rx = website.subscribe(16);

    // Spider's canonical single-page Chrome fetch pattern:
    // tokio::join! runs crawl and collection concurrently on the same thread
    // (no tokio::spawn / no Send bound required).
    //
    // `sub` reads pages in arrival order. When the broadcast sender is dropped
    // (via website.unsubscribe() inside `crawl`), rx.recv() returns Err and the
    // loop exits cleanly — no race between a "done" signal and buffered pages.
    let crawl = async move {
        website.crawl().await;
        website.unsubscribe(); // drops the broadcast sender → signals EOF to rx
    };

    let sub = async move {
        let mut pages: Vec<spider::page::Page> = Vec::new();
        while let Ok(page) = rx.recv().await {
            pages.push(page);
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
