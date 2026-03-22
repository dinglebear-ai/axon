mod cdp_render;
mod collector;
mod dir_ops;
mod runtime;
pub(crate) mod sitemap;
#[cfg(test)]
mod tests;
mod thin_refetch;
mod url_utils;

use crate::crates::core::config::{Config, RenderMode};
use crate::crates::core::content::{
    build_selector_config, build_transform_config, extract_anchor_hrefs,
};
use crate::crates::core::http::{fetch_html, http_client, normalize_url, validate_url};
use crate::crates::core::logging::{log_done, log_info};
use crate::crates::crawl::manifest::ManifestEntry;
use collector::{CollectorConfig, collect_crawl_pages};
use dir_ops::prepare_crawl_output_dir;
pub use dir_ops::update_latest_reflink;
use runtime::configure_website;
use spider::url::Url;
#[cfg(test)]
use spider::website::Website;
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc::Sender;

pub(crate) use runtime::resolve_cdp_ws_url;
pub(crate) use sitemap::append_candidate_backfill;
pub use sitemap::{BackfillStats, append_sitemap_backfill};
pub(crate) use sitemap::{SitemapDiscovery, discover_sitemap_urls};
pub(crate) use thin_refetch::chrome_refetch_thin_pages;
use url_utils::normalize_map_candidate_url;
pub(crate) use url_utils::{MapScope, canonicalize_url_for_dedupe, is_excluded_url_path};
#[cfg(test)]
pub(crate) use url_utils::{is_junk_discovered_url, regex_escape};

#[derive(Debug, Default, Clone)]
pub struct CrawlSummary {
    pub pages_seen: u32,
    pub markdown_files: u32,
    pub thin_pages: u32,
    pub reused_pages: u32,
    pub pages_discovered: u32,
    pub elapsed_ms: u128,
    /// Canonical URLs of pages that were below `min_markdown_chars`.
    /// Populated by the collector and used by the auto-switch path to
    /// perform targeted per-URL Chrome re-fetches instead of a full re-crawl.
    pub thin_urls: HashSet<String>,
    /// Pages skipped due to non-2xx HTTP status codes.
    pub error_pages: u32,
    /// Pages blocked by a WAF or anti-bot system (`waf_check || blocked_crawl`).
    pub waf_blocked_pages: u32,
    /// Canonical URLs of WAF-blocked pages; used for targeted stealth Chrome retry.
    pub waf_blocked_urls: HashSet<String>,
}

pub fn should_fallback_to_chrome(summary: &CrawlSummary, max_pages: u32, cfg: &Config) -> bool {
    if summary.markdown_files == 0 {
        return true;
    }
    // A very-low-page crawl does not provide enough HTTP-only signal to judge
    // whether the captured content is complete, so give AutoSwitch one Chrome
    // retry even if the page is not technically "thin".
    if summary.pages_seen <= 2 {
        return true;
    }
    let thin_ratio = if summary.pages_seen == 0 {
        1.0
    } else {
        summary.thin_pages as f64 / summary.pages_seen as f64
    };
    if thin_ratio > cfg.auto_switch_thin_ratio {
        return true;
    }
    // When max_pages == 0 (uncapped), there's no expected page count to compare
    // against, so "low coverage" is meaningless — skip that check entirely.
    if max_pages == 0 {
        return false;
    }
    summary.markdown_files < (max_pages / 10).max(cfg.auto_switch_min_pages as u32)
}

fn should_retry_map_with_chrome(summary: &CrawlSummary) -> bool {
    summary.pages_seen <= 2
}

fn should_retry_map_with_html_fallback(url_count: usize) -> bool {
    url_count <= 2
}

/// Fallback page limit when `max_pages` is uncapped (0).
fn effective_fallback_limit(cfg: &Config) -> usize {
    if cfg.max_pages == 0 {
        500
    } else {
        cfg.max_pages as usize
    }
}

