use super::{CrawlDiagnostic, CrawlSummary, canonicalize_url_for_dedupe};
use crate::web_engine::manifest::ManifestEntry;
use axon_core::config::Config;
use axon_core::content::{build_selector_config, bytes_to_markdown, url_to_stable_filename};
use axon_core::http::axon_ua;
use axon_core::logging::{log_info, log_warn};
use futures_util::stream::{self, StreamExt};
use spider::page::Page;
use spider::website::Website;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Maximum number of concurrent Chrome fetches during thin-page re-fetch.
pub(super) const THIN_REFETCH_CONCURRENCY: usize = 4;

/// Outcome of a single per-URL Chrome re-fetch attempt.
pub(super) struct RefetchResult {
    pub url: String,
    /// `Some(markdown)` on success, `None` if the page is still thin or fetch failed.
    pub markdown: Option<String>,
    pub diagnostic: Option<CrawlDiagnostic>,
}

// Re-export the inline CDP renderer so the collector can call it directly.
pub(super) use super::cdp_render::render_html_with_chrome;

// ── Spider-based post-crawl re-fetch (batch fallback) ─────────────────────────

/// Build a minimal spider Website configured for a single-page Chrome fetch.
fn build_single_page_website(cfg: &Config, url: &str) -> Website {
    let mut website = Website::new(url);
    website.with_limit(1);
    website.with_block_assets(true);
    website.with_no_control_thread(true);
    if let Some(timeout_ms) = cfg.request_timeout_ms {
        website.with_request_timeout(Some(Duration::from_millis(timeout_ms)));
    }
    let retries = cfg.fetch_retries.min(u8::MAX as usize) as u8;
    if retries > 0 {
        website.with_retry(retries);
    }
    website.with_user_agent(Some(
        cfg.chrome_user_agent
            .as_deref()
            .unwrap_or_else(|| axon_ua()),
    ));
    if let Some(proxy) = cfg.chrome_proxy.as_deref() {
        website.with_proxies(Some(vec![proxy.to_string()]));
    }
    // Wire custom headers so `--header` applies to Chrome re-fetches too.
    if !cfg.custom_headers.is_empty() {
        let map = axon_core::http::parse_custom_headers(&cfg.custom_headers);
        if !map.is_empty() {
            website.with_headers(Some(map));
        }
    }
    // Wire SSRF blacklist so Chrome re-fetches cannot reach internal
    // services via DNS rebinding or redirects.
    website.with_blacklist_url(Some(
        axon_core::http::ssrf_blacklist_compact_strings().to_vec(),
    ));

    website
}

/// Fetch a single URL using Chrome via spider (makes a new HTTP request).
///
/// Used by the post-crawl batch fallback path when we don't have the HTML bytes.
async fn fetch_url_with_chrome(
    cfg: &Config,
    url: &str,
    min_chars: usize,
) -> (Option<String>, Option<CrawlDiagnostic>) {
    let website = build_single_page_website(cfg, url);
    let Ok(mut website) = super::super::browser::configure_spider_browser(
        cfg,
        website,
        axon_core::config::RenderMode::Chrome,
        super::super::browser::BrowserTimeoutPolicy::FloorForBrowserWork,
    )
    .await
    else {
        return (
            None,
            Some(
                CrawlDiagnostic::new(
                    "chrome_render",
                    "chrome_configuration_failed",
                    "shared Chrome configuration failed",
                )
                .with_url(url.to_string()),
            ),
        );
    };
    let mut rx = website.subscribe(16);

    let collect: tokio::task::JoinHandle<Option<Page>> =
        tokio::spawn(async move { rx.recv().await.ok() });

    // Crawl on its own task stack (see `fresh_stack`); the website moves in
    // and back out so retained-page fallbacks below still see the crawl.
    let mut crawl_site = std::mem::take(&mut website);
    website = crate::web_engine::fresh_stack::crawl_on_fresh_stack(async move {
        crawl_site.crawl().await;
        crawl_site
    })
    .await;
    website.unsubscribe();

    let page = match collect.await {
        Ok(Some(p)) => p,
        _ => {
            log_warn(&format!("thin_refetch: no page received for {url}"));
            return (
                None,
                Some(
                    CrawlDiagnostic::new(
                        "chrome_render",
                        "chrome_no_page",
                        "Chrome re-fetch completed without returning a page",
                    )
                    .with_url(url.to_string()),
                ),
            );
        }
    };

    if !page.status_code.is_success() {
        log_warn(&format!(
            "thin_refetch: HTTP {} for {url}",
            page.status_code.as_u16()
        ));
        return (
            None,
            Some(
                CrawlDiagnostic::new(
                    "chrome_render",
                    "chrome_non_2xx",
                    format!(
                        "Chrome re-fetch returned HTTP {}",
                        page.status_code.as_u16()
                    ),
                )
                .with_url(url.to_string())
                .with_http_status(page.status_code.as_u16()),
            ),
        );
    }

    let sel_cfg = build_selector_config(cfg);
    let trimmed = bytes_to_markdown(page.get_html_bytes_u8(), sel_cfg.as_ref());

    if trimmed.len() < min_chars {
        return (
            None,
            Some(
                CrawlDiagnostic::new(
                    "chrome_render",
                    "chrome_still_thin",
                    format!("Chrome re-fetch markdown below {min_chars} chars"),
                )
                .with_url(url.to_string()),
            ),
        );
    }

    (Some(trimmed), None)
}

/// Re-fetch thin pages with Chrome after the HTTP crawl completes.
///
/// This is the post-crawl batch fallback used when inline rendering was not
/// possible (Chrome URL not configured at crawl time). Only URLs that are still
/// in `http_summary.thin_urls` are re-fetched.
pub async fn chrome_refetch_thin_pages(
    cfg: &Config,
    http_summary: CrawlSummary,
    output_dir: &Path,
) -> CrawlSummary {
    let thin_urls: Vec<String> = http_summary.thin_urls.iter().cloned().collect();
    if thin_urls.is_empty() {
        return http_summary;
    }

    log_info(&format!(
        "auto-switch: re-fetching {} thin page(s) with Chrome (concurrency={})",
        thin_urls.len(),
        THIN_REFETCH_CONCURRENCY
    ));

    let min_chars = cfg.min_markdown_chars;
    // Wrap in Arc so each concurrent task gets a cheap reference clone rather
    // than a full deep clone of the Config struct.
    let cfg = Arc::new(cfg.clone());

    let results: Vec<RefetchResult> = stream::iter(thin_urls.iter().cloned())
        .map(|url| {
            let cfg = Arc::clone(&cfg);
            async move {
                let (markdown, diagnostic) = fetch_url_with_chrome(&cfg, &url, min_chars).await;
                RefetchResult {
                    url,
                    markdown,
                    diagnostic,
                }
            }
        })
        .buffer_unordered(THIN_REFETCH_CONCURRENCY)
        .collect()
        .await;

    write_refetch_results(http_summary, results, output_dir).await
}

/// Write a batch of `RefetchResult`s to disk and update the manifest.
///
/// Used by both the post-crawl batch path and the collector's inline Chrome path.
pub(super) async fn write_refetch_results(
    summary: CrawlSummary,
    results: Vec<RefetchResult>,
    output_dir: &Path,
) -> CrawlSummary {
    write_refetch_results_with_failure(summary, results, output_dir, None).await
}

mod commit;
use commit::*;

#[cfg(test)]
#[path = "thin_refetch_tests.rs"]
mod tests;
