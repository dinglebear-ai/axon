//! GitHub Releases vertical extractor.
//!
//! Matches github.com/{owner}/{repo}/releases and /releases/tag/{tag}.
//! Fetches release metadata from the GitHub REST API.

use crate::context::VerticalContext;
use crate::error::VerticalError;
use crate::git_payload::{ContentKind, GitPayload, build_git_payload};
use crate::types::{ExtractorInfo, ScrapedDoc};
use axon_core::http::http_client;

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "github_release",
    label: "GitHub Release",
    description: "Fetches release metadata from api.github.com — tag, body, assets, published date.",
    url_patterns: &[
        "https://github.com/{owner}/{repo}/releases",
        "https://github.com/{owner}/{repo}/releases/tag/{tag}",
    ],
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
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    // /owner/repo/releases  OR  /owner/repo/releases/tag/{tag}
    if segs.len() < 3 {
        return false;
    }
    segs[2] == "releases"
}

fn format_release_markdown(
    owner: &str,
    repo: &str,
    url: &str,
    data: &serde_json::Value,
) -> (String, Option<String>) {
    let mut md = format!(
        "# {owner}/{repo} — Releases

"
    );
    let title;
    if data.is_array() {
        title = Some(format!("{owner}/{repo} releases"));
        let releases = data.as_array().unwrap();
        for r in releases.iter().take(5) {
            let tag = r["tag_name"].as_str().unwrap_or("unknown");
            let name = r["name"].as_str().unwrap_or(tag);
            let published = r["published_at"].as_str().unwrap_or("");
            let body = r["body"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(400)
                .collect::<String>();
            md.push_str(&format!(
                "## {name} ({tag})
"
            ));
            if !published.is_empty() {
                md.push_str(&format!(
                    "Published: {published}

"
                ));
            }
            if !body.is_empty() {
                md.push_str(&body);
                md.push_str(
                    "

",
                );
            }
        }
    } else {
        let tag = data["tag_name"].as_str().unwrap_or("unknown");
        let name = data["name"].as_str().unwrap_or(tag);
        let published = data["published_at"].as_str().unwrap_or("");
        let body = data["body"].as_str().unwrap_or("");
        title = Some(format!("{name} ({tag})"));
        md.push_str(&format!(
            "## {name} ({tag})
"
        ));
        if !published.is_empty() {
            md.push_str(&format!(
                "Published: {published}

"
            ));
        }
        if !body.is_empty() {
            md.push_str(body);
            md.push('\n');
        }
    }
    md.push_str(&format!("\n**GitHub:** {url}\n"));
    (md, title)
}
fn build_extra(owner: &str, repo: &str) -> serde_json::Value {
    build_git_payload(&GitPayload {
        provider: "github".to_string(),
        host: "github.com".to_string(),
        owner: Some(owner.to_string()),
        repo: repo.to_string(),
        content_kind: ContentKind::Release,
        ..Default::default()
    })
}

pub async fn extract(url: &str, ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
    let parsed = url::Url::parse(url).map_err(|_| VerticalError::VerticalUnsupportedUrl {
        vertical: INFO.name,
        url: url.to_string(),
    })?;
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() < 3 || segs[2] != "releases" {
        return Err(VerticalError::VerticalUnsupportedUrl {
            vertical: INFO.name,
            url: url.to_string(),
        });
    }

    let (owner, repo) = (segs[0], segs[1]);
    // /releases/tag/{tag} → single release; otherwise latest list
    let api_url = if segs.len() >= 5 && segs[3] == "tag" {
        let tag = segs[4];
        format!("https://api.github.com/repos/{owner}/{repo}/releases/tags/{tag}")
    } else {
        format!("https://api.github.com/repos/{owner}/{repo}/releases?per_page=10")
    };

    let client = http_client().map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status: 0,
    })?;

    let mut req = client
        .get(&api_url)
        .header("User-Agent", ctx.api_ua())
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");

    if let Ok(token) = std::env::var("GITHUB_TOKEN")
        && !token.is_empty()
    {
        req = req.header("Authorization", format!("Bearer {token}"));
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

    let (md, title) = format_release_markdown(owner, repo, url, &data);
    let extra = build_extra(owner, repo);

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title,
        extractor_name: INFO.name,
        extractor_version: 2,
        structured: Some(data),
        follow_crawl_urls: vec![],
        extra: Some(extra),
    })
}

#[cfg(test)]
#[path = "github_release_tests.rs"]
mod tests;
