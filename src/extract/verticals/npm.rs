//! npm package vertical extractor.
//!
//! Matches npmjs.com/package/{name} (including scoped @scope/name packages)
//! and fetches metadata from the npm registry API. No authentication required.

use crate::core::http::http_client;
use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "npm",
    label: "npm Package",
    description: "Fetches package metadata from registry.npmjs.org — version, description, author, license, dist-tags.",
    url_patterns: &[
        "https://npmjs.com/package/{name}",
        "https://npmjs.com/package/@{scope}/{name}",
    ],
    auto_dispatch: true,
};

pub fn matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if host != "npmjs.com" && host != "www.npmjs.com" {
        return false;
    }
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    // /package/{name}  OR  /package/@scope/name
    segs.len() >= 2 && segs[0] == "package"
}

pub async fn extract(url: &str, _ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
    let parsed = url::Url::parse(url).map_err(|_| VerticalError::VerticalUnsupportedUrl {
        vertical: INFO.name,
        url: url.to_string(),
    })?;
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() < 2 || segs[0] != "package" {
        return Err(VerticalError::VerticalUnsupportedUrl {
            vertical: INFO.name,
            url: url.to_string(),
        });
    }

    // Reconstruct package name: supports scoped @scope/name
    let pkg_name = if segs.len() >= 3 && segs[1].starts_with('@') {
        format!("{}/{}", segs[1], segs[2])
    } else {
        segs[1].to_string()
    };

    // Encode scoped packages for URL: @scope/name → @scope%2Fname
    let encoded_name = pkg_name.replace('/', "%2F");
    let api_url = format!("https://registry.npmjs.org/{encoded_name}");

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

    let name = data["name"].as_str().unwrap_or(&pkg_name);
    let description = data["description"].as_str().unwrap_or("");
    let latest_version = data["dist-tags"]["latest"].as_str().unwrap_or("unknown");
    let license = data["license"].as_str().unwrap_or("");
    let homepage = data["homepage"].as_str().unwrap_or("");

    // Author can be a string or an object
    let author = if let Some(s) = data["author"].as_str() {
        s.to_string()
    } else if let Some(obj) = data["author"].as_object() {
        obj.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    } else {
        String::new()
    };

    let title = Some(format!("{name}@{latest_version}"));
    let mut md = format!("# {name}@{latest_version}\n\n");
    if !description.is_empty() {
        md.push_str(description);
        md.push_str("\n\n");
    }
    if !author.is_empty() {
        md.push_str(&format!("**Author:** {author}\n"));
    }
    if !license.is_empty() {
        md.push_str(&format!("**License:** {license}\n"));
    }
    if !homepage.is_empty() {
        md.push_str(&format!("**Homepage:** {homepage}\n"));
    }
    md.push_str(&format!("\n**npm:** {url}\n"));

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title,
        extractor_name: INFO.name,
        extractor_version: 1,
        structured: Some(data),
    })
}
