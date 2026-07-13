//! GitHub Pull Requests vertical extractor via GitHub REST API.
//!
//! Matches github.com/{owner}/{repo}/pull/{number} URLs.
//! Uses GITHUB_TOKEN when available.
//!
//! auto_dispatch: true

use crate::context::VerticalContext;
use crate::error::VerticalError;
use crate::git_payload::{ContentKind, GitPayload, build_git_payload};
use crate::types::{ExtractorInfo, ScrapedDoc};
use axon_core::http::http_client;

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "github_pr",
    label: "GitHub Pull Request",
    description: "Fetches GitHub pull request metadata and description from api.github.com.",
    url_patterns: &["https://github.com/{owner}/{repo}/pull/{number}"],
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
    // Exactly 4 segments: owner / repo / pull / number
    segs.len() == 4 && segs[2] == "pull" && segs[3].parse::<u64>().is_ok()
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
    if segs.len() == 4 && segs[2] == "pull" {
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

    let api_url = format!("https://api.github.com/repos/{owner}/{repo}/pulls/{number}");
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

    build_scraped_doc(url, &owner, &repo, number, &data)
}

fn build_extra(
    owner: &str,
    repo: &str,
    number: u64,
    data: &serde_json::Value,
) -> serde_json::Value {
    let state = data["state"].as_str().unwrap_or("");
    let merged_at = data["merged_at"].as_str().unwrap_or("");
    let author = data["user"]["login"].as_str().unwrap_or("");
    let labels: Vec<String> = data["labels"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|l| l["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let pr_state = if state == "closed" && !merged_at.is_empty() {
        "merged"
    } else {
        state
    };
    build_git_payload(&GitPayload {
        provider: "github".to_string(),
        host: "github.com".to_string(),
        owner: Some(owner.to_string()),
        repo: repo.to_string(),
        content_kind: ContentKind::Pr,
        state: if pr_state.is_empty() {
            None
        } else {
            Some(pr_state.to_string())
        },
        number: Some(number),
        author: if author.is_empty() {
            None
        } else {
            Some(author.to_string())
        },
        labels,
        is_draft: Some(data["draft"].as_bool().unwrap_or(false)),
        merged_at: if merged_at.is_empty() {
            None
        } else {
            Some(merged_at.to_string())
        },
        created_at: data["created_at"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        ..Default::default()
    })
}

fn build_scraped_doc(
    url: &str,
    owner: &str,
    repo: &str,
    number: u64,
    data: &serde_json::Value,
) -> Result<ScrapedDoc, VerticalError> {
    let pr_title = data["title"].as_str().unwrap_or("Untitled").to_string();
    let title = format!("{owner}/{repo}#{number}: {pr_title}");
    let body = data["body"].as_str().unwrap_or("");
    let body_truncated: String = body.chars().take(8000).collect();
    let state = data["state"].as_str().unwrap_or("unknown");
    let draft = data["draft"].as_bool().unwrap_or(false);
    let merged = data["merged"].as_bool().unwrap_or(false);
    let merged_at = data["merged_at"].as_str().unwrap_or("");
    let author = data["user"]["login"].as_str().unwrap_or("unknown");
    let head_ref = data["head"]["ref"].as_str().unwrap_or("");
    let base_ref = data["base"]["ref"].as_str().unwrap_or("");
    let additions = data["additions"].as_u64().unwrap_or(0);
    let deletions = data["deletions"].as_u64().unwrap_or(0);
    let changed_files = data["changed_files"].as_u64().unwrap_or(0);
    let commits = data["commits"].as_u64().unwrap_or(0);
    let comments = data["comments"].as_u64().unwrap_or(0);
    let review_comments = data["review_comments"].as_u64().unwrap_or(0);
    let html_url = data["html_url"].as_str().unwrap_or(url);

    let labels: Vec<&str> = data["labels"]
        .as_array()
        .map(|a| a.iter().filter_map(|l| l["name"].as_str()).collect())
        .unwrap_or_default();

    let mut state_desc = state.to_string();
    if draft {
        state_desc.push_str(" [DRAFT]");
    }
    if merged {
        state_desc.push_str(" [MERGED]");
    }

    let mut md = format!("# {title}\n\n");
    md.push_str(&format!("**State:** {state_desc} | **Author:** {author}\n"));
    if !base_ref.is_empty() || !head_ref.is_empty() {
        md.push_str(&format!("**Base:** {base_ref} ← **Head:** {head_ref}\n"));
    }
    md.push_str(&format!(
        "**Changes:** +{additions}/-{deletions} in {changed_files} files, {commits} commits\n"
    ));
    md.push_str(&format!(
        "**Reviews:** {review_comments} | **Comments:** {comments}\n"
    ));
    if !labels.is_empty() {
        md.push_str(&format!("**Labels:** {}\n", labels.join(", ")));
    }
    if merged && !merged_at.is_empty() {
        md.push_str(&format!("**Merged at:** {merged_at}\n"));
    }
    if !body_truncated.is_empty() {
        md.push_str("\n## Description\n\n");
        md.push_str(&body_truncated);
        md.push('\n');
    }
    md.push_str(&format!("\n**GitHub:** {html_url}\n"));

    let structured = serde_json::json!({
        "number": number,
        "title": pr_title,
        "state": state,
        "draft": draft,
        "merged": merged,
        "author": author,
        "head_ref": head_ref,
        "base_ref": base_ref,
        "additions": additions,
        "deletions": deletions,
        "changed_files": changed_files,
        "commits": commits,
        "labels": labels,
        "html_url": html_url,
    });

    let extra = build_extra(owner, repo, number, data);

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title: Some(title),
        extractor_name: INFO.name,
        extractor_version: 2,
        structured: Some(structured),
        follow_crawl_urls: vec![],
        extra: Some(extra),
    })
}

#[cfg(test)]
#[path = "github_pr_tests.rs"]
mod tests;
