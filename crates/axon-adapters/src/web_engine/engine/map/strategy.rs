use std::collections::HashSet;
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use url::Url;

use axon_api::source::{MetadataMap, RenderMode as ProviderRenderMode, RenderRequest};
use axon_core::config::{Config, RenderMode as CrawlRenderMode};
use axon_core::content::extract_anchor_hrefs;
use axon_core::http::normalize_url;
use axon_core::logging::{log_info, log_warn};

use super::super::sitemap::{
    DISCOVERY_MAX_BODY_BYTES, SitemapDiscovery, discover_sitemap_urls_with_metadata,
    fetch_text_with_metadata, sitemap_url_limit,
};
use super::super::url_utils::MapScope;
use super::super::{CrawlSummary, is_excluded_url_path};
use super::{
    MapDiscoveryOutcome, MapResult, derive_map_scope, derive_map_scope_url, is_excluded_map_url,
    merge_discovery_and_anchor_urls, merge_discovery_candidate_urls, merge_map_candidate_urls,
    resolve_map_seed_url_with_metadata,
};
use crate::boundary::{FetchProvider, RenderProvider};

/// URL count at or above which sitemap/llms discovery is considered sufficient
/// on its own and anchor discovery is skipped.
///
/// Mirrors webclaw's `MapOptions::min_sitemap_urls` default. Below this the
/// layers are merged rather than the sitemap result being returned alone — a
/// site with a rich sitemap skips the extra fetch entirely, while a thin or
/// out-of-scope one still gets anchors.
const MIN_DISCOVERY_URLS: usize = 200;
const MIN_CORROBORATED_SITEMAP_URLS: usize = 100;
const LLMS_SITEMAP_COVERAGE_PERCENT: usize = 95;

pub(crate) fn discovery_is_sufficient(
    source: &str,
    discovery_url_count: usize,
    sitemap_urls: &[String],
    llms_urls: &[String],
) -> bool {
    if discovery_url_count >= MIN_DISCOVERY_URLS {
        return true;
    }
    if source != "sitemap+llms" || sitemap_urls.len() < MIN_CORROBORATED_SITEMAP_URLS {
        return false;
    }

    let markdown_alternates = llms_urls
        .iter()
        .filter_map(|url| {
            url.strip_suffix(".md")
                .or_else(|| url.strip_suffix(".markdown"))
        })
        .collect::<HashSet<_>>();
    let covered = sitemap_urls
        .iter()
        .filter(|url| markdown_alternates.contains(url.as_str()))
        .count();
    covered.saturating_mul(100)
        >= sitemap_urls
            .len()
            .saturating_mul(LLMS_SITEMAP_COVERAGE_PERCENT)
}

/// Merged-result count below which the map is reported as possibly incomplete.
const MIN_HEALTHY_MAP_URLS: usize = 5;
const MAP_ROOT_RENDER_TIMEOUT_MS: u64 = 8_000;
const MAP_ROOT_NETWORK_IDLE_SECS: u64 = 1;

fn effective_root_anchor_limit(cfg: &Config) -> usize {
    if cfg.max_pages == 0 {
        500
    } else {
        cfg.max_pages as usize
    }
}

async fn render_root_anchor_candidates(
    cfg: &Config,
    scope_start_url: &str,
    root_anchor_limit: usize,
    render: Arc<dyn RenderProvider>,
    mode: ProviderRenderMode,
    execution_metadata: &MetadataMap,
) -> Result<Vec<String>, String> {
    let mut metadata = execution_metadata.clone();
    metadata.insert("normalize".to_string(), serde_json::json!(cfg.normalize));
    metadata.insert("block_assets".to_string(), serde_json::json!(true));
    metadata.insert("exact_browser_timeout".to_string(), serde_json::json!(true));
    metadata.insert(
        "chrome_network_idle_timeout_secs".to_string(),
        serde_json::json!(MAP_ROOT_NETWORK_IDLE_SECS),
    );
    let render_timeout_ms = cfg
        .request_timeout_ms
        .unwrap_or(MAP_ROOT_RENDER_TIMEOUT_MS)
        .min(MAP_ROOT_RENDER_TIMEOUT_MS);
    let rendered = tokio::time::timeout(
        Duration::from_millis(render_timeout_ms),
        render.render(RenderRequest {
            uri: scope_start_url.to_string(),
            mode,
            timeout_ms: Some(render_timeout_ms),
            wait_ms: None,
            automation_script: None,
            credential_refs: Vec::new(),
            metadata,
        }),
    )
    .await;

    match rendered {
        Ok(Ok(page)) => Ok(page.html.map_or_else(Vec::new, |html| {
            extract_anchor_hrefs(&page.final_uri, &html, root_anchor_limit)
        })),
        Ok(Err(error)) => {
            log_info(&format!(
                "bounded-structure: browser render failed for {scope_start_url}: {error}"
            ));
            Err(format!(
                "browser root discovery failed for {scope_start_url}: {error}"
            ))
        }
        Err(_) => {
            let warning = format!(
                "browser root discovery timed out after {render_timeout_ms}ms for {scope_start_url}"
            );
            log_info(&format!("bounded-structure: {warning}"));
            Err(warning)
        }
    }
}

