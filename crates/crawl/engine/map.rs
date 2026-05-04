mod strategy;

pub use strategy::map_with_sitemap;
pub(crate) use strategy::{append_html_anchor_backfill, crawl_and_collect_map};

use std::collections::HashSet;
use std::error::Error;

use spider::url::Url;

use crate::crates::core::http::{http_client, normalize_url, validate_url};

use super::is_excluded_url_path;
use super::sitemap::{SitemapDiscovery, discover_sitemap_urls};
use super::url_utils::{MapScope, canonicalize_url_for_dedupe, normalize_map_candidate_url};

/// The unified result of a `map` operation.
#[derive(Debug, Default)]
pub struct MapResult {
    pub summary: super::CrawlSummary,
    pub urls: Vec<String>,
    pub sitemap_urls: usize,
    pub map_source: String,
    pub warning: Option<String>,
}

/// Check URL against exclusions, also applying them relative to the effective scope root.
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
        match path[1..].find('/') {
            Some(n) => 1 + n,
            None => return false,
        }
    };

    let rel = match path.get(check_from..) {
        Some(r) if !r.is_empty() => r,
        _ => return false,
    };
    is_excluded_url_path(&format!("https://x{rel}"), excludes)
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
