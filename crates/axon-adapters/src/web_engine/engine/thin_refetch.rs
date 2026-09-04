use super::collector::sanitized_url_for_log;
use super::{CrawlDiagnostic, CrawlSummary, canonicalize_url_for_dedupe};
use crate::web_engine::manifest::ManifestEntry;
use axon_core::config::Config;
use axon_core::content::{build_selector_config, bytes_to_markdown, url_to_stable_filename};
use axon_core::http::axon_ua;
use axon_core::logging::{log_info, log_warn};
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
                .with_url(sanitized_url_for_log(url)),
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
            log_warn(&format!(
                "thin_refetch: no page received for {}",
                sanitized_url_for_log(url)
            ));
            return (
                None,
                Some(
                    CrawlDiagnostic::new(
                        "chrome_render",
                        "chrome_no_page",
                        "Chrome re-fetch completed without returning a page",
                    )
                    .with_url(sanitized_url_for_log(url)),
                ),
            );
        }
    };

    if !page.status_code.is_success() {
        log_warn(&format!(
            "thin_refetch: HTTP {} for {}",
            page.status_code.as_u16(),
            sanitized_url_for_log(url)
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
                .with_url(sanitized_url_for_log(url))
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
                .with_url(sanitized_url_for_log(url)),
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
) -> Result<CrawlSummary, String> {
    let thin_urls: Vec<String> = http_summary.thin_urls.iter().cloned().collect();
    if thin_urls.is_empty() {
        return Ok(http_summary);
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
    mut summary: CrawlSummary,
    results: Vec<RefetchResult>,
    output_dir: &Path,
) -> Result<CrawlSummary, String> {
    let markdown_dir = output_dir.join("markdown");
    let manifest_path = output_dir.join("manifest.jsonl");

    let mut manifest = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .read(true)
        .open(&manifest_path)
        .await
        .map_err(|error| format!("thin_refetch: failed to open manifest for append: {error}"))?;

    for result in results {
        if let Some(diagnostic) = result.diagnostic {
            summary.push_diagnostic(diagnostic);
        }
        let Some(markdown) = result.markdown else {
            continue;
        };
        let Some(canonical) = canonicalize_url_for_dedupe(&result.url) else {
            continue;
        };

        let filename = url_to_stable_filename(&canonical);
        let path = markdown_dir.join(&filename);
        let mut hasher = Sha256::new();
        hasher.update(markdown.as_bytes());
        let content_hash = hex::encode(hasher.finalize());

        let entry = ManifestEntry {
            url: canonical.clone(),
            relative_path: format!("markdown/{filename}"),
            markdown_chars: markdown.len(),
            content_hash: Some(content_hash),
            changed: true,
            // Thin-refetch Chrome re-render: raw HTML is not available here,
            // so structured data is absent. HTML bytes would need to be
            // threaded through RefetchResult to enable extraction.
            structured: None,
        };
        let mut line = serde_json::to_string(&entry)
            .map_err(|error| format!("thin_refetch: manifest serialize failed: {error}"))?;
        line.push('\n');

        publish_refetch_markdown(
            &mut manifest,
            &path,
            markdown.as_bytes(),
            line.as_bytes(),
            || Ok(()),
            || Ok(()),
            || Ok(()),
        )
        .await?;

        summary.thin_urls.remove(&canonical);
        summary.thin_pages = summary.thin_pages.saturating_sub(1);
        summary.markdown_files += 1;

        log_info(&format!(
            "thin_refetch: recovered {}",
            sanitized_url_for_log(&canonical)
        ));
    }

    Ok(summary)
}

/// Publish one recovered page as a single recoverable operation.
///
/// The previous markdown and manifest length are retained until the manifest
/// append has been flushed. Any later failure restores both, so a retry sees
/// the same pre-publication state.
async fn publish_refetch_markdown<F, B, R>(
    manifest: &mut tokio::fs::File,
    path: &Path,
    markdown: &[u8],
    manifest_line: &[u8],
    before_manifest_write: F,
    before_manifest_rollback: B,
    before_restore: R,
) -> Result<(), String>
where
    F: FnOnce() -> Result<(), String>,
    B: FnOnce() -> Result<(), String>,
    R: FnOnce() -> Result<(), String>,
{
    let manifest_len = manifest
        .metadata()
        .await
        .map_err(|error| format!("thin_refetch: failed to inspect manifest: {error}"))?
        .len();
    let tmp_path = path.with_extension("thin-refetch-tmp");
    let backup_path = path.with_extension("thin-refetch-backup");

    tokio::fs::write(&tmp_path, markdown)
        .await
        .map_err(|error| {
            format!(
                "thin_refetch: failed to write temporary file {}: {error}",
                tmp_path.display()
            )
        })?;

    let had_previous = tokio::fs::try_exists(path).await.map_err(|error| {
        format!(
            "thin_refetch: failed to inspect {}: {error}",
            path.display()
        )
    })?;
    if had_previous {
        if let Err(error) = tokio::fs::remove_file(&backup_path).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            let primary = format!(
                "thin_refetch: failed to clear stale rollback file {}: {error}",
                backup_path.display()
            );
            return Err(aggregate_temp_cleanup(primary, &tmp_path).await);
        }
        if let Err(error) = tokio::fs::rename(path, &backup_path).await {
            let primary = format!(
                "thin_refetch: failed to stage previous {}: {error}",
                path.display()
            );
            return Err(aggregate_temp_cleanup(primary, &tmp_path).await);
        }
    }
    if let Err(error) = tokio::fs::rename(&tmp_path, path).await {
        let publish_error = format!(
            "thin_refetch: failed to publish {}: {error}",
            path.display()
        );
        return match restore_previous_markdown(path, &backup_path, had_previous, || Ok(())).await {
            Ok(()) => Err(publish_error),
            Err(restore) => Err(format!(
                "{publish_error}; markdown restoration failed: {restore}"
            )),
        };
    }

    let publication = async {
        manifest
            .write_all(manifest_line)
            .await
            .map_err(|error| format!("thin_refetch: manifest write failed: {error}"))?;
        // The fault hook deliberately runs after the append so tests exercise
        // truncation of a partially published manifest, not only pre-write failures.
        before_manifest_write()?;
        manifest
            .flush()
            .await
            .map_err(|error| format!("thin_refetch: manifest flush failed: {error}"))?;
        Ok::<(), String>(())
    }
    .await;

    if let Err(error) = publication {
        let mut failures = vec![error];
        let rollback = match before_manifest_rollback() {
            Ok(()) => manifest
                .set_len(manifest_len)
                .await
                .map_err(|error| error.to_string()),
            Err(error) => Err(error),
        };
        if let Err(rollback) = rollback {
            failures.push(format!(
                "manifest rollback to {manifest_len} bytes failed: {rollback}"
            ));
        }
        if let Err(restore) =
            restore_previous_markdown(path, &backup_path, had_previous, before_restore).await
        {
            failures.push(format!("markdown restoration failed: {restore}"));
        }
        return Err(failures.join("; "));
    }

    if had_previous && let Err(error) = tokio::fs::remove_file(&backup_path).await {
        log_warn(&format!(
            "thin_refetch: retained rollback file {} after cleanup failure: {error}",
            backup_path.display()
        ));
    }
    Ok(())
}

async fn aggregate_temp_cleanup(primary: String, tmp_path: &Path) -> String {
    match tokio::fs::remove_file(tmp_path).await {
        Ok(()) => primary,
        Err(cleanup) => format!(
            "{primary}; temporary file cleanup failed for {}: {cleanup}",
            tmp_path.display()
        ),
    }
}

async fn restore_previous_markdown<F>(
    path: &Path,
    backup_path: &Path,
    had_previous: bool,
    before_restore: F,
) -> Result<(), String>
where
    F: FnOnce() -> Result<(), String>,
{
    before_restore()?;
    match tokio::fs::remove_file(path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("failed to remove {}: {error}", path.display())),
    }
    if had_previous {
        tokio::fs::rename(backup_path, path)
            .await
            .map_err(|error| {
                format!(
                    "failed to restore {} from {}: {error}",
                    path.display(),
                    backup_path.display()
                )
            })?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "thin_refetch_tests.rs"]
mod tests;
