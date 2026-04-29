use super::sitemap::{SitemapDiscovery, discover_sitemap_urls};
use super::url_utils::{MapScope, canonicalize_url_for_dedupe, normalize_map_candidate_url};
use super::{CrawlSummary, append_candidate_backfill, is_excluded_url_path};
use crate::crates::core::config::{Config, MapFallback, RenderMode};
use crate::crates::core::content::extract_anchor_hrefs;
use crate::crates::core::http::{fetch_html, http_client, normalize_url, validate_url};
use crate::crates::core::logging::log_info;
use spider::url::Url;
use std::collections::HashSet;
use std::error::Error;
use std::path::Path;
use std::time::Instant;

/// Check URL against exclusions, also applying them relative to the effective scope root.
///
/// Standard `is_excluded_url_path` only matches from the domain root, so `/de` would
/// not exclude `/docs/de/overview`. This function checks ONE additional level:
///
/// - When `scope_prefix_len > 0` (e.g. scope = `/docs`): checks the sub-path after the
///   scope prefix. `/docs/de/overview` → `/de/overview` → excluded by `/de` ✓
///   `/docs/en/settings` → `/en/settings` → not excluded by `/settings` ✓ (no false positive)
///
/// - When `scope_prefix_len == 0` (root scope, e.g. cross-host redirect): detects the
///   first segment boundary in the path and checks the sub-path from there.
///   `/docs/de/overview` → `/de/overview` → excluded ✓
///   `/docs/en/settings` → `/en/settings` → not excluded ✓
///
/// Lowercases the path before comparison so `/zh-CN` is correctly caught by `/zh-cn`.
fn is_excluded_map_url(url: &str, excludes: &[String], scope_prefix_len: usize) -> bool {
    if is_excluded_url_path(url, excludes) {
        return true;
    }
    if excludes.is_empty() {
        return false;
    }
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    let path_lc = parsed.path().to_ascii_lowercase();
    let path = path_lc.as_str();

    let check_from = if scope_prefix_len > 0 {
        scope_prefix_len
    } else {
        // Root scope: find the first segment boundary (skip one directory level)
        match path[1..].find('/') {
            Some(n) => 1 + n,
            None => return false, // single-segment path, nothing to check below it
        }
    };

    let rel = match path.get(check_from..) {
        Some(r) if !r.is_empty() => r,
        _ => return false,
    };
    is_excluded_url_path(&format!("https://x{rel}"), excludes)
}

/// The unified result of a `map` operation: URLs discovered via sitemaps, bounded
/// structure extraction, or full crawl (when `--map-fallback crawl` is set).
#[derive(Debug, Default)]
pub struct MapResult {
    pub summary: CrawlSummary,
    /// All discovered URLs, sorted and deduplicated.
    pub urls: Vec<String>,
    /// Raw number of `<loc>` entries found in sitemaps before dedup.
    pub sitemap_urls: usize,
    /// How URLs were discovered: "sitemap", "bounded-structure", or "crawl".
    pub map_source: String,
    /// Optional warning, e.g. when bounded-structure fallback returns very few URLs.
    pub warning: Option<String>,
}

fn effective_fallback_limit(cfg: &Config) -> usize {
    if cfg.max_pages == 0 {
        500
    } else {
        cfg.max_pages as usize
    }
}

