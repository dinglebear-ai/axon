use super::{CrawlSummary, canonicalize_url_for_dedupe};
use crate::core::config::Config;
use crate::core::content::{build_selector_config, bytes_to_markdown, url_to_filename};
use crate::core::logging::{log_info, log_warn};
use crate::crawl::manifest::ManifestEntry;
use futures_util::stream::{self, StreamExt};
use sha2::{Digest, Sha256};
use spider::page::Page;
use spider::website::Website;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

/// Maximum number of concurrent Chrome fetches during thin-page re-fetch.
pub(super) const THIN_REFETCH_CONCURRENCY: usize = 4;

/// Outcome of a single per-URL Chrome re-fetch attempt.
pub(super) struct RefetchResult {
    pub url: String,
    /// `Some(markdown)` on success, `None` if the page is still thin or fetch failed.
    pub markdown: Option<String>,
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
    if let Some(ua) = cfg.chrome_user_agent.as_deref() {
        website.with_user_agent(Some(ua));
    }
    if let Some(proxy) = cfg.chrome_proxy.as_deref() {
        website.with_proxies(Some(vec![proxy.to_string()]));
    }
    // Wire custom headers so `--header` applies to Chrome re-fetches too.
    if !cfg.custom_headers.is_empty() {
        let map = crate::core::http::parse_custom_headers(&cfg.custom_headers);
        if !map.is_empty() {
            website.with_headers(Some(map));
        }
    }
    // Wire SSRF blacklist so Chrome re-fetches cannot reach internal
    // services via DNS rebinding or redirects.
    website.with_blacklist_url(Some(
        crate::core::http::ssrf_blacklist_compact_strings().to_vec(),
    ));

    if cfg.bypass_csp {
        website.with_csp_bypass(true);
    }
    website.with_dismiss_dialogs(true);
    website.configuration.disable_log = true;
    if let Some(ref chrome_url) = cfg.chrome_remote_url {
        website.with_chrome_connection(Some(chrome_url.clone()));
    }
    website
}

/// Fetch a single URL using Chrome via spider (makes a new HTTP request).
///
/// Used by the post-crawl batch fallback path when we don't have the HTML bytes.
async fn fetch_url_with_chrome(cfg: &Config, url: &str, min_chars: usize) -> Option<String> {
    let mut website = build_single_page_website(cfg, url);
    let mut rx = website.subscribe(16);

    let collect: tokio::task::JoinHandle<Option<Page>> =
        tokio::spawn(async move { rx.recv().await.ok() });

    website.crawl().await;
    website.unsubscribe();

    let page = match collect.await {
        Ok(Some(p)) => p,
        _ => {
            log_warn(&format!("thin_refetch: no page received for {url}"));
            return None;
        }
    };

    if !page.status_code.is_success() {
        log_warn(&format!(
            "thin_refetch: HTTP {} for {url}",
            page.status_code.as_u16()
        ));
        return None;
    }

    let sel_cfg = build_selector_config(cfg);
    let trimmed = bytes_to_markdown(page.get_html_bytes_u8(), sel_cfg.as_ref());

    if trimmed.len() < min_chars {
        return None;
    }

    Some(trimmed)
}

/// Re-fetch thin pages with Chrome after the HTTP crawl completes.
///
/// This is the post-crawl batch fallback used when inline rendering was not
/// possible (Chrome URL not configured at crawl time). Only URLs that are still
/// in `http_summary.thin_urls` are re-fetched.
pub(crate) async fn chrome_refetch_thin_pages(
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
                let markdown = fetch_url_with_chrome(&cfg, &url, min_chars).await;
                RefetchResult { url, markdown }
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
    mut summary: CrawlSummary,
    results: Vec<RefetchResult>,
    output_dir: &Path,
) -> CrawlSummary {
    let markdown_dir = output_dir.join("markdown");
    let manifest_path = output_dir.join("manifest.jsonl");

    let Ok(file) = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&manifest_path)
        .await
    else {
        log_warn("thin_refetch: failed to open manifest for append; skipping disk writes");
        return summary;
    };
    let mut manifest = tokio::io::BufWriter::new(file);

    for result in results {
        let Some(markdown) = result.markdown else {
            continue;
        };
        let Some(canonical) = canonicalize_url_for_dedupe(&result.url) else {
            continue;
        };

        summary.thin_urls.remove(&canonical);
        summary.thin_pages = summary.thin_pages.saturating_sub(1);

        // Determine the filename using the would-be count (current + 1), then
        // only increment the counter after a successful write so we never have
        // an optimistic counter that needs rolling back on failure.
        let next_count = summary.markdown_files + 1;
        let filename = url_to_filename(&canonical, next_count);
        let path = markdown_dir.join(&filename);

        if let Err(e) = tokio::fs::write(&path, markdown.as_bytes()).await {
            log_warn(&format!(
                "thin_refetch: failed to write {}: {e}",
                path.display()
            ));
            // Undo the thin-page removals above since we didn't actually recover.
            summary.thin_pages += 1;
            summary.thin_urls.insert(canonical);
            continue;
        }

        // Write succeeded — now it is safe to count this file.
        summary.markdown_files += 1;

        let mut hasher = Sha256::new();
        hasher.update(markdown.as_bytes());
        let content_hash = hex::encode(hasher.finalize());

        let entry = ManifestEntry {
            url: canonical.clone(),
            relative_path: format!("markdown/{filename}"),
            markdown_chars: markdown.len(),
            content_hash: Some(content_hash),
            changed: true,
        };
        match serde_json::to_string(&entry) {
            Ok(mut line) => {
                line.push('\n');
                if let Err(e) = manifest.write_all(line.as_bytes()).await {
                    log_warn(&format!(
                        "thin_refetch: manifest write failed for {canonical}: {e}"
                    ));
                }
            }
            Err(e) => {
                log_warn(&format!(
                    "thin_refetch: manifest serialize failed for {canonical}: {e}"
                ));
            }
        }

        log_info(&format!("thin_refetch: recovered {canonical}"));
    }

    if let Err(e) = manifest.flush().await {
        log_warn(&format!("thin_refetch: manifest flush failed: {e}"));
    }

    summary
}
