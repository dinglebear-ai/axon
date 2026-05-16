//! Docker Hub image vertical extractor.
//!
//! Matches hub.docker.com/r/{namespace}/{repo} (community images) and
//! hub.docker.com/_/{repo} (official images). Uses the Docker Hub v2 API.

use crate::core::http::http_client;
use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "docker_hub",
    label: "Docker Hub Image",
    description: "Fetches image metadata from hub.docker.com/v2/repositories — pulls, stars, description, tags.",
    url_patterns: &[
        "https://hub.docker.com/r/{namespace}/{repo}",
        "https://hub.docker.com/_/{repo}",
    ],
    auto_dispatch: true,
};

pub fn matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if host != "hub.docker.com" {
        return false;
    }
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    // /r/{namespace}/{repo} or /_/{repo}
    if segs.len() < 2 {
        return false;
    }
    segs[0] == "r" || segs[0] == "_"
}

pub async fn extract(url: &str, _ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
    let parsed = url::Url::parse(url).map_err(|_| VerticalError::VerticalUnsupportedUrl {
        vertical: INFO.name,
        url: url.to_string(),
    })?;
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    let (namespace, repo) = if segs.len() >= 3 && segs[0] == "r" {
        (segs[1], segs[2])
    } else if segs.len() >= 2 && segs[0] == "_" {
        ("library", segs[1])
    } else {
        return Err(VerticalError::VerticalUnsupportedUrl {
            vertical: INFO.name,
            url: url.to_string(),
        });
    };

    let api_url = format!("https://hub.docker.com/v2/repositories/{namespace}/{repo}");

    let client = http_client().map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status: 0,
    })?;

    let resp = client
        .get(&api_url)
        .header(
            "User-Agent",
            format!(
                "axon/{} (+https://github.com/jmagar/axon_rust)",
                env!("CARGO_PKG_VERSION")
            ),
        )
        .header("Accept", "application/json")
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

    let img_name = data["name"].as_str().unwrap_or(repo);
    let full_name = data["full_name"].as_str().unwrap_or(img_name);
    let description = data["description"].as_str().unwrap_or("");
    let full_description = data["full_description"].as_str().unwrap_or("");
    let pull_count = data["pull_count"].as_u64().unwrap_or(0);
    let star_count = data["star_count"].as_u64().unwrap_or(0);
    let is_official = data["is_official"].as_bool().unwrap_or(false);

    let title = Some(full_name.to_string());
    let mut md = format!("# {full_name}\n\n");
    if is_official {
        md.push_str("**Official Image**\n\n");
    }
    if !description.is_empty() {
        md.push_str(description);
        md.push_str("\n\n");
    }
    md.push_str(&format!(
        "**Pulls:** {pull_count} | **Stars:** {star_count}\n"
    ));
    if !full_description.is_empty() {
        let excerpt: String = full_description.chars().take(500).collect();
        md.push_str(&format!("\n{excerpt}\n"));
    }
    md.push_str(&format!("\n**Docker Hub:** {url}\n"));

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title,
        extractor_name: INFO.name,
        extractor_version: 1,
        structured: Some(data),
    })
}
