use std::collections::HashSet;
use std::error::Error;
use std::path::Path;
use std::time::Instant;

use spider::url::Url;

use crate::crates::core::config::{Config, MapFallback, RenderMode};
use crate::crates::core::content::extract_anchor_hrefs;
use crate::crates::core::http::{fetch_html, http_client, normalize_url};
use crate::crates::core::logging::log_info;

use super::super::sitemap::{SitemapDiscovery, discover_sitemap_urls};
use super::super::url_utils::{MapScope, normalize_map_candidate_url};
use super::super::{CrawlSummary, append_candidate_backfill, is_excluded_url_path};
use super::{
    MapResult, derive_map_scope, derive_map_scope_url, is_excluded_map_url,
    merge_map_candidate_urls, resolve_map_seed_url,
};

fn effective_fallback_limit(cfg: &Config) -> usize {
    if cfg.max_pages == 0 {
        500
    } else {
        cfg.max_pages as usize
    }
}

/// Run a full Spider.rs crawl and collect map-format results.
/// Only called when `--map-fallback crawl` is set.
pub async fn crawl_and_collect_map(
    cfg: &Config,
    start_url: &str,
    mode: RenderMode,
    scope: &MapScope,
) -> Result<(CrawlSummary, Vec<String>), Box<dyn Error>> {
    use super::super::runtime::configure_website;
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
    urls.sort();
    Ok((summary, urls))
}

async fn crawl_with_auto_switch(
    cfg: &Config,
    crawl_start_url: &str,
    scope: &MapScope,
) -> Result<(CrawlSummary, Vec<String>), Box<dyn Error>> {
    let (summary, urls) =
        crawl_and_collect_map(cfg, crawl_start_url, RenderMode::Http, scope).await?;
    let coverage_floor = cfg.auto_switch_min_pages.max(1);
    let chrome_available = cfg
        .chrome_remote_url
        .as_ref()
        .is_some_and(|s| !s.is_empty());
    if urls.len() >= coverage_floor || !chrome_available {
        return Ok((summary, urls));
    }
    log_info(&format!(
        "map: HTTP crawl returned {} URLs (< {} threshold), retrying with Chrome",
        urls.len(),
        coverage_floor
    ));
    match crawl_and_collect_map(cfg, crawl_start_url, RenderMode::Chrome, scope).await {
        Ok((chrome_summary, chrome_urls)) if chrome_urls.len() > urls.len() => {
            Ok((chrome_summary, chrome_urls))
        }
        Ok(_) => Ok((summary, urls)),
        Err(err) => {
            log_info(&format!(
                "map: Chrome retry failed ({err}); keeping HTTP result"
            ));
            Ok((summary, urls))
        }
    }
}

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
    let mut urls: Vec<String> = urls
        .into_iter()
        .filter(|url| !is_excluded_url_path(url, &cfg.exclude_path_prefix))
        .collect();
    urls.sort();

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
pub async fn map_with_sitemap(cfg: &Config, start_url: &str) -> Result<MapResult, Box<dyn Error>> {
    let start = Instant::now();

    let (seed_result, sitemap_result) = tokio::join!(
        async {
            resolve_map_seed_url(start_url)
                .await
                .map_err(|e| e.to_string())
        },
        async {
            if cfg.discover_sitemaps {
                discover_sitemap_urls(cfg, start_url)
                    .await
                    .map_err(|e| e.to_string())
            } else {
                Ok(SitemapDiscovery::default())
            }
        }
    );

    let crawl_start_url = seed_result.unwrap_or_else(|_| normalize_url(start_url).into_owned());

    let scope_base = {
        let start_host = Url::parse(&normalize_url(start_url).into_owned())
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

    match cfg.map_fallback {
        MapFallback::Crawl => {
            let (mut summary, urls) = match cfg.render_mode {
                RenderMode::AutoSwitch => {
                    Box::pin(crawl_with_auto_switch(cfg, &crawl_start_url, &scope)).await?
                }
                m => Box::pin(crawl_and_collect_map(cfg, &crawl_start_url, m, &scope)).await?,
            };
            summary.elapsed_ms = start.elapsed().as_millis();
            Ok(MapResult {
                summary,
                sitemap_urls: raw_sitemap_count,
                urls,
                map_source: "crawl".to_string(),
                warning: None,
            })
        }
        MapFallback::Structure => {
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
pub async fn append_html_anchor_backfill(
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
