//! docs.rs vertical extractor — rustdoc JSON API reference.
//!
//! Matches any `https://docs.rs/{crate}` or `https://docs.rs/{crate}/{version}/...`
//! URL and fetches the gzip-compressed rustdoc JSON from docs.rs instead of
//! scraping HTML. The JSON contains every public item with its doc comment,
//! type signature, and module path — far richer than what HTML scraping produces.
//!
//! docs.rs started building rustdoc JSON on 2025-05-23. Older releases get a
//! graceful 404 and this extractor returns `VerticalTargetNotFound`.
//!
//! `fetch_rustdoc_docs()` is `pub(super)` so `crates_io.rs` can call it
//! without duplicating the logic.

use crate::context::VerticalContext;
use crate::error::VerticalError;
use crate::types::{ExtractorInfo, ScrapedDoc};
use axon_core::http::http_client;

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "docs_rs",
    label: "docs.rs API Reference",
    description: "Fetches rustdoc JSON from docs.rs — all public items with doc comments, \
        type signatures, and module paths. Structured machine-readable format; \
        no HTML noise.",
    url_patterns: &[
        "https://docs.rs/{crate}",
        "https://docs.rs/{crate}/{version}",
        "https://docs.rs/{crate}/{version}/{module}/...",
    ],
    auto_dispatch: true,
};

/// Non-crate top-level docs.rs paths that should not be auto-dispatched.
const RESERVED_PATHS: &[&str] = &[
    "releases",
    "about",
    "help",
    "crate",
    "search",
    "login",
    "sync",
    "settings",
    "queue",
    "features",
    "badge",
    "robots.txt",
];

/// Returns `true` when the URL is a docs.rs page for a specific crate.
/// Rejects known non-crate paths (releases, about, etc.) so they fall
/// through to the generic scraper.
pub fn matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    if parsed.host_str().map(|h| h.eq_ignore_ascii_case("docs.rs")) != Some(true) {
        return false;
    }
    let path = parsed.path().trim_matches('/');
    let first = path.split('/').next().unwrap_or("");
    !first.is_empty() && !RESERVED_PATHS.contains(&first)
}

fn build_extra(name: &str, version: &str, item_count: usize) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "pkg_registry": "docs_rs",
        "pkg_name": name,
        "pkg_version": version,
        "pkg_language": "rust"
    });
    if item_count > 0 {
        obj["docrs_item_count"] = serde_json::json!(item_count);
    }
    obj
}

pub async fn extract(url: &str, ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
    let (name, version) =
        parse_crate_and_version(url).ok_or_else(|| VerticalError::VerticalUnsupportedUrl {
            vertical: INFO.name,
            url: url.to_string(),
        })?;

    let ua = ctx.api_ua();
    let client = http_client().map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status: 0,
    })?;

    let api_docs = fetch_rustdoc_docs(client, &name, &version, ua)
        .await
        .ok_or_else(|| VerticalError::VerticalTargetNotFound {
            vertical: INFO.name,
            url: url.to_string(),
        })?;

    // Approximate item count from the markdown (each public item becomes a ### heading).
    let item_count = api_docs.matches("\n### ").count();
    let extra = build_extra(&name, &version, item_count);

    // Pull the resolved version from the JSON itself for an accurate title.
    let (markdown, title) = build_markdown(&name, &api_docs, url);

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown,
        title,
        extractor_name: INFO.name,
        extractor_version: 2,
        structured: None,
        follow_crawl_urls: vec![],
        extra: Some(extra),
    })
}

/// Parse `(crate_name, version)` from a docs.rs URL.
///
/// - `https://docs.rs/tokio`               → ("tokio", "latest")
/// - `https://docs.rs/tokio/1.38.0`        → ("tokio", "1.38.0")
/// - `https://docs.rs/tokio/latest/tokio/sync` → ("tokio", "latest")
fn parse_crate_and_version(url: &str) -> Option<(String, String)> {
    let parsed = url::Url::parse(url).ok()?;
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let name = segs.first()?.to_string();
    let version = segs
        .get(1)
        .filter(|v| *v == &"latest" || v.starts_with(|c: char| c.is_ascii_digit()))
        .map(|v| v.to_string())
        .unwrap_or_else(|| "latest".to_string());
    Some((name, version))
}

fn build_markdown(name: &str, api_docs: &str, url: &str) -> (String, Option<String>) {
    let title = format!("{name} API Reference");
    let mut md = format!("# {title}\n\n");
    md.push_str(&format!("Source: {url}\n\n"));
    md.push_str(api_docs);
    (md, Some(title))
}

// ── Rustdoc JSON fetching (also used by crates_io.rs) ─────────────────────────

