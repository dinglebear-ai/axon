//! PyPI package vertical extractor.
//!
//! Matches pypi.org/project/{name} or /project/{name}/{version} and fetches
//! metadata from the PyPI JSON API. No authentication required.

use crate::core::http::http_client;
use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "pypi",
    label: "PyPI Package",
    description: "Fetches package metadata from pypi.org/pypi/{name}/json — version, description, classifiers, links.",
    url_patterns: &[
        "https://pypi.org/project/{name}",
        "https://pypi.org/project/{name}/{version}",
    ],
    auto_dispatch: true,
};

pub fn matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if host != "pypi.org" {
        return false;
    }
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    // /project/{name}  OR  /project/{name}/{version}
    segs.len() >= 2 && segs[0] == "project"
}

pub async fn extract(url: &str, _ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
    let parsed = url::Url::parse(url).map_err(|_| VerticalError::VerticalUnsupportedUrl {
        vertical: INFO.name,
        url: url.to_string(),
    })?;
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() < 2 || segs[0] != "project" {
        return Err(VerticalError::VerticalUnsupportedUrl {
            vertical: INFO.name,
            url: url.to_string(),
        });
    }

    let name = segs[1];
    let api_url = if segs.len() >= 3 {
        let version = segs[2];
        format!("https://pypi.org/pypi/{name}/{version}/json")
    } else {
        format!("https://pypi.org/pypi/{name}/json")
    };

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

    let info = &data["info"];
    let pkg_name = info["name"].as_str().unwrap_or(name);
    let version = info["version"].as_str().unwrap_or("unknown");
    let summary = info["summary"].as_str().unwrap_or("");
    let author = info["author"].as_str().unwrap_or("");
    let license = info["license"].as_str().unwrap_or("");
    let home_page = info["home_page"].as_str().unwrap_or("");
    let requires_python = info["requires_python"].as_str().unwrap_or("");

    let title = Some(format!("{pkg_name} {version}"));
    let mut md = format!("# {pkg_name} {version}\n\n");
    if !summary.is_empty() {
        md.push_str(summary);
        md.push_str("\n\n");
    }
    if !author.is_empty() {
        md.push_str(&format!("**Author:** {author}\n"));
    }
    if !license.is_empty() {
        md.push_str(&format!("**License:** {license}\n"));
    }
    if !requires_python.is_empty() {
        md.push_str(&format!("**Requires Python:** {requires_python}\n"));
    }
    if !home_page.is_empty() {
        md.push_str(&format!("**Homepage:** {home_page}\n"));
    }
    md.push_str(&format!("\n**PyPI:** {url}\n"));

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title,
        extractor_name: INFO.name,
        extractor_version: 1,
        structured: Some(data),
    })
}
