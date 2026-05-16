//! GitHub repository vertical extractor (axon_rust-di8j reference extractor).
//!
//! Matches `https://github.com/{owner}/{repo}` (no sub-paths) and fetches
//! metadata from the GitHub REST API. Uses GITHUB_TOKEN when set.

use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};
use crate::core::http::http_client;

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "github_repo",
    label: "GitHub Repository",
    description: "Fetches repository metadata from api.github.com — stars, description, license, language, topics.",
    url_patterns: &["https://github.com/{owner}/{repo}"],
    auto_dispatch: true,
};

/// Reserved GitHub paths that are NOT repository roots.
const RESERVED_OWNERS: &[&str] = &[
    "settings", "marketplace", "explore", "features", "pricing",
    "about", "login", "join", "organizations", "apps", "topics",
    "trending", "collections", "sponsors", "dashboard",
];

/// Returns `true` when `url` points to a GitHub repository root page
/// (exactly two non-empty path segments, no file/blob/tree paths).
pub fn matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if host != "github.com" {
        return false;
    }
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() != 2 {
        return false;
    }
    let owner = segs[0].to_lowercase();
    if RESERVED_OWNERS.contains(&owner.as_str()) {
        return false;
    }
    // Reject if the second segment looks like it has a file extension
    // (e.g. github.com/user/file.txt) — not a repo root.
    !segs[1].contains('.')
}

/// Extract repository metadata from the GitHub REST API.
pub async fn extract(url: &str, _ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
    let parsed = url::Url::parse(url).map_err(|_| VerticalError::VerticalUnsupportedUrl {
        vertical: INFO.name,
        url: url.to_string(),
    })?;
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() != 2 {
        return Err(VerticalError::VerticalUnsupportedUrl {
            vertical: INFO.name,
            url: url.to_string(),
        });
    }
    let (owner, repo) = (segs[0], segs[1]);
    let api_url = format!("https://api.github.com/repos/{owner}/{repo}");

    let client = http_client().map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status: 0,
    })?;

    let mut req = client.get(&api_url)
        .header("User-Agent", format!("axon/{} (+https://github.com/jmagar/axon_rust)", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");

    // Auth: per-request header (NOT global client header — prevents leaking
    // GITHUB_TOKEN to non-github.com hosts sharing the pool).
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        if !token.is_empty() {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
    }

    let resp = req.send().await.map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status: 0,
    })?;

    let status = resp.status().as_u16();
    match status {
        404 => return Err(VerticalError::VerticalTargetNotFound {
            vertical: INFO.name,
            url: url.to_string(),
        }),
        429 => return Err(VerticalError::VerticalRateLimited {
            vertical: INFO.name,
            retry_after: None,
        }),
        200 => {}
        _ => return Err(VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status,
        }),
    }

    let data: serde_json::Value = resp.json().await.map_err(|_| {
        VerticalError::VerticalTargetUnavailable { vertical: INFO.name, status }
    })?;

    let title = data["full_name"].as_str().map(str::to_string);
    let description = data["description"].as_str().unwrap_or("").to_string();
    let stars = data["stargazers_count"].as_u64().unwrap_or(0);
    let forks = data["forks_count"].as_u64().unwrap_or(0);
    let language = data["language"].as_str().unwrap_or("").to_string();
    let license = data["license"]["spdx_id"].as_str().unwrap_or("").to_string();
    let homepage = data["homepage"].as_str().unwrap_or("").to_string();
    let topics: Vec<&str> = data["topics"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let mut md = format!("# {}\n\n", title.as_deref().unwrap_or(repo));
    if !description.is_empty() {
        md.push_str(&description);
        md.push_str("\n\n");
    }
    md.push_str(&format!("**Stars:** {stars} | **Forks:** {forks}"));
    if !language.is_empty() {
        md.push_str(&format!(" | **Language:** {language}"));
    }
    if !license.is_empty() {
        md.push_str(&format!(" | **License:** {license}"));
    }
    md.push('\n');
    if !homepage.is_empty() {
        md.push_str(&format!("\n**Homepage:** {homepage}\n"));
    }
    if !topics.is_empty() {
        md.push_str(&format!("\n**Topics:** {}\n", topics.join(", ")));
    }
    md.push_str(&format!("\n**GitHub:** {url}\n"));

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title,
        extractor_name: INFO.name,
        extractor_version: 1,
        structured: Some(data),
    })
}