fn append_warning(existing: Option<String>, warning: String) -> Option<String> {
    Some(match existing {
        Some(existing) => format!("{existing}; {warning}"),
        None => warning,
    })
}

async fn discover_root_anchors(
    cfg: &Config,
    scope_start_url: &str,
    scope: &MapScope,
    fetch: Arc<dyn FetchProvider>,
    render: Arc<dyn RenderProvider>,
    execution_metadata: &MetadataMap,
) -> (Vec<String>, Option<String>) {
    let root_anchor_limit = effective_root_anchor_limit(cfg);
    let mut fast_warning = None;
    let anchor_urls = if let Some(html) = fetch_text_with_metadata(
        fetch.as_ref(),
        scope_start_url,
        Some(DISCOVERY_MAX_BODY_BYTES),
        execution_metadata,
    )
    .await
    {
        extract_anchor_hrefs(scope_start_url, &html, root_anchor_limit)
    } else if matches!(cfg.render_mode, CrawlRenderMode::Http) {
        // In HTTP-only mode the render provider is a useful second transport
        // after the lightweight fetch provider fails. For Chrome/AutoSwitch,
        // skip this duplicate HTTP render and fall through to the single
        // bounded browser attempt below. Besides avoiding redundant work, this
        // guarantees a slow root cannot consume two independent deadlines.
        match render_root_anchor_candidates(
            cfg,
            scope_start_url,
            root_anchor_limit,
            render.clone(),
            ProviderRenderMode::Http,
            execution_metadata,
        )
        .await
        {
            Ok(candidates) => candidates,
            Err(error) => {
                fast_warning = Some(format!(
                    "bounded-structure discovery failed for {scope_start_url}: {error}"
                ));
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    let filter_anchors = |candidates| {
        let urls = merge_map_candidate_urls(Vec::new(), candidates, scope, true);
        let mut urls: Vec<String> = urls
            .into_iter()
            .filter(|url| !is_excluded_url_path(url, &cfg.exclude_path_prefix))
            .collect();
        urls.sort();
        urls
    };
    let mut urls = filter_anchors(anchor_urls);

    if urls.is_empty()
        && matches!(
            cfg.render_mode,
            CrawlRenderMode::Chrome | CrawlRenderMode::AutoSwitch
        )
    {
        log_info(&format!(
            "bounded-structure: fast discovery empty; rendering root once for {scope_start_url}"
        ));
        match render_root_anchor_candidates(
            cfg,
            scope_start_url,
            root_anchor_limit,
            render,
            ProviderRenderMode::Chrome,
            execution_metadata,
        )
        .await
        {
            Ok(candidates) => {
                urls = filter_anchors(candidates);
                if urls.is_empty() {
                    let browser_warning = format!(
                        "browser root discovery completed for {scope_start_url} but returned no in-scope links"
                    );
                    fast_warning = append_warning(fast_warning, browser_warning);
                }
            }
            Err(browser_warning) => {
                fast_warning = append_warning(fast_warning, browser_warning);
            }
        }
    }

    let empty = urls.is_empty();
    (urls, fast_warning.filter(|_| empty))
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
    warning: Option<String>,
) -> MapResult {
    let outcome = if urls.is_empty() && warning.is_some() {
        MapDiscoveryOutcome::Failed
    } else if urls.is_empty() {
        MapDiscoveryOutcome::Empty
    } else {
        MapDiscoveryOutcome::Completed
    };
    MapResult {
        summary: CrawlSummary {
            elapsed_ms,
            ..Default::default()
        },
        sitemap_urls: raw_sitemap_count,
        urls,
        map_source: map_source.to_string(),
        outcome,
        warning,
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
    execution_metadata: &MetadataMap,
) -> DiscoveryProbes {
    let (seed_result, sitemap_result, llms_urls) = tokio::join!(
        async {
            resolve_map_seed_url_with_metadata(start_url, fetch.clone(), execution_metadata)
                .await
                .map_err(|e| e.to_string())
        },
        async {
            if cfg.discover_sitemaps {
                discover_sitemap_urls_with_metadata(
                    cfg,
                    start_url,
                    fetch.clone(),
                    execution_metadata,
                )
                .await
                .map_err(|e| e.to_string())
            } else {
                Ok(SitemapDiscovery::default())
            }
        },
        async {
            if cfg.discover_llms_txt {
                // warn-and-continue: never fail the map call on llms.txt errors.
                match crate::web_engine::engine::discover_llms_txt_urls_with_metadata(
                    cfg,
                    start_url,
                    fetch.clone(),
                    execution_metadata,
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
    render: Arc<dyn RenderProvider>,
) -> Result<MapResult, Box<dyn Error>> {
    discover_site_urls_with_metadata(cfg, start_url, fetch, render, &MetadataMap::new()).await
}

pub async fn discover_site_urls_with_metadata(
    cfg: &Config,
    start_url: &str,
    fetch: Arc<dyn FetchProvider>,
    render: Arc<dyn RenderProvider>,
    execution_metadata: &MetadataMap,
) -> Result<MapResult, Box<dyn Error>> {
    let start = Instant::now();

    let DiscoveryProbes {
        resolved_start_url,
        sitemap: sitemap_discovery,
        llms_urls,
    } = run_discovery_probes(cfg, start_url, fetch.clone(), execution_metadata).await;

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

    let scoped_sitemap_urls =
        scope_and_filter_map_urls(cfg, sitemap_discovery.urls.clone(), &scope);
    let scoped_llms_urls = scope_and_filter_map_urls(cfg, llms_urls.clone(), &scope);
    let combined = merge_discovery_candidate_urls(sitemap_discovery.urls, llms_urls);
    let discovery_urls = scope_and_filter_map_urls(cfg, combined, &scope);

    if cfg.sitemap_only {
        let warning = (sitemap_discovery.failed_fetches > 0
            && sitemap_discovery.parsed_sitemap_documents == 0)
            .then(|| {
                format!(
                    "sitemap discovery failed to fetch {} candidate(s)",
                    sitemap_discovery.failed_fetches
                )
            });
        return Ok(build_discovery_map_result(
            discovery_urls,
            raw_sitemap_count,
            discovery_source.unwrap_or("sitemap"),
            start.elapsed().as_millis(),
            warning,
        ));
    }

    // A sitemap that parsed successfully but yielded few in-scope URLs is not a
    // usable map — a stale sitemap, a thin one listing only section roots, or one
    // whose entries all fell outside scope. Gate on the RESULTING URL count, not
    // on how many documents parsed, so those cases still reach anchor discovery.
    if let Some(source) = discovery_source
        && discovery_is_sufficient(
            source,
            discovery_urls.len(),
            &scoped_sitemap_urls,
            &scoped_llms_urls,
        )
    {
        return Ok(build_discovery_map_result(
            discovery_urls,
            raw_sitemap_count,
            source,
            start.elapsed().as_millis(),
            None,
        ));
    }

    let (anchor_urls, fetch_warning) = discover_root_anchors(
        cfg,
        &scope_start_url,
        &scope,
        fetch,
        render,
        execution_metadata,
    )
    .await;
    let anchors_found = !anchor_urls.is_empty();

    // Layers are additive: discovery entries keep priority, anchors fill in the
    // rest, deduplicated through the same normalization the map already uses.
    let urls =
        merge_discovery_and_anchor_urls(discovery_urls, anchor_urls, &scoped_llms_urls, &scope);

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

    let outcome = if urls.is_empty() {
        if warning.is_some() {
            MapDiscoveryOutcome::Failed
        } else {
            MapDiscoveryOutcome::Empty
        }
    } else {
        MapDiscoveryOutcome::Completed
    };

    Ok(MapResult {
        summary: CrawlSummary {
            elapsed_ms: start.elapsed().as_millis(),
            ..Default::default()
        },
        sitemap_urls: raw_sitemap_count,
        urls,
        map_source,
        outcome,
        warning,
    })
}
