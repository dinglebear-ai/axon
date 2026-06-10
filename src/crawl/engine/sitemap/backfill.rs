//! Post-crawl backfill: fetch sitemap/llms.txt-discovered URLs the crawler
//! missed, convert to markdown, and append to the crawl manifest.

use super::discover::discover_sitemap_urls;
use super::filter::is_already_markdown;
use super::{fetch_text_with_retry, request_timeout_secs};
use crate::core::config::Config;
use crate::core::content::{build_selector_config, to_markdown, url_to_stable_filename};
use crate::core::http::build_client;
use crate::core::logging::log_info;
use crate::crawl::engine::CrawlSummary;
use crate::crawl::manifest::ManifestEntry;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::error::Error;
use std::path::Path;
use tokio::io::{AsyncWriteExt, BufWriter};

/// Stats returned by [`append_sitemap_backfill`].
#[derive(Debug, Clone, Default)]
pub struct BackfillStats {
    /// Total URLs discovered from sitemaps (before filtering).
    pub discovered_urls: usize,
    /// URLs that passed the `seen_urls` + manifest dedup filter.
    pub candidates: usize,
    /// URLs fetched successfully (HTTP 2xx).
    pub fetched_ok: usize,
    /// Markdown files actually written to disk + manifest.
    pub written: usize,
    /// URLs that failed validation, fetch, or I/O.
    pub failed: usize,
}

/// Fetch `url`, convert to markdown, and classify as thin/dropped.
/// Returns `(url, None)` on fetch failure, `(url, Some(...))` otherwise.
async fn fetch_and_convert_backfill_url(
    http: reqwest::Client,
    url: String,
    retries: usize,
    backoff: u64,
    min_chars: usize,
    drop_thin: bool,
    selector_config: Option<spider_transformations::transformation::content::SelectorConfiguration>,
) -> (String, Option<(String, usize, bool, bool)>) {
    // HTML page backfill: pass `None` to preserve `main`'s uncapped, charset-aware decode.
    // Real HTML pages can exceed the discovery cap and may not be strict UTF-8.
    let Some(html) = fetch_text_with_retry(&http, &url, retries, backoff, None).await else {
        return (url, None);
    };
    let trimmed = if is_already_markdown(&url) {
        // Already markdown/plaintext — pass through verbatim, do not run the HTML transform.
        html.trim().to_string()
    } else {
        to_markdown(&html, selector_config.as_ref())
    };
    let markdown_chars = trimmed.len();
    let is_thin = markdown_chars < min_chars;
    let dropped = is_thin && drop_thin;
    (url, Some((trimmed, markdown_chars, is_thin, dropped)))
}

/// Open (or create) a manifest file in append mode wrapped in a `BufWriter`.
async fn open_append_manifest(
    manifest_path: &Path,
) -> Result<BufWriter<tokio::fs::File>, Box<dyn Error>> {
    let file = tokio::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(manifest_path)
        .await?;
    Ok(BufWriter::new(file))
}

/// Filter `candidates` to those not already in `seen_urls` or the on-disk manifest.
async fn filter_seen_candidates(
    manifest_path: &Path,
    seen_urls: &HashSet<String>,
    candidates: Vec<String>,
) -> Result<Vec<String>, Box<dyn Error>> {
    let previous_manifest = crate::crawl::manifest::read_manifest_data(manifest_path).await?;
    let manifest_urls: HashSet<String> = previous_manifest.keys().cloned().collect();
    Ok(candidates
        .into_iter()
        .filter(|url| !seen_urls.contains(url) && !manifest_urls.contains(url))
        .collect())
}

/// Write a single backfill page to disk and append its entry to the manifest.
async fn write_backfill_entry(
    manifest: &mut BufWriter<tokio::fs::File>,
    markdown_dir: &Path,
    url: &str,
    trimmed: &str,
    markdown_chars: usize,
) -> Result<(), Box<dyn Error>> {
    let mut hasher = Sha256::new();
    hasher.update(trimmed.as_bytes());
    let content_hash = hex::encode(hasher.finalize());

    let filename = url_to_stable_filename(url);
    let file = markdown_dir.join(&filename);
    tokio::fs::write(&file, trimmed.as_bytes()).await?;

    let entry = ManifestEntry {
        url: url.to_string(),
        relative_path: format!("markdown/{filename}"),
        markdown_chars,
        content_hash: Some(content_hash),
        changed: true,
        // Sitemap backfill fetches plain HTTP responses — raw HTML is not
        // available at manifest-write time, so structured data is absent.
        structured: None,
    };
    let mut line = serde_json::to_string(&entry)?;
    line.push('\n');
    manifest.write_all(line.as_bytes()).await?;
    Ok(())
}

