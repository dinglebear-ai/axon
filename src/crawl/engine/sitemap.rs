use super::{CrawlSummary, canonicalize_url_for_dedupe, is_excluded_url_path};
use crate::core::config::Config;
use crate::core::content::{
    build_selector_config, extract_loc_values, extract_loc_with_lastmod, extract_robots_sitemaps,
    to_markdown, url_to_stable_filename,
};
use crate::core::http::{build_client, validate_url};
use crate::core::logging::{log_info, log_warn};
use crate::crawl::manifest::ManifestEntry;
use sha2::{Digest, Sha256};
use spider::url::Url;
use std::collections::{HashSet, VecDeque};
use std::error::Error;
use std::path::Path;
use std::time::Duration;
use tokio::io::{AsyncWriteExt, BufWriter};

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

/// Default body cap for the `/llms.txt` discovery document (and small docs like robots.txt).
/// Guards the discovery path — NOT general HTML/sitemap fetches — against OOM from a
/// malicious/misconfigured host. 512 KB comfortably exceeds a real llms.txt link index.
pub(crate) const DISCOVERY_MAX_BODY_BYTES: u64 = 512 * 1024;

/// Body cap for `sitemap.xml`. The sitemap protocol ceiling is 50 MB uncompressed, so the
/// cap must be generous enough not to drop large-but-valid sitemaps.
pub(crate) const SITEMAP_MAX_BODY_BYTES: u64 = 50 * 1024 * 1024;

/// Join `path` onto the origin of `parsed`, producing a correctly-formatted absolute URL.
///
/// `Url::join` with a leading-slash path replaces the path while preserving scheme, host,
/// and port — and crucially brackets IPv6 literals in the authority (`[::1]:8080`), which
/// `format!("{host}:{port}")` does NOT (`host_str()` returns the address without brackets,
/// yielding an invalid authority for IPv6 hosts).
pub(crate) fn join_origin_path(parsed: &Url, path: &str) -> Result<String, Box<dyn Error>> {
    // Strip any userinfo (`user:pass@`) so credentials never propagate into discovery
    // requests or logs — join only the origin (scheme://host:port) with `path`. The
    // setters only fail on cannot-be-a-base URLs, which http(s) origins never are.
    let mut origin = parsed.clone();
    let _ = origin.set_username("");
    let _ = origin.set_password(None);
    Ok(origin.join(path)?.to_string())
}

fn should_retry_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

pub(crate) fn request_timeout_secs(cfg: &Config) -> u64 {
    cfg.request_timeout_ms
        .unwrap_or(30_000)
        .div_ceil(1000)
        .max(1)
}

/// Read a successful response body, optionally capped at `max_bytes`.
///
/// - `Some(cap)` → streamed read with a hard byte cap and lossy UTF-8 decode (fine for
///   llms.txt/sitemap, which are UTF-8 by spec; lossy is strictly safer than dropping the
///   whole doc on a stray byte). Oversized bodies return `None`.
/// - `None` → charset-aware, lossy, uncapped `resp.text()` — matches `main`'s behavior for
///   HTML page backfill and any caller that must not silently drop large or non-UTF8 bodies.
async fn read_body_capped(
    resp: reqwest::Response,
    url: &str,
    max_bytes: Option<u64>,
) -> Option<String> {
    let Some(cap) = max_bytes else {
        return match resp.text().await {
            Ok(text) => Some(text),
            Err(e) => {
                log_warn(&format!("command=fetch body read failed url={url}: {e}"));
                None
            }
        };
    };
    if resp.content_length().is_some_and(|len| len > cap) {
        log_warn(&format!(
            "command=fetch oversized body rejected (content-length) cap_bytes={cap} url={url}"
        ));
        return None;
    }
    let mut collected: Vec<u8> = Vec::new();
    let mut stream = resp;
    loop {
        match stream.chunk().await {
            Ok(Some(chunk)) => {
                if collected.len() as u64 + chunk.len() as u64 > cap {
                    log_warn(&format!(
                        "command=fetch oversized body rejected (mid-stream) cap_bytes={cap} url={url}"
                    ));
                    return None;
                }
                collected.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(e) => {
                log_warn(&format!("command=fetch stream error url={url}: {e}"));
                return None;
            }
        }
    }
    // Lossy decode: replace malformed bytes rather than dropping the entire document
    // (a regression vs reqwest::Response::text(), which decodes charset-aware/lossily).
    Some(String::from_utf8_lossy(&collected).into_owned())
}

pub(crate) async fn fetch_text_with_retry(
    client: &reqwest::Client,
    url: &str,
    retries: usize,
    backoff_ms: u64,
    max_bytes: Option<u64>,
) -> Option<String> {
    if validate_url(url).is_err() {
        return None;
    }
    let mut attempt = 0usize;
    loop {
        match client.get(url).send().await {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    return read_body_capped(resp, url, max_bytes).await;
                }
                if !should_retry_status(status) {
                    // True 404/non-retryable absence stays low-noise (no warn).
                    return None;
                }
                if attempt >= retries {
                    // Retryable status (429/5xx) that never recovered — worth a warn.
                    log_warn(&format!(
                        "command=fetch status={} retries exhausted url={url}",
                        status.as_u16()
                    ));
                    return None;
                }
            }
            Err(_) if attempt >= retries => {
                log_warn(&format!(
                    "command=fetch transport error, retries exhausted url={url}"
                ));
                return None;
            }
            Err(_) => {}
        }

        attempt = attempt.saturating_add(1);
        let exp = attempt.saturating_sub(1).min(20) as u32;
        let multiplier = 1u64 << exp;
        let delay_ms = backoff_ms.saturating_mul(multiplier).max(1);
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }
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

