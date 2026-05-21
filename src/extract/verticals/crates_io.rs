//! crates.io package vertical extractor.
//!
//! Three network calls (README + rustdoc JSON are parallel):
//!   1. `crates.io/api/v1/crates/{name}` — metadata (sequential, version needed)
//!   2. `crates.io/api/v1/crates/{name}/{version}/readme` — README HTML
//!   3. `docs.rs/crate/{name}/{version}/json.gz` — rustdoc JSON via `docs_rs::fetch_rustdoc_docs`
//!
//! README and rustdoc fetches are non-fatal and run concurrently via `tokio::join!`.
//! docs.rs started building rustdoc JSON on 2025-05-23; older releases fall back
//! gracefully to metadata + README only.

use super::docs_rs::fetch_rustdoc_docs;
use crate::core::http::http_client;
use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "crates_io",
    label: "crates.io Crate",
    description: "Fetches crate metadata, README, and full API docs (rustdoc JSON) from \
        crates.io + docs.rs — version, MSRV, edition, downloads, license, features, \
        categories, keywords, and all public items with their doc comments.",
    url_patterns: &[
        "https://crates.io/crates/{name}",
        "https://crates.io/crates/{name}/{version}",
    ],
    auto_dispatch: true,
};

pub fn matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if host != "crates.io" {
        return false;
    }
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    segs.len() >= 2 && segs[0] == "crates"
}

fn build_extra(data: &serde_json::Value) -> serde_json::Value {
    let krate = &data["crate"];
    let ver = &data["versions"][0];
    let name = krate["name"].as_str().unwrap_or("");
    let max_version = krate["max_stable_version"]
        .as_str()
        .or_else(|| krate["newest_version"].as_str())
        .unwrap_or("");
    let license = ver["license"].as_str().unwrap_or("");
    let downloads = krate["downloads"].as_u64().unwrap_or(0);
    let homepage = krate["homepage"].as_str().unwrap_or("");
    let repository = krate["repository"].as_str().unwrap_or("");
    let msrv = ver["rust_version"].as_str().unwrap_or("");
    let edition = ver["edition"].as_str().unwrap_or("");
    let keywords: Vec<&str> = data["keywords"]
        .as_array()
        .map(|a| a.iter().filter_map(|k| k["keyword"].as_str()).collect())
        .unwrap_or_default();
    let mut obj = serde_json::json!({
        "pkg_registry": "crates_io",
        "pkg_name": name,
        "pkg_version": max_version,
        "pkg_language": "rust"
    });
    if !license.is_empty() {
        obj["pkg_license"] = serde_json::Value::String(license.to_string());
    }
    if !keywords.is_empty() {
        obj["pkg_keywords"] = serde_json::json!(keywords);
    }
    if downloads > 0 {
        obj["pkg_downloads"] = serde_json::json!(downloads);
    }
    if !homepage.is_empty() {
        obj["pkg_homepage"] = serde_json::Value::String(homepage.to_string());
    }
    if !repository.is_empty() {
        obj["pkg_repo_url"] = serde_json::Value::String(repository.to_string());
    }
    if !msrv.is_empty() {
        obj["crate_msrv"] = serde_json::Value::String(msrv.to_string());
    }
    if !edition.is_empty() {
        obj["crate_edition"] = serde_json::Value::String(edition.to_string());
    }
    obj
}

pub async fn extract(url: &str, ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
    let parsed = url::Url::parse(url).map_err(|_| VerticalError::VerticalUnsupportedUrl {
        vertical: INFO.name,
        url: url.to_string(),
    })?;
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() < 2 || segs[0] != "crates" {
        return Err(VerticalError::VerticalUnsupportedUrl {
            vertical: INFO.name,
            url: url.to_string(),
        });
    }

    let name = segs[1];
    let ua = ctx.api_ua();
    let client = http_client().map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status: 0,
    })?;

    let data = fetch_crate_json(client, name, url, ua).await?;
    // If the URL specifies an explicit version (e.g. /crates/serde/1.0.219),
    // use it for README and rustdoc lookups so we fetch the right release.
    let url_version = segs.get(2).filter(|v| !v.is_empty()).map(|v| v.to_string());
    let version = url_version.unwrap_or_else(|| resolve_version(&data, name));

    // Fetch README and rustdoc JSON concurrently — both non-fatal.
    let (readme_text, rustdoc_text) = tokio::join!(
        fetch_readme(client, name, &version, ua),
        fetch_rustdoc_docs(client, name, &version, ua),
    );

    let (markdown, title) = build_markdown(
        &data,
        name,
        url,
        readme_text.as_deref(),
        rustdoc_text.as_deref(),
    );

    let extra = build_extra(&data);

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown,
        title,
        extractor_name: INFO.name,
        extractor_version: 3,
        structured: Some(data),
        follow_crawl_urls: vec![],
        extra: Some(extra),
    })
}

