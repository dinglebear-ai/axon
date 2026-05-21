//! GitHub repository vertical extractor (axon_rust-di8j reference extractor).
//!
//! Matches `https://github.com/{owner}/{repo}` (no sub-paths) and fetches
//! metadata from the GitHub REST API. Uses GITHUB_TOKEN when set.

use base64::Engine;

use crate::core::http::http_client;
use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};
use crate::ingest::git_payload::{GitPayload, build_git_payload};

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "github_repo",
    label: "GitHub Repository",
    description: "Fetches repository metadata from api.github.com — stars, description, license, language, topics, README.",
    url_patterns: &["https://github.com/{owner}/{repo}"],
    auto_dispatch: true,
};

/// Reserved GitHub paths that are NOT repository roots.
const RESERVED_OWNERS: &[&str] = &[
    "settings",
    "marketplace",
    "explore",
    "features",
    "pricing",
    "about",
    "login",
    "join",
    "organizations",
    "apps",
    "topics",
    "trending",
    "collections",
    "sponsors",
    "dashboard",
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
    !segs[1].contains('.')
}

/// Fetch the GitHub token from env (per-request, never global).
fn github_auth_header() -> Option<String> {
    let token = std::env::var("GITHUB_TOKEN")
        .ok()
        .filter(|s| !s.is_empty())?;
    Some(format!("Bearer {token}"))
}

/// Fetch and decode the README for a repo. Non-fatal — returns None on any error.
async fn fetch_readme(owner: &str, repo: &str) -> Option<String> {
    let client = http_client().ok()?;
    let readme_url = format!("https://api.github.com/repos/{owner}/{repo}/readme");
    let mut req = client
        .get(&readme_url)
        .header("User-Agent", crate::core::http::axon_api_ua())
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(auth) = github_auth_header() {
        req = req.header("Authorization", auth);
    }
    let resp = req.send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let json: serde_json::Value = resp.json().await.ok()?;
    let encoded = json["content"].as_str()?;
    // GitHub includes newlines in base64 — strip before decoding
    let cleaned: String = encoded
        .chars()
        .filter(|c| *c != '\n' && *c != '\r')
        .collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(cleaned.as_bytes())
        .ok()?;
    let text = String::from_utf8(bytes).ok()?;
    // Truncate to 40_000 chars
    let truncated: String = text.chars().take(40_000).collect();
    Some(truncated)
}

/// Aggregated data for building GitHub repo markdown.
struct RepoMarkdownData<'a> {
    repo_name: &'a str,
    description: &'a str,
    stars: u64,
    forks: u64,
    watchers: u64,
    open_issues: u64,
    language: &'a str,
    license: &'a str,
    homepage: &'a str,
    topics: &'a [&'a str],
    created_at: &'a str,
    updated_at: &'a str,
    size_kb: u64,
    readme: Option<&'a str>,
    url: &'a str,
}

/// Build the markdown output for a GitHub repo.
fn build_github_markdown(d: &RepoMarkdownData<'_>) -> String {
    let mut md = format!("# {}\n\n", d.repo_name);
    if !d.description.is_empty() {
        md.push_str(d.description);
        md.push_str("\n\n");
    }
    md.push_str(&format!(
        "**Stars:** {} | **Forks:** {} | **Watchers:** {} | **Open Issues:** {}",
        d.stars, d.forks, d.watchers, d.open_issues
    ));
    if !d.language.is_empty() {
        md.push_str(&format!(" | **Language:** {}", d.language));
    }
    if !d.license.is_empty() {
        md.push_str(&format!(" | **License:** {}", d.license));
    }
    md.push('\n');
    if !d.homepage.is_empty() {
        md.push_str(&format!("\n**Homepage:** {}\n", d.homepage));
    }
    if !d.topics.is_empty() {
        md.push_str(&format!("\n**Topics:** {}\n", d.topics.join(", ")));
    }
    if !d.created_at.is_empty() {
        md.push_str(&format!("**Created:** {}", d.created_at));
        if !d.updated_at.is_empty() {
            md.push_str(&format!(" | **Updated:** {}", d.updated_at));
        }
        md.push('\n');
    }
    if d.size_kb > 0 {
        md.push_str(&format!("**Size:** {} KB\n", d.size_kb));
    }
    md.push_str(&format!("\n**GitHub:** {}\n", d.url));
    if let Some(readme_text) = d.readme {
        md.push_str("\n## README\n\n");
        md.push_str(readme_text);
        md.push('\n');
    }
    md
}