/// Fetch and convert the rustdoc JSON gzip for the crate to markdown.
///
/// Tries the specific version first, then falls back to "latest". Returns
/// `None` when docs.rs has no JSON for this version or on any network error.
///
/// `pub(super)` so `crates_io.rs` can call this without duplicating logic.
pub(super) async fn fetch_rustdoc_docs(
    client: &reqwest::Client,
    name: &str,
    version: &str,
    ua: &str,
) -> Option<String> {
    // Build the candidate list, deduplicating if version is already "latest"
    let candidates: Vec<&str> = if version == "latest" {
        vec!["latest"]
    } else {
        vec![version, "latest"]
    };
    for ver in candidates {
        let url = format!("https://docs.rs/crate/{name}/{ver}/json.gz");
        if let Some(md) = try_fetch_rustdoc_gz(client, &url, ua, name).await {
            return Some(md);
        }
    }
    None
}

/// Fetch one rustdoc JSON gzip URL, retrying on 429 with Retry-After backoff.
/// docs.rs is CDN-served (429 is rare), but we honour the header for correctness.
async fn try_fetch_rustdoc_gz(
    client: &reqwest::Client,
    url: &str,
    ua: &str,
    crate_name: &str,
) -> Option<String> {
    const MAX_ATTEMPTS: u32 = 3;
    for attempt in 0..MAX_ATTEMPTS {
        let resp = client.get(url).header("User-Agent", ua).send().await.ok()?;
        let status = resp.status();
        if status.as_u16() == 429 {
            let wait_secs = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(10)
                .min(60);
            if attempt + 1 < MAX_ATTEMPTS {
                tokio::time::sleep(std::time::Duration::from_secs(wait_secs)).await;
                continue;
            }
            return None;
        }
        if !status.is_success() {
            return None;
        }
        let bytes = resp.bytes().await.ok()?;
        let json = decompress_and_parse_gz(&bytes)?;
        return Some(rustdoc_to_markdown(&json, crate_name));
    }
    None
}

fn decompress_and_parse_gz(bytes: &[u8]) -> Option<serde_json::Value> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    let mut decoder = GzDecoder::new(bytes);
    let mut buf = Vec::new();
    decoder.read_to_end(&mut buf).ok()?;
    serde_json::from_slice(&buf).ok()
}

/// Item kinds that are containers or sub-items — skipped to avoid redundancy.
fn should_skip_kind(kind: &str) -> bool {
    matches!(
        kind,
        "module"
            | "impl"
            | "use"
            | "extern_crate"
            | "struct_field"
            | "variant"
            | "assoc_type"
            | "assoc_const"
            | "keyword"
            | "primitive"
    )
}

/// Convert rustdoc JSON to a markdown section listing every public item with
/// its doc comment. Capped at 150 000 chars for very large crates.
fn rustdoc_to_markdown(data: &serde_json::Value, crate_name: &str) -> String {
    let Some(index) = data["index"].as_object() else {
        return String::new();
    };
    let paths = data["paths"].as_object();

    let mut items: Vec<(String, &str, &str)> = vec![];
    for (id, item) in index {
        if item["visibility"].as_str() != Some("public") {
            continue;
        }
        let docs = match item["docs"].as_str().filter(|d| !d.is_empty()) {
            Some(d) => d,
            None => continue,
        };
        let kind = match item["inner"]
            .as_object()
            .and_then(|o| o.keys().next())
            .map(|s| s.as_str())
        {
            Some(k) => k,
            None => continue,
        };
        if should_skip_kind(kind) {
            continue;
        }
        let path = paths
            .and_then(|p| p.get(id.as_str()))
            .and_then(|v| v["path"].as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join("::")
            })
            .unwrap_or_else(|| item["name"].as_str().unwrap_or("?").to_string());
        items.push((path, kind, docs));
    }

    items.sort_by(|a, b| a.0.cmp(&b.0));

    // Use crate_version from the JSON if present.
    let version = data["crate_version"].as_str().unwrap_or("?");
    let mut md = format!("## {crate_name} {version} API Reference\n\n");
    let mut total = 0usize;
    const CAP: usize = 150_000;
    let count = items.len();

    for (path, kind, docs) in &items {
        if total >= CAP {
            break;
        }
        let preview: String = docs.chars().take(600).collect();
        let suffix = if docs.len() > 600 { "..." } else { "" };
        let entry = format!("### `{path}` ({kind})\n\n{preview}{suffix}\n\n");
        total += entry.len();
        md.push_str(&entry);
    }

    md.push_str(&format!("*{count} public items with documentation.*\n"));
    md
}

#[cfg(test)]
#[path = "docs_rs_tests.rs"]
mod tests;
