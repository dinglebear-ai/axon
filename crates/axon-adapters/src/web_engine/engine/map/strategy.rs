use std::error::Error;
use std::time::Instant;

use url::Url;

use axon_core::config::Config;
use axon_core::content::extract_anchor_hrefs;
use axon_core::http::{fetch_html, http_client, normalize_url};
use axon_core::logging::{log_info, log_warn};

use super::super::sitemap::{SitemapDiscovery, discover_sitemap_urls, sitemap_url_limit};
use super::super::url_utils::MapScope;
use super::super::{CrawlSummary, is_excluded_url_path};
use super::{
    MapResult, derive_map_scope, derive_map_scope_url, is_excluded_map_url,
    merge_map_candidate_urls, resolve_map_seed_url,
};

fn effective_root_anchor_limit(cfg: &Config) -> usize {
    if cfg.max_pages == 0 {
        500
    } else {
        cfg.max_pages as usize
    }
}

async fn discover_root_anchors(
    cfg: &Config,
    scope_start_url: &str,
    scope: &MapScope,
) -> (Vec<String>, Option<String>) {
    let root_anchor_limit = effective_root_anchor_limit(cfg);
    let client = match http_client() {
        Ok(c) => c,
        Err(e) => {
            log_info(&format!("bounded-structure: http_client failed: {e}"));
            return (
                vec![],
                Some("bounded-structure discovery: http client unavailable".to_string()),
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
                    "bounded-structure discovery failed to fetch {scope_start_url}; \
                     dynamic navigation may not be discoverable"
                )),
            );
        }
    };

    let anchor_urls = extract_anchor_hrefs(scope_start_url, &html, root_anchor_limit);
    let urls = merge_map_candidate_urls(Vec::new(), anchor_urls, scope, true);
    let mut urls: Vec<String> = urls
        .into_iter()
        .filter(|url| !is_excluded_url_path(url, &cfg.exclude_path_prefix))
        .collect();
    urls.sort();

    let warning = if urls.len() < 5 {
        Some(format!(
            "bounded-structure discovery returned {} URL(s); \
             dynamic navigation may not be discoverable",
            urls.len()
        ))
    } else {
        None
    };

    (urls, warning)
}

/// Merge `candidates` into the map scope, then drop scope/locale-excluded URLs.
fn scope_and_filter_map_urls(
    cfg: &Config,
    candidates: Vec<String>,
    scope: &MapScope,
) -> Vec<String> {
    let url_limit = sitemap_url_limit(cfg);
    let urls = merge_map_candidate_urls(Vec::new(), candidates, scope, true);
    let scope_prefix_len = scope.path_prefix.as_deref().map_or(0, str::len);
    urls.into_iter()
        .filter(|url| !is_excluded_map_url(url, &cfg.exclude_path_prefix, scope_prefix_len))
        .take(url_limit)
        .collect()
}

/// Build a `MapResult` for a discovery-sourced map (sitemap / sitemap+llms / llms).
fn build_discovery_map_result(
    urls: Vec<String>,
    raw_sitemap_count: usize,
    map_source: &str,
    elapsed_ms: u128,
) -> MapResult {
    MapResult {
        summary: CrawlSummary {
            elapsed_ms,
            ..Default::default()
        },
        sitemap_urls: raw_sitemap_count,
        urls,
        map_source: map_source.to_string(),
        warning: None,
    }
}

/// Discover canonical in-scope URLs without crawling or writing page content.
pub async fn discover_site_urls(
    cfg: &Config,
    start_url: &str,
) -> Result<MapResult, Box<dyn Error>> {
    let start = Instant::now();

    let (seed_result, sitemap_result, llms_urls) = tokio::join!(
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
        },
        async {
            if cfg.discover_llms_txt {
                // warn-and-continue: never fail the map call on llms.txt errors.
                match crate::web_engine::engine::discover_llms_txt_urls(cfg, start_url).await {
                    Ok(urls) => urls,
                    Err(e) => {
                        log_warn(&format!(
                            "command=llms_txt map discovery failed url={start_url}: {e}"
                        ));
                        Vec::new()
                    }
                }
            } else {
                Vec::new()
            }
        }
    );

    let resolved_start_url = seed_result.unwrap_or_else(|_| normalize_url(start_url).into_owned());

    let scope_base = {
        let start_host = Url::parse(&normalize_url(start_url))
            .ok()
            .and_then(|u| u.host_str().map(str::to_ascii_lowercase));
        let resolved_host = Url::parse(&resolved_start_url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_ascii_lowercase));
        if start_host != resolved_host {
            normalize_url(start_url).into_owned()
        } else {
            resolved_start_url.clone()
        }
    };

    let scope = derive_map_scope(start_url, &scope_base).ok_or("failed to derive map scope")?;
    let scope_start_url =
        derive_map_scope_url(start_url, &scope_base).unwrap_or_else(|| resolved_start_url.clone());

    let sitemap_discovery: SitemapDiscovery = match sitemap_result {
        Ok(d) => d,
        Err(e) => {
            log_warn(&format!(
                "command=sitemap map discovery failed url={start_url}: {e}"
            ));
            SitemapDiscovery::default()
        }
    };
    if sitemap_discovery.failed_fetches > 0 {
        log_warn(&format!(
            "command=sitemap map discovery failed_fetches={} discovered_urls={} url={start_url}",
            sitemap_discovery.failed_fetches, sitemap_discovery.discovered_urls
        ));
    }
    let raw_sitemap_count = sitemap_discovery.discovered_urls;

    log_info(&format!(
        "map sitemap_docs={} sitemap_urls={} url={}",
        sitemap_discovery.parsed_sitemap_documents, raw_sitemap_count, start_url
    ));

    if sitemap_discovery.parsed_sitemap_documents > 0 {
        let mut combined = sitemap_discovery.urls;
        combined.extend(llms_urls.iter().cloned());
        let urls = scope_and_filter_map_urls(cfg, combined, &scope);
        let map_source = if llms_urls.is_empty() {
            "sitemap"
        } else {
            "sitemap+llms"
        };
        return Ok(build_discovery_map_result(
            urls,
            raw_sitemap_count,
            map_source,
            start.elapsed().as_millis(),
        ));
    }

    // No sitemap, but a curated llms.txt: use it before root-anchor discovery.
    if !llms_urls.is_empty() {
        let urls = scope_and_filter_map_urls(cfg, llms_urls, &scope);
        if !urls.is_empty() {
            return Ok(build_discovery_map_result(
                urls,
                raw_sitemap_count,
                "llms",
                start.elapsed().as_millis(),
            ));
        }
    }

    let (urls, warning) = discover_root_anchors(cfg, &scope_start_url, &scope).await;
    Ok(MapResult {
        summary: CrawlSummary {
            elapsed_ms: start.elapsed().as_millis(),
            ..Default::default()
        },
        sitemap_urls: raw_sitemap_count,
        urls,
        map_source: "bounded-structure".to_string(),
        warning,
    })
}
