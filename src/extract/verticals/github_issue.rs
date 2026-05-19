//! GitHub Issues vertical extractor via GitHub REST API.
//!
//! Matches github.com/{owner}/{repo}/issues/{number} URLs.
//! Uses GITHUB_TOKEN when available.
//! If the URL is actually a pull request, returns VerticalUnsupportedUrl.
//!
//! auto_dispatch: true

use crate::core::http::http_client;
use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "github_issue",
    label: "GitHub Issue",
    description: "Fetches GitHub issue metadata and body from api.github.com.",
    url_patterns: &["https://github.com/{owner}/{repo}/issues/{number}"],
    auto_dispatch: true,
};

pub fn matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if host != "github.com" {
        return false;
    }
    let segs: Vec<&str> = parsed
        .path()
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    // Exactly 4 segments: owner / repo / issues / number
    segs.len() == 4 && segs[2] == "issues" && segs[3].parse::<u64>().is_ok()
}

fn github_auth_header() -> Option<String> {
    let token = std::env::var("GITHUB_TOKEN")
        .ok()
        .filter(|s| !s.is_empty())?;
    Some(format!("Bearer {token}"))
}

fn parse_url_parts(url: &str) -> Option<(String, String, u64)> {
    let parsed = url::Url::parse(url).ok()?;
    let segs: Vec<&str> = parsed
        .path()
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    if segs.len() == 4 && segs[2] == "issues" {
        let owner = segs[0].to_string();
        let repo = segs[1].to_string();
        let number = segs[3].parse::<u64>().ok()?;
        return Some((owner, repo, number));
    }
    None
}

pub async fn extract(url: &str, ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
    let (owner, repo, number) =
        parse_url_parts(url).ok_or(VerticalError::VerticalUnsupportedUrl {
            vertical: INFO.name,
            url: url.to_string(),
        })?;

    let api_url = format!("https://api.github.com/repos/{owner}/{repo}/issues/{number}");
    let client = http_client().map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status: 0,
    })?;

    let mut req = client
        .get(&api_url)
        .header("User-Agent", ctx.api_ua())
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");

    if let Some(auth) = github_auth_header() {
        req = req.header("Authorization", auth);
    }

    let resp = req
        .send()
        .await
        .map_err(|_| VerticalError::VerticalTargetUnavailable {
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
        // 403 = unauthenticated rate limit (60/hr); 429 = explicit rate limit
        403 | 429 => {
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

    // If the API response has a `pull_request` key, this is a PR — wrong extractor
    if data.get("pull_request").is_some() {
        return Err(VerticalError::VerticalUnsupportedUrl {
            vertical: INFO.name,
            url: url.to_string(),
        });
    }

    build_scraped_doc(url, &owner, &repo, number, &data)
}

fn build_scraped_doc(
    url: &str,
    owner: &str,
    repo: &str,
    number: u64,
    data: &serde_json::Value,
) -> Result<ScrapedDoc, VerticalError> {
    let issue_title = data["title"].as_str().unwrap_or("Untitled").to_string();
    let title = format!("{owner}/{repo}#{number}: {issue_title}");
    let body = data["body"].as_str().unwrap_or("");
    let body_truncated: String = body.chars().take(8000).collect();
    let state = data["state"].as_str().unwrap_or("unknown");
    let author = data["user"]["login"].as_str().unwrap_or("unknown");
    let comments = data["comments"].as_u64().unwrap_or(0);
    let created_at = data["created_at"].as_str().unwrap_or("");
    let html_url = data["html_url"].as_str().unwrap_or(url);

    let labels: Vec<&str> = data["labels"]
        .as_array()
        .map(|a| a.iter().filter_map(|l| l["name"].as_str()).collect())
        .unwrap_or_default();

    let assignees: Vec<&str> = data["assignees"]
        .as_array()
        .map(|a| a.iter().filter_map(|u| u["login"].as_str()).collect())
        .unwrap_or_default();

    let milestone = data["milestone"]["title"].as_str().unwrap_or("");

    let mut md = format!("# {title}\n\n");
    md.push_str(&format!(
        "**State:** {state} | **Author:** {author} | **Comments:** {comments} | **Created:** {created_at}\n"
    ));
    if !labels.is_empty() {
        md.push_str(&format!("**Labels:** {}\n", labels.join(", ")));
    }
    if !assignees.is_empty() {
        md.push_str(&format!("**Assignees:** {}\n", assignees.join(", ")));
    }
    if !milestone.is_empty() {
        md.push_str(&format!("**Milestone:** {milestone}\n"));
    }
    if !body_truncated.is_empty() {
        md.push_str("\n## Body\n\n");
        md.push_str(&body_truncated);
        md.push('\n');
    }
    md.push_str(&format!("\n**GitHub:** {html_url}\n"));

    let structured = serde_json::json!({
        "number": number,
        "title": issue_title,
        "state": state,
        "author": author,
        "labels": labels,
        "assignees": assignees,
        "comments": comments,
        "created_at": created_at,
        "html_url": html_url,
    });

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title: Some(title),
        extractor_name: INFO.name,
        extractor_version: 1,
        structured: Some(structured),
        follow_crawl_urls: vec![],
    })
}

#[cfg(test)]
#[path = "github_issue_tests.rs"]
mod tests;
