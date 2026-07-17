//! Sitemap discovery: robots.txt parsing, seed paths, and batched concurrent
//! fetching of sitemap documents (following `<sitemapindex>` references).

use super::filter::{lastmod_is_recent, loc_in_scope};
use super::{
    DISCOVERY_MAX_BODY_BYTES, SITEMAP_MAX_BODY_BYTES, fetch_text_with_retry, join_origin_path,
    request_timeout_secs,
};
use axon_core::config::Config;
use axon_core::content::{extract_loc_values, extract_loc_with_lastmod, extract_robots_sitemaps};
use axon_core::http::build_client;
use axon_core::logging::log_info;
use spider::url::Url;
use std::collections::{HashSet, VecDeque};
use std::error::Error;

use crate::web_engine::engine::MAX_TRACKED_DISCOVERED_URLS;

/// Result of sitemap discovery including URLs and diagnostic stats.
#[derive(Debug, Clone, Default)]
pub struct SitemapDiscovery {
    /// Sorted, deduplicated URLs discovered from sitemaps.
    pub urls: Vec<String>,
    /// Number of sitemaps declared in robots.txt.
    pub robots_declared_sitemaps: usize,
    /// Number of default seed sitemaps added to the queue.
    pub seeded_default_sitemaps: usize,
    /// Total sitemap documents successfully parsed.
    pub parsed_sitemap_documents: usize,
    /// Total page URLs discovered (before dedup).
    pub discovered_urls: usize,
    /// Fetches that failed or returned non-success.
    pub failed_fetches: usize,
}

struct SitemapScope<'a> {
    start_host: &'a str,
    start_path: &'a str,
    scoped_to_root: bool,
}

struct SitemapBatchOutput<'a> {
    seen_sitemaps: &'a HashSet<String>,
    queue: &'a mut VecDeque<String>,
    out: &'a mut HashSet<String>,
    sitemap_fetch_limit: usize,
    url_limit: usize,
    failed_fetches: usize,
}

pub(crate) fn sitemap_url_limit(cfg: &Config) -> usize {
    // Zero means caller-uncapped, not server-unbounded.
    if cfg.max_pages == 0 {
        MAX_TRACKED_DISCOVERED_URLS
    } else {
        (cfg.max_pages as usize).min(MAX_TRACKED_DISCOVERED_URLS)
    }
}

pub(crate) fn sitemap_fetch_limit(cfg: &Config) -> usize {
    // Keep malicious indexes from turning max_sitemaps=0 into unlimited I/O.
    if cfg.max_sitemaps == 0 {
        MAX_TRACKED_DISCOVERED_URLS
    } else {
        cfg.max_sitemaps.min(MAX_TRACKED_DISCOVERED_URLS)
    }
}

pub(crate) fn insert_discovered_url(
    out: &mut HashSet<String>,
    url: String,
    url_limit: usize,
) -> bool {
    if out.len() >= url_limit {
        return false;
    }
    out.insert(url);
    out.len() < url_limit
}

/// Seed the queue with default sitemap paths. Uses `join_origin_path` so IPv6 authorities
/// are bracketed correctly (and non-standard ports preserved).
fn sitemap_seed_queue(parsed: &Url) -> VecDeque<String> {
    [
        "/sitemap.xml",
        "/sitemap_index.xml",
        "/sitemap-index.xml",
        "/wp-sitemap.xml",
        "/sitemap/sitemap-index.xml",
    ]
    .into_iter()
    .filter_map(|path| join_origin_path(parsed, path).ok())
    .collect()
}