pub(crate) fn merge_map_candidate_urls(
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

/// Resolve the final URL after redirects, used as the crawl/scope seed.
pub(crate) async fn resolve_map_seed_url(start_url: &str) -> Result<String, Box<dyn Error>> {
    let normalized = normalize_url(start_url);
    validate_url(&normalized).map_err(|e| format!("invalid map seed URL {normalized}: {e}"))?;
    let client =
        http_client().map_err(|e| format!("http client init for map seed {normalized}: {e}"))?;

    if let Ok(response) = client.head(normalized.as_ref()).send().await
        && response.status().is_success()
    {
        let final_url = response.url().to_string();
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

pub(crate) fn derive_map_scope(requested_url: &str, resolved_url: &str) -> Option<MapScope> {
    let scope_url = derive_map_scope_url(requested_url, resolved_url)?;
    let parsed = Url::parse(&scope_url).ok()?;
    let path = parsed.path().trim_end_matches('/');

    // Single-segment paths (e.g. /home, /about) are top-level pages, not directory
    // prefixes — don't scope to them. Mirrors derive_auto_whitelist_pattern's rule.
    let segment_count = path.split('/').filter(|s| !s.is_empty()).count();

    Some(MapScope {
        host: parsed.host_str()?.to_string(),
        path_prefix: if path.is_empty() || segment_count <= 1 {
            None
        } else {
            Some(path.to_string())
        },
    })
}

/// Run a full Spider.rs crawl and collect map-format results.
/// Only called when `--map-fallback crawl` is set.
pub(super) async fn crawl_and_collect_map(
    cfg: &Config,
    start_url: &str,
    mode: RenderMode,
    scope: &MapScope,
) -> Result<(CrawlSummary, Vec<String>), Box<dyn Error>> {
    use super::runtime::configure_website;
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

/// Fetch root HTML and extract anchor hrefs as a bounded fallback.
/// Returns (urls, warning) — warning is non-None when fewer than 5 URLs found.
async fn bounded_structure_fallback(
    cfg: &Config,
    scope_start_url: &str,
    scope: &MapScope,
) -> (Vec<String>, Option<String>) {
    let fallback_limit = effective_fallback_limit(cfg);
    let client = match http_client() {
        Ok(c) => c,
        Err(e) => {
            log_info(&format!("bounded-structure: http_client failed: {e}"));
            return (
                vec![],
                Some("bounded-structure fallback: http client unavailable".to_string()),
            );
        }
    };
    let html = match fetch_html(client, scope_start_url).await {
        Ok(h) => h,
        Err(e) => {
            log_info(&format!(
                "bounded-structure: fetch failed for {scope_start_url}: {e}"
            ));
            return (
                vec![],
                Some(format!(
                    "bounded-structure fallback failed to fetch {scope_start_url}; \
                     consider --map-fallback crawl for SPA sites"
                )),
            );
        }
    };

    let anchor_urls = extract_anchor_hrefs(scope_start_url, &html, fallback_limit);
    let urls = merge_map_candidate_urls(Vec::new(), anchor_urls, scope, true);
    let urls: Vec<String> = urls
        .into_iter()
        .filter(|url| !is_excluded_url_path(url, &cfg.exclude_path_prefix))
        .collect();

    let warning = if urls.len() < 5 {
        Some(format!(
            "bounded-structure fallback returned {} URL(s); \
             consider --map-fallback crawl for SPA sites",
            urls.len()
        ))
    } else {
        None
    };

    (urls, warning)
}

/// Discover all URLs reachable from `start_url` using a sitemap-first strategy.
///
/// Flow:
/// 1. Resolve seed URL + discover sitemaps in parallel.
/// 2. If sitemaps were parsed (`parsed_sitemap_documents > 0`): use sitemap URLs, done.
/// 3. If no sitemaps parsed AND `cfg.map_fallback == Crawl`: run full crawl.
/// 4. If no sitemaps parsed AND `cfg.map_fallback == Structure` (default):
///    fetch root HTML and extract anchor hrefs (bounded, fast).
///
/// NOTE: sitemap discovery runs against the original `start_url` host (before redirect),
/// in parallel with seed URL resolution. Out-of-scope URLs from redirected hosts are
/// filtered by `normalize_map_candidate_url` once the scope is derived.
pub async fn map_with_sitemap(cfg: &Config, start_url: &str) -> Result<MapResult, Box<dyn Error>> {
    let start = Instant::now();

    // Step 1: resolve seed URL and discover sitemaps in parallel.
    // Errors are converted to String before the join point so both futures are Send.
    let (seed_result, sitemap_result) = tokio::join!(
        async {
            resolve_map_seed_url(start_url)
                .await
                .map_err(|e| e.to_string())
        },
        async {
            discover_sitemap_urls(cfg, start_url)
                .await
                .map_err(|e| e.to_string())
        }
    );

    let crawl_start_url = seed_result.unwrap_or_else(|_| normalize_url(start_url).into_owned());

    // When the seed URL resolves to a different host (cross-host redirect), anchor the
    // map scope to the original start URL. Sitemap discovery already ran against
    // start_url, so its URLs carry start_url's host — using the redirect target host
    // as the scope would filter everything out.
    // The crawl fallback still uses crawl_start_url so --map-fallback crawl works.
    let scope_base = {
        let start_host = Url::parse(&normalize_url(start_url))
            .ok()
            .and_then(|u| u.host_str().map(str::to_ascii_lowercase));
        let resolved_host = Url::parse(&crawl_start_url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_ascii_lowercase));
        if start_host != resolved_host {
            normalize_url(start_url).into_owned()
        } else {
            crawl_start_url.clone()
        }
    };

    let scope = derive_map_scope(start_url, &scope_base).ok_or("failed to derive map scope")?;
    let scope_start_url =
        derive_map_scope_url(start_url, &scope_base).unwrap_or_else(|| crawl_start_url.clone());

    let sitemap_discovery: SitemapDiscovery = sitemap_result.unwrap_or_default();
    let raw_sitemap_count = sitemap_discovery.discovered_urls;

    log_info(&format!(
        "map sitemap_docs={} sitemap_urls={} url={}",
        sitemap_discovery.parsed_sitemap_documents, raw_sitemap_count, start_url
    ));

    // Step 2: if sitemaps were found and parsed, use them directly — no fallback.
    if sitemap_discovery.parsed_sitemap_documents > 0 {
        let urls = merge_map_candidate_urls(Vec::new(), sitemap_discovery.urls, &scope, true);
        let scope_prefix_len = scope.path_prefix.as_deref().map_or(0, str::len);
        let urls: Vec<String> = urls
            .into_iter()
            .filter(|url| !is_excluded_map_url(url, &cfg.exclude_path_prefix, scope_prefix_len))
            .collect();
        let summary = CrawlSummary {
            elapsed_ms: start.elapsed().as_millis(),
            ..Default::default()
        };
        return Ok(MapResult {
            summary,
            sitemap_urls: raw_sitemap_count,
            urls,
            map_source: "sitemap".to_string(),
            warning: None,
        });
    }

    // Step 3: no sitemap documents parsed — check fallback mode.
    match cfg.map_fallback {
        MapFallback::Crawl => {
            // Explicit opt-in: run full Spider.rs crawl (legacy behaviour).
            let initial_mode = match cfg.render_mode {
                RenderMode::AutoSwitch => RenderMode::Http,
                m => m,
            };
            let (summary, urls) =
                crawl_and_collect_map(cfg, &crawl_start_url, initial_mode, &scope).await?;
            Ok(MapResult {
                summary,
                sitemap_urls: raw_sitemap_count,
                urls,
                map_source: "crawl".to_string(),
                warning: None,
            })
        }
        MapFallback::Structure => {
            // Default: bounded anchor extraction from homepage (fast, no full crawl).
            let (urls, warning) = bounded_structure_fallback(cfg, &scope_start_url, &scope).await;
            let summary = CrawlSummary {
                elapsed_ms: start.elapsed().as_millis(),
                ..Default::default()
            };
            Ok(MapResult {
                summary,
                sitemap_urls: raw_sitemap_count,
                urls,
                map_source: "bounded-structure".to_string(),
                warning,
            })
        }
    }
}

/// HTML anchor backfill: used by sync-crawl as a post-crawl supplement.
/// NOT part of the map command's primary flow.
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