fn merge_map_candidate_urls(
    existing: Vec<String>,
    candidates: Vec<String>,
    scope: &MapScope,
    drop_query: bool,
) -> Vec<String> {
    let mut merged = Vec::new();
    let mut seen = HashSet::new();

    for url in existing {
        let Some(canonical) = canonicalize_url_for_dedupe(&url) else {
            continue;
        };
        if seen.insert(canonical.clone()) {
            merged.push(canonical);
        }
    }

    for url in candidates {
        let Some(canonical) = normalize_map_candidate_url(&url, scope, drop_query) else {
            continue;
        };
        if seen.insert(canonical.clone()) {
            merged.push(canonical);
        }
    }

    merged
}

pub(crate) async fn append_html_anchor_backfill(
    cfg: &Config,
    start_url: &str,
    output_dir: &Path,
    seen_urls: &HashSet<String>,
    summary: &mut CrawlSummary,
) -> Result<Vec<String>, Box<dyn Error>> {
    let crawl_start_url = resolve_map_seed_url(start_url)
        .await
        .unwrap_or_else(|_| normalize_url(start_url).into_owned());
    let scope =
        derive_map_scope(start_url, &crawl_start_url).ok_or("failed to derive map scope")?;
    let fallback_limit = effective_fallback_limit(cfg);

    let client = http_client()
        .map_err(|e| format!("http client init failed for backfill of {start_url}: {e}"))?;
    let html = fetch_html(client, &crawl_start_url)
        .await
        .map_err(|e| format!("fetch failed for backfill of {crawl_start_url}: {e}"))?;
    let fallback_urls = extract_anchor_hrefs(&crawl_start_url, &html, fallback_limit);

    // Skip the intermediate Vec<String> clone of seen_urls. Instead, pass an
    // empty existing list and rely on the filter below to deduplicate against
    // seen_urls. merge_map_candidate_urls handles internal dedup of fallback_urls.
    let merged_urls = merge_map_candidate_urls(Vec::new(), fallback_urls, &scope, true);
    let candidates: Vec<String> = merged_urls
        .into_iter()
        .filter(|url| {
            !seen_urls.contains(url) && !is_excluded_url_path(url, &cfg.exclude_path_prefix)
        })
        .collect();

    let (_, added_urls) =
        append_candidate_backfill(cfg, output_dir, seen_urls, candidates, summary).await?;
    Ok(added_urls)
}

fn derive_map_scope_url(requested_url: &str, resolved_url: &str) -> Option<String> {
    let requested_canonical = canonicalize_url_for_dedupe(requested_url)?;
    let requested = Url::parse(&requested_canonical).ok()?;
    let resolved_canonical = canonicalize_url_for_dedupe(resolved_url)
        .or_else(|| canonicalize_url_for_dedupe(requested_url))?;
    let mut resolved = Url::parse(&resolved_canonical).ok()?;

    let requested_path = requested.path().trim_end_matches('/').to_string();
    let resolved_path = resolved.path().trim_end_matches('/').to_string();
    let scope_path = if !requested_path.is_empty()
        && requested.host_str()? != resolved.host_str()?
        && resolved_path.is_empty()
    {
        requested_path
    } else {
        resolved_path
    };

    resolved.set_path(if scope_path.is_empty() {
        "/"
    } else {
        &scope_path
    });
    canonicalize_url_for_dedupe(resolved.as_ref())
}

fn derive_map_scope(requested_url: &str, resolved_url: &str) -> Option<MapScope> {
    let scope_url = derive_map_scope_url(requested_url, resolved_url)?;
    let parsed = Url::parse(&scope_url).ok()?;
    let path = parsed.path().trim_end_matches('/');

    Some(MapScope {
        host: parsed.host_str()?.to_string(),
        path_prefix: if path.is_empty() {
            None
        } else {
            Some(path.to_string())
        },
    })
}