async fn process_sitemap_batch(
    cfg: &Config,
    client: &reqwest::Client,
    batch: Vec<String>,
    scope: &SitemapScope<'_>,
    output: &mut SitemapBatchOutput<'_>,
) -> usize {
    // In test builds, propagate the thread-local SSRF loopback bypass flag
    // into spawned tasks so httpmock servers on 127.0.0.1 are reachable.
    #[cfg(test)]
    let loopback_flag = axon_core::http::get_allow_loopback();

    let mut joins = tokio::task::JoinSet::new();
    for sitemap_url in batch {
        let http = client.clone();
        let retries = cfg.fetch_retries;
        let backoff = cfg.retry_backoff_ms;
        joins.spawn(async move {
            #[cfg(test)]
            axon_core::http::set_allow_loopback(loopback_flag);

            fetch_text_with_retry(
                &http,
                &sitemap_url,
                retries,
                backoff,
                Some(SITEMAP_MAX_BODY_BYTES),
            )
            .await
            .map(|xml| (sitemap_url, xml))
        });
    }

    let mut parsed = 0usize;
    while let Some(joined) = joins.join_next().await {
        let Ok(Some((_sitemap_url, xml))) = joined else {
            output.failed_fetches += 1;
            continue;
        };
        parsed += 1;
        // The <sitemapindex> root element always appears in the XML prolog
        // (first ~200 bytes). Scanning the full multi-MB body is wasteful.
        let head = xml.as_bytes().get(..512).unwrap_or(xml.as_bytes());
        let is_index = head
            .windows(b"<sitemapindex".len())
            .any(|w| w.eq_ignore_ascii_case(b"<sitemapindex"));
        let since_days = cfg.sitemap_since_days;
        if !is_index && since_days > 0 {
            // Date-filtered path: use block-level parsing to get <lastmod> per URL.
            for (loc, lastmod) in extract_loc_with_lastmod(&xml) {
                if let Some(ref lm) = lastmod
                    && !lastmod_is_recent(lm, since_days)
                {
                    continue;
                }
                if let Some(canonical_loc) = loc_in_scope(
                    cfg,
                    &loc,
                    scope.start_host,
                    scope.start_path,
                    scope.scoped_to_root,
                ) {
                    if !insert_discovered_url(output.out, canonical_loc, output.url_limit) {
                        break;
                    }
                }
            }
        } else {
            for loc in extract_loc_values(&xml) {
                if let Some(canonical_loc) = loc_in_scope(
                    cfg,
                    &loc,
                    scope.start_host,
                    scope.start_path,
                    scope.scoped_to_root,
                ) {
                    if is_index
                        && !output.seen_sitemaps.contains(&canonical_loc)
                        && output.seen_sitemaps.len() + output.queue.len()
                            < output.sitemap_fetch_limit
                    {
                        output.queue.push_back(canonical_loc);
                    } else if !is_index {
                        if !insert_discovered_url(output.out, canonical_loc, output.url_limit) {
                            break;
                        }
                    }
                }
            }
        }
    }
    parsed
}

/// Fetch `robots.txt` for `scheme://host/robots.txt` and enqueue any `Sitemap:` directives
/// into `queue`. Returns the count of declared sitemaps found.
async fn enqueue_robots_sitemaps(
    client: &reqwest::Client,
    parsed: &Url,
    cfg: &Config,
    queue: &mut VecDeque<String>,
    sitemap_fetch_limit: usize,
) -> usize {
    let Ok(robots_url) = join_origin_path(parsed, "/robots.txt") else {
        return 0;
    };
    let Some(robots_txt) = fetch_text_with_retry(
        client,
        &robots_url,
        cfg.fetch_retries,
        cfg.retry_backoff_ms,
        Some(DISCOVERY_MAX_BODY_BYTES),
    )
    .await
    else {
        return 0;
    };
    let declared = extract_robots_sitemaps(&robots_txt);
    let count = declared.len();
    queue.extend(
        declared
            .into_iter()
            .take(sitemap_fetch_limit.saturating_sub(queue.len())),
    );
    count
}

