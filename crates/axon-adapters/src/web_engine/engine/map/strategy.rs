use std::error::Error;
use std::sync::Arc;
use std::time::Instant;

use url::Url;

use axon_core::config::Config;
use axon_core::content::extract_anchor_hrefs;
use axon_core::http::{FetchWebOptions, fetch_web, normalize_url};
use axon_core::logging::{log_info, log_warn};

use super::super::sitemap::{
    DISCOVERY_MAX_BODY_BYTES, SitemapDiscovery, discover_sitemap_urls, fetch_text,
    sitemap_url_limit,
};
use super::super::url_utils::MapScope;
use super::super::{CrawlSummary, is_excluded_url_path};
use super::{
    MapResult, derive_map_scope, derive_map_scope_url, is_excluded_map_url,
    merge_map_candidate_urls, resolve_map_seed_url,
};
use crate::boundary::FetchProvider;

/// URL count at or above which sitemap/llms discovery is considered sufficient
/// on its own and anchor discovery is skipped.
///
/// Mirrors webclaw's `MapOptions::min_sitemap_urls` default. Below this the
/// layers are merged rather than the sitemap result being returned alone — a
/// site with a rich sitemap skips the extra fetch entirely, while a thin or
/// out-of-scope one still gets anchors.
const MIN_DISCOVERY_URLS: usize = 200;