async fn resolve_map_seed_url(start_url: &str) -> Result<String, Box<dyn Error>> {
    let normalized = normalize_url(start_url);
    validate_url(&normalized).map_err(|e| format!("invalid map seed URL {normalized}: {e}"))?;
    let client =
        http_client().map_err(|e| format!("http client init for map seed {normalized}: {e}"))?;

    if let Ok(response) = client.head(normalized.as_ref()).send().await
        && response.status().is_success()
    {
        let final_url = response.url().to_string();
        // Re-validate the final URL after redirects to prevent a public URL
        // from redirecting to an internal/private address.
        validate_url(&final_url)
            .map_err(|e| format!("map seed redirect target blocked: {final_url}: {e}"))?;
        return Ok(final_url);
    }

    let response = client
        .get(normalized.as_ref())
        .send()
        .await
        .map_err(|e| format!("GET failed resolving map seed {normalized}: {e}"))?
        .error_for_status()
        .map_err(|e| format!("non-success status resolving map seed {normalized}: {e}"))?;
    let final_url = response.url().to_string();
    validate_url(&final_url)
        .map_err(|e| format!("map seed redirect target blocked: {final_url}: {e}"))?;
    Ok(final_url)
}

pub(crate) async fn crawl_and_collect_map(
    cfg: &Config,
    start_url: &str,
    mode: RenderMode,
    scope: &MapScope,
) -> Result<(CrawlSummary, Vec<String>), Box<dyn Error>> {
    let mut website = configure_website(cfg, start_url, mode)
        .await
        .map_err(|e| format!("failed to configure website for map of {start_url}: {e}"))?;
    let start = Instant::now();

    match mode {
        RenderMode::Http => website.crawl_raw().await,
        RenderMode::Chrome | RenderMode::AutoSwitch => website.crawl().await,
    }

    let mut summary = CrawlSummary::default();
    let mut urls = Vec::new();
    let mut seen = HashSet::new();
    let exclude_path_prefix = cfg.exclude_path_prefix.clone();

    for link in website.get_links() {
        let page_url = link.as_ref().to_string();
        if is_excluded_url_path(&page_url, &exclude_path_prefix) {
            continue;
        }
        let Some(canonical_url) = normalize_map_candidate_url(&page_url, scope, true) else {
            continue;
        };
        if !seen.insert(canonical_url.clone()) {
            continue;
        }
        summary.pages_seen += 1;
        urls.push(canonical_url);
    }

    summary.elapsed_ms = start.elapsed().as_millis();
    Ok((summary, urls))
}

/// The unified result of a `map` operation: crawler-discovered URLs merged with
/// sitemap-discovered URLs, deduplicated and sorted.
#[derive(Debug, Default)]
pub struct MapResult {
    pub summary: CrawlSummary,
    /// All discovered URLs (crawler + sitemap), sorted and deduplicated.
    pub urls: Vec<String>,
    /// Raw number of URLs returned by `discover_sitemap_urls` before any
    /// deduplication against crawler-discovered URLs.  This is the count of
    /// `<loc>` entries in the sitemap(s), not the count of net-new URLs that
    /// were absent from the crawler results.
    pub sitemap_urls: usize,
}