fn build_extra(owner: &str, repo: &str, data: &serde_json::Value) -> serde_json::Value {
    let topics: Vec<String> = data["topics"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let mut extra = build_git_payload(&GitPayload {
        provider: "github".to_string(),
        host: "github.com".to_string(),
        owner: Some(owner.to_string()),
        repo: repo.to_string(),
        content_kind: "repo_metadata",
        ..Default::default()
    });
    if let Some(obj) = extra.as_object_mut() {
        obj.insert(
            "git_meta".to_string(),
            serde_json::json!({
                "stars": data["stargazers_count"].as_u64(),
                "forks": data["forks_count"].as_u64(),
                "language": data["language"].as_str(),
                "topics": topics,
                "visibility": data["visibility"].as_str(),
                "clone_url": data["clone_url"].as_str(),
            }),
        );
    }
    extra
}

/// Extract repository metadata from the GitHub REST API.
pub async fn extract(url: &str, ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
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

    let mut repo_req = client
        .get(&api_url)
        .header("User-Agent", ctx.api_ua())
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");

    if let Some(auth) = github_auth_header() {
        repo_req = repo_req.header("Authorization", auth);
    }

    // Fetch metadata and README in parallel
    let (repo_resp, readme) = tokio::join!(repo_req.send(), fetch_readme(owner, repo));

    let resp = repo_resp.map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status: 0,
    })?;

    let status = resp.status().as_u16();
    match status {
        404 => {
            return Err(VerticalError::VerticalTargetNotFound {
                vertical: INFO.name,
                url: url.to_string(),
            });
        }
        429 => {
            return Err(VerticalError::VerticalRateLimited {
                vertical: INFO.name,
                retry_after: None,
            });
        }
        200 => {}
        _ => {
            return Err(VerticalError::VerticalTargetUnavailable {
                vertical: INFO.name,
                status,
            });
        }
    }

    let data: serde_json::Value =
        resp.json()
            .await
            .map_err(|_| VerticalError::VerticalTargetUnavailable {
                vertical: INFO.name,
                status,
            })?;

    let title = data["full_name"].as_str().map(str::to_string);
    let description = data["description"].as_str().unwrap_or("").to_string();
    let stars = data["stargazers_count"].as_u64().unwrap_or(0);
    let forks = data["forks_count"].as_u64().unwrap_or(0);
    let watchers = data["watchers_count"].as_u64().unwrap_or(0);
    let open_issues = data["open_issues_count"].as_u64().unwrap_or(0);
    let language = data["language"].as_str().unwrap_or("").to_string();
    let license = data["license"]["spdx_id"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let homepage = data["homepage"].as_str().unwrap_or("").to_string();
    let created_at = data["created_at"].as_str().unwrap_or("").to_string();
    let updated_at = data["updated_at"].as_str().unwrap_or("").to_string();
    let size_kb = data["size"].as_u64().unwrap_or(0);
    let topics: Vec<&str> = data["topics"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let mut follow_crawl_urls: Vec<String> = vec![];
    if !homepage.is_empty() {
        follow_crawl_urls.push(homepage.clone());
    }

    let repo_name = title.as_deref().unwrap_or(repo);
    let md = build_github_markdown(&RepoMarkdownData {
        repo_name,
        description: &description,
        stars,
        forks,
        watchers,
        open_issues,
        language: &language,
        license: &license,
        homepage: &homepage,
        topics: &topics,
        created_at: &created_at,
        updated_at: &updated_at,
        size_kb,
        readme: readme.as_deref(),
        url,
    });

    let extra = build_extra(owner, repo, &data);

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title,
        extractor_name: INFO.name,
        extractor_version: 2,
        structured: Some(data),
        follow_crawl_urls,
        extra: Some(extra),
    })
}

#[cfg(test)]
#[path = "github_repo_tests.rs"]
mod tests;
