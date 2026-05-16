//! GitHub Releases vertical extractor.
//!
//! Matches github.com/{owner}/{repo}/releases and /releases/tag/{tag}.
//! Fetches release metadata from the GitHub REST API.

use crate::core::http::http_client;
use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};

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

pub async fn extract(url: &str, _ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
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
        .header("User-Agent", format!("axon/{} (+https://github.com/jmagar/axon_rust)", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");

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

    let mut md = format!("# {owner}/{repo} — Releases\n\n");
    let title;

    if data.is_array() {
        title = Some(format!("{owner}/{repo} releases"));
        let releases = data.as_array().unwrap();
        for r in releases.iter().take(5) {
            let tag = r["tag_name"].as_str().unwrap_or("unknown");
            let name = r["name"].as_str().unwrap_or(tag);
            let published = r["published_at"].as_str().unwrap_or("");
            let body = r["body"].as_str().unwrap_or("").chars().take(400).collect::<String>();
            md.push_str(&format!("## {name} ({tag})\n"));
            if !published.is_empty() {
                md.push_str(&format!("Published: {published}\n\n"));
            }
            if !body.is_empty() {
                md.push_str(&body);
                md.push_str("\n\n");
            }
        }
    } else {
        let tag = data["tag_name"].as_str().unwrap_or("unknown");
        let name = data["name"].as_str().unwrap_or(tag);
        let published = data["published_at"].as_str().unwrap_or("");
        let body = data["body"].as_str().unwrap_or("");
        title = Some(format!("{name} ({tag})"));
        md.push_str(&format!("## {name} ({tag})\n"));
        if !published.is_empty() {
            md.push_str(&format!("Published: {published}\n\n"));
        }
        if !body.is_empty() {
            md.push_str(body);
            md.push('\n');
        }
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