/// Discover all URLs reachable from `start_url` and merge with sitemap
/// discovery, returning a single deduplicated sorted list.
///
/// Handles the AutoSwitch fallback to Chrome when HTTP finds zero pages.
/// Sitemap merge/sort/dedup happens here — callers receive a final unified set.
pub async fn map_with_sitemap(cfg: &Config, start_url: &str) -> Result<MapResult, Box<dyn Error>> {
    let initial_mode = match cfg.render_mode {
        RenderMode::AutoSwitch => RenderMode::Http,
        m => m,
    };
    let crawl_start_url = resolve_map_seed_url(start_url)
        .await
        .unwrap_or_else(|_| normalize_url(start_url).into_owned());
    let scope =
        derive_map_scope(start_url, &crawl_start_url).ok_or("failed to derive map scope")?;
    let scope_start_url = derive_map_scope_url(start_url, &crawl_start_url)
        .unwrap_or_else(|| crawl_start_url.clone());

    let (mut summary, mut urls) =
        crawl_and_collect_map(cfg, &crawl_start_url, initial_mode, &scope).await?;

    if matches!(cfg.render_mode, RenderMode::AutoSwitch)
        && should_retry_map_with_chrome(&summary)
        && let Ok((chrome_summary, chrome_urls)) =
            crawl_and_collect_map(cfg, &crawl_start_url, RenderMode::Chrome, &scope).await
        && chrome_summary.pages_seen > summary.pages_seen
    {
        summary = chrome_summary;
        urls = chrome_urls;
    }

    if should_retry_map_with_html_fallback(urls.len()) {
        let fallback_limit = effective_fallback_limit(cfg);
        if let Ok(client) = http_client()
            && let Ok(html) = fetch_html(client, &crawl_start_url).await
        {
            let fallback_urls = extract_anchor_hrefs(&crawl_start_url, &html, fallback_limit);
            urls = merge_map_candidate_urls(urls, fallback_urls, &scope, true);
            summary.pages_seen = urls.len() as u32;
        }
    }

    let raw_sitemap_count = if cfg.discover_sitemaps {
        let sitemap_url_list = discover_sitemap_urls(cfg, &scope_start_url).await?.urls;
        let count = sitemap_url_list.len();
        urls = merge_map_candidate_urls(urls, sitemap_url_list, &scope, true);
        summary.pages_seen = urls.len() as u32;
        count
    } else {
        0
    };

    Ok(MapResult {
        summary,
        urls,
        sitemap_urls: raw_sitemap_count,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "crawl orchestration requires many config/state params"
)]
pub async fn run_crawl_once(
    cfg: &Config,
    start_url: &str,
    mode: RenderMode,
    output_dir: &Path,
    progress_tx: Option<Sender<CrawlSummary>>,
    run_sitemap: bool,
    previous_manifest: Arc<HashMap<String, ManifestEntry>>,
    crawl_id: Option<&str>,
) -> Result<(CrawlSummary, HashSet<String>), Box<dyn Error>> {
    log_info(&format!(
        "crawl start url={} render_mode={:?} max_pages={} max_depth={}",
        start_url, mode, cfg.max_pages, cfg.max_depth
    ));
    let total_start = Instant::now();

    let markdown_dir = output_dir.join("markdown");
    let recycling_bin = output_dir.join("markdown.old");

    prepare_crawl_output_dir(output_dir, &markdown_dir, &recycling_bin, cfg)
        .await
        .map_err(|e| {
            format!(
                "failed to prepare output dir {} for crawl of {start_url}: {e}",
                output_dir.display()
            )
        })?;

    let mut website = runtime::configure_website_with_crawl_id(cfg, start_url, mode, crawl_id)
        .await
        .map_err(|e| format!("failed to configure crawl website for {start_url}: {e}"))?;
    // Buffer at least max_pages worth of messages to prevent silent page drops
    // under high-throughput crawls (extreme/max profiles). Clamp to 16 384 so
    // a large --max-pages value can't allocate an unbounded broadcast ring buffer.
    let subscribe_buf = (cfg.max_pages as usize).clamp(4096, 16_384);
    let rx = website.subscribe(subscribe_buf).ok_or_else(|| {
        format!("failed to subscribe to spider broadcast channel for {start_url}")
    })?;
    let markdown_dir = output_dir.join("markdown");
    let manifest_path = output_dir.join("manifest.jsonl");

    let min_chars = cfg.min_markdown_chars;
    let drop_thin = cfg.drop_thin_markdown;
    let exclude_path_prefix = cfg.exclude_path_prefix.clone();
    let crawl_start = Instant::now();
    let transform_cfg = build_transform_config();

    // Enable inline Chrome re-rendering when the *config* requests AutoSwitch,
    // even though `mode` is `Http` for the initial crawl phase (AutoSwitch
    // always starts with HTTP — `resolve_initial_mode` converts AutoSwitch→Http).
    // Chrome mode does its own rendering; Http mode with no AutoSwitch intent
    // has no Chrome target. When cfg.render_mode is AutoSwitch and Chrome is
    // configured, thin pages are re-rendered immediately while the HTTP crawl
    // continues — no second pass needed.
    let inline_chrome_ws_url = if matches!(cfg.render_mode, RenderMode::AutoSwitch) {
        cfg.chrome_remote_url.clone()
    } else {
        None
    };

    let join = tokio::spawn(collect_crawl_pages(
        rx,
        CollectorConfig {
            markdown_dir,
            manifest_path,
            min_chars,
            drop_thin,
            exclude_path_prefix,
            scope: None,
            transform_cfg,
            progress_tx,
            previous_manifest: Arc::clone(&previous_manifest),
            selector_config: build_selector_config(cfg),
            chrome_ws_url: inline_chrome_ws_url,
            chrome_timeout_secs: cfg.chrome_network_idle_timeout_secs,
            output_dir: output_dir.to_path_buf(),
        },
    ));

    // Spider-native sitemap phase: pages flow through the live subscription above.
    // persist_links() carries accumulated sitemap links into the subsequent main crawl.
    if run_sitemap && cfg.discover_sitemaps {
        website.crawl_sitemap().await;
        website.persist_links();
    }

    match mode {
        RenderMode::Http => website.crawl_raw().await,
        RenderMode::Chrome | RenderMode::AutoSwitch => website.crawl().await,
    }
    website.unsubscribe();

    let (mut summary, urls) = join
        .await
        .map_err(|e| format!("collector join failure for {start_url}: {e}"))?
        .map_err(|e| format!("collector failure for {start_url}: {e}"))?;
    summary.elapsed_ms = crawl_start.elapsed().as_millis();

    if dir_ops::path_exists(&recycling_bin).await {
        tokio::fs::remove_dir_all(&recycling_bin)
            .await
            .map_err(|e| {
                format!(
                    "failed to remove recycling bin {}: {e}",
                    recycling_bin.display()
                )
            })?;
        log_info("Purged recycling bin — armory is now synchronized with battlefield.");
    }

    log_done(&format!(
        "crawl done url={} pages_fetched={} duration_ms={}",
        start_url,
        summary.pages_seen,
        total_start.elapsed().as_millis()
    ));
    Ok((summary, urls))
}

/// Crawl only the sitemap — no follow-on main crawl.
/// Pages flow through the same subscription pipeline as `run_crawl_once`.
pub async fn run_sitemap_only(
    cfg: &Config,
    start_url: &str,
    output_dir: &Path,
    previous_manifest: Arc<HashMap<String, ManifestEntry>>,
) -> Result<(CrawlSummary, HashSet<String>), Box<dyn Error>> {
    tokio::fs::create_dir_all(output_dir.join("markdown"))
        .await
        .map_err(|e| {
            format!("failed to create markdown dir for sitemap crawl of {start_url}: {e}")
        })?;

    let mut website = configure_website(cfg, start_url, cfg.render_mode)
        .await
        .map_err(|e| {
            format!("failed to configure website for sitemap crawl of {start_url}: {e}")
        })?;
    // Override the default set by configure_website: sitemap IS the crawl here.
    website.with_ignore_sitemap(false);

    let subscribe_buf = (cfg.max_pages as usize).clamp(4096, 16_384);
    let rx = website.subscribe(subscribe_buf).ok_or_else(|| {
        format!("failed to subscribe to spider broadcast for sitemap crawl of {start_url}")
    })?;
    let manifest_path = output_dir.join("manifest.jsonl");
    let markdown_dir = output_dir.join("markdown");
    let transform_cfg = build_transform_config();
    let crawl_start = Instant::now();

    let join = tokio::spawn(collect_crawl_pages(
        rx,
        CollectorConfig {
            markdown_dir,
            manifest_path,
            min_chars: cfg.min_markdown_chars,
            drop_thin: cfg.drop_thin_markdown,
            exclude_path_prefix: cfg.exclude_path_prefix.clone(),
            scope: None,
            transform_cfg,
            progress_tx: None,
            previous_manifest: Arc::clone(&previous_manifest),
            selector_config: build_selector_config(cfg),
            // Sitemap-only crawl: no inline Chrome rendering (HTTP-only path).
            chrome_ws_url: None,
            chrome_timeout_secs: cfg.chrome_network_idle_timeout_secs,
            output_dir: output_dir.to_path_buf(),
        },
    ));

    website.crawl_sitemap().await;
    website.unsubscribe();

    let (mut summary, urls) = join
        .await
        .map_err(|e| format!("sitemap collector join failure for {start_url}: {e}"))?
        .map_err(|e| format!("sitemap collector failure for {start_url}: {e}"))?;
    summary.elapsed_ms = crawl_start.elapsed().as_millis();

    Ok((summary, urls))
}
