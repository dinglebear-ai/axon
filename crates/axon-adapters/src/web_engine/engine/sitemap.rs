//! Sitemap discovery + post-crawl backfill.
//!
//! Module root holds the shared bounded-HTTP fetch helpers used by both
//! halves (and by llms.txt discovery); the submodules own the pipeline:
//! - [`discover`] — robots.txt/seed-path sitemap discovery
//! - [`backfill`] — fetch missed URLs, convert, append to manifest
//! - [`filter`] — URL scope, `<lastmod>`, and markdown-passthrough checks

mod backfill;
mod discover;
mod filter;

use super::collector::sanitized_url_for_log;

pub use backfill::append_candidate_backfill;
pub use backfill::{BackfillStats, append_sitemap_backfill};
pub(crate) use discover::sitemap_url_limit;
pub use discover::{SitemapDiscovery, discover_sitemap_urls, discover_sitemap_urls_with_metadata};
#[cfg(test)]
pub(crate) use discover::{insert_discovered_url, sitemap_fetch_limit};
#[cfg(test)]
use filter::is_already_markdown;
pub use filter::loc_in_scope;

use crate::boundary::FetchProvider;
use axon_api::source::{ContentRef, FetchRequest, MetadataMap, RedactedHeaders};
use axon_core::logging::log_warn;
use base64::Engine;
use spider::url::Url;
use std::error::Error;

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

/// Fetch bounded discovery content through the adapter boundary. Retry, cooldown,
/// redirect validation, and reservation accounting belong to the provider, never to
/// sitemap/llms/backfill callers.
pub(crate) async fn fetch_text(
    fetch: &dyn FetchProvider,
    url: &str,
    max_bytes: Option<u64>,
) -> Option<String> {
    fetch_text_with_metadata(fetch, url, max_bytes, &MetadataMap::new()).await
}

pub(crate) async fn fetch_text_with_metadata(
    fetch: &dyn FetchProvider,
    url: &str,
    max_bytes: Option<u64>,
    metadata: &MetadataMap,
) -> Option<String> {
    let request = FetchRequest {
        uri: url.to_string(),
        method: "GET".to_string(),
        headers: RedactedHeaders {
            headers: Vec::new(),
        },
        body: None,
        timeout_ms: None,
        max_bytes,
        credential_refs: Vec::new(),
        metadata: metadata.clone(),
    };
    let resource = match fetch.fetch(request).await {
        Ok(resource) if (200..300).contains(&resource.status) => resource,
        Ok(_) => return None,
        Err(error) => {
            log_warn(&format!(
                "command=fetch provider error url={}: {error}",
                sanitized_url_for_log(url)
            ));
            return None;
        }
    };
    let bytes = match resource.content {
        ContentRef::InlineText { text } => text.into_bytes(),
        ContentRef::InlineBytes { bytes_base64, .. } => {
            match base64::engine::general_purpose::STANDARD.decode(bytes_base64) {
                Ok(bytes) => bytes,
                Err(error) => {
                    log_warn(&format!(
                        "command=fetch invalid inline bytes url={}: {error}",
                        sanitized_url_for_log(url)
                    ));
                    return None;
                }
            }
        }
        ContentRef::Artifact { .. } | ContentRef::External { .. } => return None,
    };
    if max_bytes.is_some_and(|cap| bytes.len() as u64 > cap) {
        log_warn(&format!(
            "command=fetch oversized body rejected cap_bytes={} url={}",
            max_bytes.unwrap_or_default(),
            sanitized_url_for_log(url)
        ));
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
#[path = "sitemap_tests.rs"]
mod tests;