/// crates.io policy: ≤1 request/second. We retry on 429 with Retry-After
/// backoff (up to 3 attempts). The Retry-After header is seconds; we cap at
/// 60 s and default to 10 s when the header is absent.
async fn fetch_crate_json(
    client: &reqwest::Client,
    name: &str,
    url: &str,
    ua: &str,
) -> Result<serde_json::Value, VerticalError> {
    let api_url = format!("https://crates.io/api/v1/crates/{name}");
    const MAX_ATTEMPTS: u32 = 3;

    for attempt in 0..MAX_ATTEMPTS {
        let resp = client
            .get(&api_url)
            .header("User-Agent", ua)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|_| VerticalError::VerticalTargetUnavailable {
                vertical: INFO.name,
                status: 0,
            })?;

        let status = resp.status().as_u16();
        match status {
            200 => {
                return resp
                    .json()
                    .await
                    .map_err(|_| VerticalError::VerticalTargetUnavailable {
                        vertical: INFO.name,
                        status,
                    });
            }
            404 => {
                return Err(VerticalError::VerticalTargetNotFound {
                    vertical: INFO.name,
                    url: url.to_string(),
                });
            }
            429 => {
                let wait_secs = parse_retry_after(&resp).unwrap_or(10).min(60);
                if attempt + 1 < MAX_ATTEMPTS {
                    tokio::time::sleep(std::time::Duration::from_secs(wait_secs)).await;
                    continue;
                }
                return Err(VerticalError::VerticalRateLimited {
                    vertical: INFO.name,
                    retry_after: Some(std::time::Duration::from_secs(wait_secs)),
                });
            }
            _ => {
                return Err(VerticalError::VerticalTargetUnavailable {
                    vertical: INFO.name,
                    status,
                });
            }
        }
    }
    unreachable!()
}

/// Parse the `Retry-After` header as seconds. Returns `None` when absent or
/// unparseable (caller applies a default).
fn parse_retry_after(resp: &reqwest::Response) -> Option<u64> {
    resp.headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

fn resolve_version(data: &serde_json::Value, fallback: &str) -> String {
    let krate = &data["crate"];
    krate["max_stable_version"]
        .as_str()
        .or_else(|| krate["newest_version"].as_str())
        .unwrap_or(fallback)
        .to_string()
}

// ── README ────────────────────────────────────────────────────────────────────

async fn fetch_readme(
    client: &reqwest::Client,
    name: &str,
    version: &str,
    ua: &str,
) -> Option<String> {
    let url = format!("https://crates.io/api/v1/crates/{name}/{version}/readme");
    let resp = client
        .get(&url)
        .header("User-Agent", ua)
        .header("Accept", "text/html, text/plain")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    Some(strip_html(&resp.text().await.ok()?))
}

/// Strip HTML tags and collapse whitespace — keeps README readable as plain text.
fn strip_html(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push('\n');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    let mut result = String::new();
    let mut prev_blank = false;
    for line in out.lines() {
        let t = line.trim();
        if t.is_empty() {
            if !prev_blank {
                result.push('\n');
            }
            prev_blank = true;
        } else {
            result.push_str(t);
            result.push('\n');
            prev_blank = false;
        }
    }
    result.trim().to_string()
}
// ── Markdown assembly ─────────────────────────────────────────────────────────

fn build_markdown(
    data: &serde_json::Value,
    name: &str,
    url: &str,
    readme: Option<&str>,
    rustdoc: Option<&str>,
) -> (String, Option<String>) {
    let krate = &data["crate"];
    let crate_name = krate["name"].as_str().unwrap_or(name);
    let max_version = krate["max_stable_version"]
        .as_str()
        .or_else(|| krate["newest_version"].as_str())
        .unwrap_or("unknown");
    let description = krate["description"].as_str().unwrap_or("").trim();
    let ver = &data["versions"][0];

    let title = Some(format!("{crate_name} {max_version}"));
    let mut md = format!("# {crate_name} {max_version}\n\n");
    if !description.is_empty() {
        md.push_str(description);
        md.push_str("\n\n");
    }
    md.push_str("## Crate Metadata\n\n");
    append_metadata(&mut md, krate, ver, max_version, url);

    append_tags(&mut md, data, ver);

    if let Some(r) = readme {
        md.push_str("\n\n---\n\n## README\n\n");
        md.push_str(r);
    }
    if let Some(r) = rustdoc {
        md.push_str(r);
    }
    (md, title)
}

fn append_tags(md: &mut String, data: &serde_json::Value, ver: &serde_json::Value) {
    let keywords: Vec<&str> = data["keywords"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v["keyword"].as_str()).collect())
        .unwrap_or_default();
    let categories: Vec<&str> = data["categories"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v["category"].as_str()).collect())
        .unwrap_or_default();
    let features: Vec<&str> = ver["features"]
        .as_object()
        .map(|f| f.keys().map(|k| k.as_str()).collect())
        .unwrap_or_default();
    if !keywords.is_empty() {
        md.push_str(&format!("\n**Keywords:** {}\n", keywords.join(", ")));
    }
    if !categories.is_empty() {
        md.push_str(&format!("\n**Categories:** {}\n", categories.join(", ")));
    }
    if !features.is_empty() {
        md.push_str(&format!("\n**Features:** {}\n", features.join(", ")));
    }
}

