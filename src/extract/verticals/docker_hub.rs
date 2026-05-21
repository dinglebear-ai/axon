//! Docker Hub image vertical extractor.
//!
//! Matches hub.docker.com/r/{namespace}/{repo} (community images) and
//! hub.docker.com/_/{repo} (official images). Uses the Docker Hub v2 API.

use crate::core::http::http_client;
use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};

#[cfg(test)]
#[path = "docker_hub_tests.rs"]
mod tests;

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "docker_hub",
    label: "Docker Hub Image",
    description: "Fetches image metadata from hub.docker.com/v2/repositories — pulls, stars, description, full description, last updated.",
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

fn build_extra(
    namespace: &str,
    img_name: &str,
    full_name: &str,
    pull_count: u64,
    star_count: u64,
    is_official: bool,
    last_updated: &str,
) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "docker_namespace": namespace,
        "docker_image": img_name,
        "docker_full_name": full_name,
        "docker_pulls": pull_count,
        "docker_stars": star_count,
        "docker_is_official": is_official,
    });
    if !last_updated.is_empty() {
        obj["docker_last_updated"] = serde_json::Value::String(last_updated.to_string());
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
        .header("User-Agent", ctx.api_ua())
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
    let last_updated = data["last_updated"].as_str().unwrap_or("");

    let extra = build_extra(
        namespace,
        img_name,
        full_name,
        pull_count,
        star_count,
        is_official,
        last_updated,
    );

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
    if !last_updated.is_empty() {
        md.push_str(&format!("**Last Updated:** {last_updated}\n"));
    }
    if !full_description.is_empty() {
        md.push_str("\n## Full Description\n\n");
        md.push_str(full_description);
        md.push('\n');
    }
    md.push_str(&format!("\n**Docker Hub:** {url}\n"));

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title,
        extractor_name: INFO.name,
        extractor_version: 3,
        structured: Some(data),
        follow_crawl_urls: vec![],
        extra: Some(extra),
    })
}