/// Returns `true` if `lastmod` (ISO 8601 date or datetime string) falls within the last
/// `since_days` days. Unknown / unparseable dates are treated as recent (not filtered out).
fn lastmod_is_recent(lastmod: &str, since_days: u32) -> bool {
    use chrono::{NaiveDate, Utc};
    let cutoff = Utc::now().date_naive() - chrono::Duration::days(i64::from(since_days));
    // Accept both "YYYY-MM-DD" and "YYYY-MM-DDTHH:MM:SSZ" by taking the first 10 chars.
    let prefix = lastmod.get(..10).unwrap_or(lastmod);
    match NaiveDate::parse_from_str(prefix, "%Y-%m-%d") {
        Ok(date) => date >= cutoff,
        Err(_) => true, // unparseable → include (don't silently drop)
    }
}

/// Returns the canonicalized URL if `loc` is in scope for a crawl/discovery rooted at
/// `start_host`/`start_path`, else `None`. Shared by sitemap and llms.txt discovery.
/// Same-host by default; honors `cfg.include_subdomains` and `cfg.exclude_path_prefix`.
pub(crate) fn loc_in_scope(
    cfg: &Config,
    loc: &str,
    start_host: &str,
    start_path: &str,
    scoped_to_root: bool,
) -> Option<String> {
    let u = Url::parse(loc).ok()?;
    let h = u.host_str()?;
    let in_scope = if cfg.include_subdomains {
        h == start_host
            || h.strip_suffix(start_host)
                .is_some_and(|rest| rest.ends_with('.'))
    } else {
        h == start_host
    };
    if !in_scope || is_excluded_url_path(loc, &cfg.exclude_path_prefix) {
        return None;
    }
    if !scoped_to_root {
        let p = u.path();
        let exact = p == start_path;
        // Avoid allocating a temporary String for the nested check.
        let nested = p.starts_with(start_path) && p.as_bytes().get(start_path.len()) == Some(&b'/');
        if !exact && !nested {
            return None;
        }
    }
    canonicalize_url_for_dedupe(loc)
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
    let loopback_flag = crate::core::http::get_allow_loopback();

    let mut joins = tokio::task::JoinSet::new();
    for sitemap_url in batch {
        let http = client.clone();
        let retries = cfg.fetch_retries;
        let backoff = cfg.retry_backoff_ms;
        joins.spawn(async move {
            #[cfg(test)]
            crate::core::http::set_allow_loopback(loopback_flag);

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
                    output.out.insert(canonical_loc);
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
                    if is_index && !output.seen_sitemaps.contains(&canonical_loc) {
                        output.queue.push_back(canonical_loc);
                    } else if !is_index {
                        output.out.insert(canonical_loc);
                    }
                }
            }
        }
    }
    parsed
}

/// Discover sitemap URLs with robots.txt parsing and batched concurrent fetching.
///
/// Seeds the queue with 3 default sitemap paths plus any sitemaps declared in
/// robots.txt. Processes sitemap documents in concurrent batches using JoinSet,
/// following sitemap index references recursively.
///
/// Returns a [`SitemapDiscovery`] with sorted, deduplicated URLs and diagnostic stats.
/// Fetch `robots.txt` for `scheme://host/robots.txt` and enqueue any `Sitemap:` directives
/// into `queue`. Returns the count of declared sitemaps found.
async fn enqueue_robots_sitemaps(
    client: &reqwest::Client,
    parsed: &Url,
    cfg: &Config,
    queue: &mut VecDeque<String>,
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
    queue.extend(declared);
    count
}

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
    let seeded_default_sitemaps = queue.len();

    let client = build_client(request_timeout_secs(cfg), None).map_err(|e| {
        format!("failed to build HTTP client for sitemap discovery of {start_url}: {e}")
    })?;
    let robots_declared_sitemaps = enqueue_robots_sitemaps(&client, &parsed, cfg, &mut queue).await;

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
    // Treat 0 as unlimited (consistent with --max-pages and --sitemap-since-days).
    let max_sitemaps = if cfg.max_sitemaps == 0 {
        usize::MAX
    } else {
        cfg.max_sitemaps
    };
    let mut parsed_sitemaps = 0usize;
    let mut failed_fetches = 0usize;

    while !queue.is_empty() && parsed_sitemaps < max_sitemaps {
        let mut batch = Vec::new();
        while batch.len() < worker_limit && parsed_sitemaps + batch.len() < max_sitemaps {
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

        let mut output = SitemapBatchOutput {
            seen_sitemaps: &seen_sitemaps,
            queue: &mut queue,
            out: &mut out,
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

struct SitemapScope<'a> {
    start_host: &'a str,
    start_path: &'a str,
    scoped_to_root: bool,
}

struct SitemapBatchOutput<'a> {
    seen_sitemaps: &'a HashSet<String>,
    queue: &'a mut VecDeque<String>,
    out: &'a mut HashSet<String>,
    failed_fetches: usize,
}

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

/// Raw markdown/text targets (e.g. llms.txt-listed `.md` docs) must skip the HTML→markdown
/// transform — `to_markdown(main_content:true)` would strip them to nothing and drop them as thin.
pub(crate) fn is_already_markdown(url: &str) -> bool {
    // Compare only the path, ignoring query/fragment.
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown") || lower.ends_with(".txt")
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

#[cfg(test)]
#[path = "sitemap_tests.rs"]
mod tests;