/// Merged-result count below which the map is reported as possibly incomplete.
const MIN_HEALTHY_MAP_URLS: usize = 5;

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
    fetch: Arc<dyn FetchProvider>,
) -> (Vec<String>, Option<String>) {
    let root_anchor_limit = effective_root_anchor_limit(cfg);
    let html = if let Some(html) = fetch_text(
        fetch.as_ref(),
        scope_start_url,
        Some(DISCOVERY_MAX_BODY_BYTES),
    )
    .await
    {
        html
    } else {
        // Preserve the provider boundary for deterministic tests and normal
        // acquisition, then use the unified anti-bot ladder as the production
        // fallback for sites that reject the plain provider request.
        match fetch_web(
            scope_start_url,
            &FetchWebOptions::html().with_scan_bytes(cfg.antibot_max_body_scan_bytes),
        )
        .await
        {
            Ok(doc) => {
                if doc.escalated {
                    log_info(&format!(
                        "bounded-structure: {scope_start_url} required browser TLS impersonation"
                    ));
                }
                doc.body
            }
            Err(error) => {
                log_info(&format!(
                    "bounded-structure: fetch failed for {scope_start_url}: {error}"
                ));
                return (
                    vec![],
                    Some(format!(
                        "bounded-structure discovery failed to fetch {scope_start_url}: {error}"
                    )),
                );
            }
        }
    };

    let anchor_urls = extract_anchor_hrefs(scope_start_url, &html, root_anchor_limit);
    let urls = merge_map_candidate_urls(Vec::new(), anchor_urls, scope, true);
    let mut urls: Vec<String> = urls
        .into_iter()
        .filter(|url| !is_excluded_url_path(url, &cfg.exclude_path_prefix))
        .collect();
    urls.sort();

    // Thinness is judged by the caller on the MERGED result — anchors may be
    // sparse while the sitemap layer supplied plenty. Only hard failures (no
    // client, no fetch) are reported from here.
    (urls, None)
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
/// Outcome of the three discovery probes run concurrently before scoping.
struct DiscoveryProbes {
    resolved_start_url: String,
    sitemap: SitemapDiscovery,
    llms_urls: Vec<String>,
}

/// Resolve the seed URL, discover sitemaps, and read `llms.txt` concurrently.
///
/// Each probe degrades independently: a failure warns and yields an empty
/// result rather than failing the whole map.
async fn run_discovery_probes(
    cfg: &Config,
    start_url: &str,
    fetch: Arc<dyn FetchProvider>,
) -> DiscoveryProbes {
    let (seed_result, sitemap_result, llms_urls) = tokio::join!(
        async {
            resolve_map_seed_url(start_url, fetch.clone())
                .await
                .map_err(|e| e.to_string())
        },
        async {
            if cfg.discover_sitemaps {
                discover_sitemap_urls(cfg, start_url, fetch.clone())
                    .await
                    .map_err(|e| e.to_string())
            } else {
                Ok(SitemapDiscovery::default())
            }
        },
        async {
            if cfg.discover_llms_txt {
                // warn-and-continue: never fail the map call on llms.txt errors.
                match crate::web_engine::engine::discover_llms_txt_urls(
                    cfg,
                    start_url,
                    fetch.clone(),
                )
                .await
                {
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

    let sitemap: SitemapDiscovery = match sitemap_result {
        Ok(d) => d,
        Err(e) => {
            log_warn(&format!(
                "command=sitemap map discovery failed url={start_url}: {e}"
            ));
            SitemapDiscovery::default()
        }
    };
    if sitemap.failed_fetches > 0 {
        log_warn(&format!(
            "command=sitemap map discovery failed_fetches={} discovered_urls={} url={start_url}",
            sitemap.failed_fetches, sitemap.discovered_urls
        ));
    }

    DiscoveryProbes {
        resolved_start_url,
        sitemap,
        llms_urls,
    }
}

/// Discover canonical in-scope URLs without crawling or writing page content.
pub async fn discover_site_urls(
    cfg: &Config,
    start_url: &str,
    fetch: Arc<dyn FetchProvider>,
) -> Result<MapResult, Box<dyn Error>> {
    let start = Instant::now();

    let DiscoveryProbes {
        resolved_start_url,
        sitemap: sitemap_discovery,
        llms_urls,
    } = run_discovery_probes(cfg, start_url, fetch.clone()).await;

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

    let raw_sitemap_count = sitemap_discovery.discovered_urls;
    log_info(&format!(
        "map sitemap_docs={} sitemap_urls={} url={}",
        sitemap_discovery.parsed_sitemap_documents, raw_sitemap_count, start_url
    ));

    let discovery_source = match (
        sitemap_discovery.parsed_sitemap_documents > 0,
        llms_urls.is_empty(),
    ) {
        (true, true) => Some("sitemap"),
        (true, false) => Some("sitemap+llms"),
        (false, false) => Some("llms"),
        (false, true) => None,
    };

    let mut combined = sitemap_discovery.urls;
    combined.extend(llms_urls);
    let discovery_urls = scope_and_filter_map_urls(cfg, combined, &scope);

    // A sitemap that parsed successfully but yielded few in-scope URLs is not a
    // usable map — a stale sitemap, a thin one listing only section roots, or one
    // whose entries all fell outside scope. Gate on the RESULTING URL count, not
    // on how many documents parsed, so those cases still reach anchor discovery.
    if let Some(source) = discovery_source
        && discovery_urls.len() >= MIN_DISCOVERY_URLS
    {
        return Ok(build_discovery_map_result(
            discovery_urls,
            raw_sitemap_count,
            source,
            start.elapsed().as_millis(),
        ));
    }

    let (anchor_urls, fetch_warning) =
        discover_root_anchors(cfg, &scope_start_url, &scope, fetch).await;
    let anchors_found = !anchor_urls.is_empty();

    // Layers are additive: discovery entries keep priority, anchors fill in the
    // rest, deduplicated through the same normalization the map already uses.
    let urls = merge_map_candidate_urls(discovery_urls, anchor_urls, &scope, true);

    let map_source = match (discovery_source, anchors_found) {
        (Some(source), true) => format!("{source}+bounded-structure"),
        (Some(source), false) => source.to_string(),
        (None, _) => "bounded-structure".to_string(),
    };

    let warning = fetch_warning.or_else(|| {
        (urls.len() < MIN_HEALTHY_MAP_URLS).then(|| {
            format!(
                "map discovery returned {} URL(s); dynamic navigation may not be discoverable",
                urls.len()
            )
        })
    });

    Ok(MapResult {
        summary: CrawlSummary {
            elapsed_ms: start.elapsed().as_millis(),
            ..Default::default()
        },
        sitemap_urls: raw_sitemap_count,
        urls,
        map_source,
        warning,
    })
}