pub(crate) async fn append_candidate_backfill(
    cfg: &Config,
    output_dir: &Path,
    seen_urls: &HashSet<String>,
    candidates: Vec<String>,
    summary: &mut CrawlSummary,
) -> Result<(BackfillStats, Vec<String>), Box<dyn Error>> {
    let manifest_path = output_dir.join("manifest.jsonl");
    let candidates = filter_seen_candidates(&manifest_path, seen_urls, candidates)
        .await
        .map_err(|e| {
            format!(
                "failed to filter backfill candidates from {}: {e}",
                manifest_path.display()
            )
        })?;

    if candidates.is_empty() {
        return Ok((BackfillStats::default(), Vec::new()));
    }

    let markdown_dir = output_dir.join("markdown");
    tokio::fs::create_dir_all(&markdown_dir)
        .await
        .map_err(|e| {
            format!(
                "failed to create backfill markdown dir {}: {e}",
                markdown_dir.display()
            )
        })?;

    let client = build_client(request_timeout_secs(cfg), None)
        .map_err(|e| format!("failed to build HTTP client for backfill: {e}"))?;
    let mut manifest = open_append_manifest(&manifest_path).await.map_err(|e| {
        format!(
            "failed to open manifest for backfill at {}: {e}",
            manifest_path.display()
        )
    })?;

    let mut stats = BackfillStats {
        candidates: candidates.len(),
        ..BackfillStats::default()
    };
    let mut added_urls = Vec::new();

    let backfill_concurrency = cfg
        .backfill_concurrency_limit
        .unwrap_or(cfg.batch_concurrency)
        .clamp(1, 512);

    // Compute the selector config once — it does not change between URLs.
    let shared_selector_config = build_selector_config(cfg);
    for chunk in candidates.chunks(backfill_concurrency) {
        let mut joins = tokio::task::JoinSet::new();
        for url in chunk.iter().cloned() {
            let http = client.clone();
            let retries = cfg.fetch_retries;
            let backoff = cfg.retry_backoff_ms;
            let min_chars = cfg.min_markdown_chars;
            let drop_thin = cfg.drop_thin_markdown;
            let selector_config = shared_selector_config.clone();
            joins.spawn(fetch_and_convert_backfill_url(
                http,
                url,
                retries,
                backoff,
                min_chars,
                drop_thin,
                selector_config,
            ));
        }

        while let Some(joined) = joins.join_next().await {
            let Ok((url, result)) = joined else {
                stats.failed += 1;
                continue;
            };
            let Some((trimmed, markdown_chars, is_thin, dropped)) = result else {
                stats.failed += 1;
                continue;
            };
            stats.fetched_ok += 1;
            summary.pages_seen += 1;
            if is_thin {
                summary.thin_pages += 1;
            }
            if dropped {
                continue;
            }

            write_backfill_entry(&mut manifest, &markdown_dir, &url, &trimmed, markdown_chars)
                .await?;
            summary.markdown_files += 1;
            stats.written += 1;
            added_urls.push(url);
        }
    }
    manifest.flush().await?;
    Ok((stats, added_urls))
}

/// Discover sitemap URLs, fetch new ones, convert to markdown, and append
/// to the manifest. Updates `summary.markdown_files` and `summary.thin_pages`.
///
/// This is the engine-level backfill that replaces the CLI's
/// `append_robots_backfill`. It reuses `discover_sitemap_urls` for discovery
/// and `fetch_text_with_retry` for fetching.
pub async fn append_sitemap_backfill(
    cfg: &Config,
    start_url: &str,
    output_dir: &Path,
    seen_urls: &HashSet<String>,
    summary: &mut CrawlSummary,
) -> Result<BackfillStats, Box<dyn Error>> {
    let discovery = discover_sitemap_urls(cfg, start_url).await?;
    if discovery.urls.is_empty() {
        return Ok(BackfillStats {
            discovered_urls: discovery.discovered_urls,
            ..BackfillStats::default()
        });
    }
    let (mut stats, _) =
        append_candidate_backfill(cfg, output_dir, seen_urls, discovery.urls.clone(), summary)
            .await?;
    stats.discovered_urls = discovery.discovered_urls;
    log_info(&format!(
        "sitemap backfill_complete urls_added={}",
        stats.written
    ));
    Ok(stats)
}