/// Discover sitemap URLs with robots.txt parsing and batched concurrent fetching.
///
/// Seeds the queue with default sitemap paths plus any sitemaps declared in
/// robots.txt. Processes sitemap documents in concurrent batches using JoinSet,
/// following sitemap index references recursively.
///
/// Returns a [`SitemapDiscovery`] with sorted, deduplicated URLs and diagnostic stats.
pub async fn discover_sitemap_urls(
    cfg: &Config,
    start_url: &str,
) -> Result<SitemapDiscovery, Box<dyn Error>> {
    let parsed = Url::parse(start_url)
        .map_err(|e| format!("invalid start URL for sitemap discovery {start_url}: {e}"))?;
    // Scheme/host/port (incl. correct IPv6 bracketing and non-standard ports) are derived
    // by joining paths onto `parsed` directly, so no manual authority string is built here.
    let bare_host = parsed
        .host_str()
        .ok_or_else(|| format!("missing host in sitemap start URL {start_url}"))?
        .to_string();

    let mut queue = sitemap_seed_queue(&parsed);
    let sitemap_fetch_limit = sitemap_fetch_limit(cfg);
    queue.truncate(sitemap_fetch_limit);
    let seeded_default_sitemaps = queue.len();

    let client = build_client(request_timeout_secs(cfg), None).map_err(|e| {
        format!("failed to build HTTP client for sitemap discovery of {start_url}: {e}")
    })?;
    let robots_declared_sitemaps =
        enqueue_robots_sitemaps(&client, &parsed, cfg, &mut queue, sitemap_fetch_limit).await;

    let mut seen_sitemaps = HashSet::new();
    let mut out = HashSet::new();
    let start_path = parsed.path().trim_end_matches('/').to_string();
    // Single-segment paths (e.g. /home, /about) are top-level pages, not directory
    // roots. Scoping sitemap discovery to /home would filter out all peer URLs.
    // Mirrors derive_auto_whitelist_pattern's rule for crawl scoping.
    let segment_count = start_path.split('/').filter(|s| !s.is_empty()).count();
    let scoped_to_root = start_path.is_empty() || segment_count <= 1;
    // Use bare hostname for scope checks — host_str() on discovered URLs
    // never includes port, so scope comparison must use bare host too.
    let scope = SitemapScope {
        start_host: &bare_host,
        start_path: &start_path,
        scoped_to_root,
    };
    let worker_limit = cfg
        .backfill_concurrency_limit
        .unwrap_or(cfg.batch_concurrency)
        .clamp(1, 1024);
    let url_limit = sitemap_url_limit(cfg);
    let mut sitemap_fetches = 0usize;
    let mut parsed_sitemaps = 0usize;
    let mut failed_fetches = 0usize;

    while !queue.is_empty() && sitemap_fetches < sitemap_fetch_limit && out.len() < url_limit {
        let mut batch = Vec::new();
        while batch.len() < worker_limit && sitemap_fetches + batch.len() < sitemap_fetch_limit {
            let Some(url) = queue.pop_front() else {
                break;
            };
            if seen_sitemaps.insert(url.clone()) {
                batch.push(url);
            }
        }
        if batch.is_empty() {
            break;
        }
        // Count attempts before I/O so failed sitemap requests consume budget too.
        sitemap_fetches += batch.len();

        let mut output = SitemapBatchOutput {
            seen_sitemaps: &seen_sitemaps,
            queue: &mut queue,
            out: &mut out,
            sitemap_fetch_limit,
            url_limit,
            failed_fetches: 0,
        };
        parsed_sitemaps += process_sitemap_batch(cfg, &client, batch, &scope, &mut output).await;
        failed_fetches += output.failed_fetches;
        if parsed_sitemaps.is_multiple_of(64) {
            log_info(&format!(
                "command=sitemap parsed={} discovered_urls={} queue={}",
                parsed_sitemaps,
                out.len(),
                queue.len()
            ));
        }
    }

    let mut urls: Vec<String> = out.into_iter().collect();
    urls.sort();
    let discovered_urls = urls.len();
    Ok(SitemapDiscovery {
        urls,
        robots_declared_sitemaps,
        seeded_default_sitemaps,
        parsed_sitemap_documents: parsed_sitemaps,
        discovered_urls,
        failed_fetches,
    })
}