fn append_metadata(
    md: &mut String,
    krate: &serde_json::Value,
    ver: &serde_json::Value,
    max_version: &str,
    url: &str,
) {
    let license = ver["license"].as_str().unwrap_or("");
    let msrv = ver["rust_version"].as_str().unwrap_or("");
    let edition = ver["edition"].as_str().unwrap_or("");
    let version_published = ver["created_at"].as_str().unwrap_or("");
    let crate_size = ver["crate_size"].as_u64();
    let downloads_total = krate["downloads"].as_u64().unwrap_or(0);
    let downloads_recent = krate["recent_downloads"].as_u64().unwrap_or(0);
    let num_versions = krate["num_versions"].as_u64().unwrap_or(0);
    let repository = krate["repository"].as_str().unwrap_or("");
    let homepage = krate["homepage"].as_str().unwrap_or("");
    let documentation = krate["documentation"].as_str().unwrap_or("");
    let created_at = krate["created_at"].as_str().unwrap_or("");
    let updated_at = krate["updated_at"].as_str().unwrap_or("");

    md.push_str(&format!("- **Version:** {max_version}\n"));
    if !license.is_empty() {
        md.push_str(&format!("- **License:** {license}\n"));
    }
    if !msrv.is_empty() {
        md.push_str(&format!("- **MSRV (min Rust):** {msrv}\n"));
    }
    if !edition.is_empty() {
        md.push_str(&format!("- **Edition:** Rust {edition}\n"));
    }
    if let Some(size) = crate_size {
        md.push_str(&format!("- **Crate size:** {} KB\n", size / 1024));
    }
    md.push_str(&format!(
        "- **Downloads:** {} total, {} recent (90d)\n",
        fmt_num(downloads_total),
        fmt_num(downloads_recent),
    ));
    md.push_str(&format!("- **Versions published:** {num_versions}\n"));
    if version_published.len() >= 10 {
        md.push_str(&format!(
            "- **Latest published:** {}\n",
            &version_published[..10]
        ));
    }
    if created_at.len() >= 10 {
        md.push_str(&format!("- **First published:** {}\n", &created_at[..10]));
    }
    if updated_at.len() >= 10 {
        md.push_str(&format!("- **Updated:** {}\n", &updated_at[..10]));
    }
    if !repository.is_empty() {
        md.push_str(&format!("- **Repository:** {repository}\n"));
    }
    if !homepage.is_empty() {
        md.push_str(&format!("- **Homepage:** {homepage}\n"));
    }
    if !documentation.is_empty() {
        md.push_str(&format!("- **Documentation:** {documentation}\n"));
    }
    md.push_str(&format!("- **crates.io:** {url}\n"));
}

fn fmt_num(n: u64) -> String {
    let s = n.to_string();
    let mut chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut i = len as isize - 3;
    while i > 0 {
        chars.insert(i as usize, ',');
        i -= 3;
    }
    chars.into_iter().collect()
}

#[cfg(test)]
#[path = "crates_io_tests.rs"]
mod tests;
