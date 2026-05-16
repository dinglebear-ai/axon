//! crates.io package vertical extractor.
//!
//! Matches crates.io/crates/{name} or /crates/{name}/{version}.
//! crates.io hard-fails (HTTP 403) on empty User-Agent — always set it.

use crate::core::http::http_client;
use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "crates_io",
    label: "crates.io Crate",
    description: "Fetches crate metadata from crates.io/api/v1/crates — version, description, downloads, license.",
    url_patterns: &[
        "https://crates.io/crates/{name}",
        "https://crates.io/crates/{name}/{version}",
    ],
    auto_dispatch: true,
};

const CRATES_UA: &str = concat!(
    "axon/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/jmagar/axon_rust)"
);

pub fn matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if host != "crates.io" {
        return false;
    }
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    segs.len() >= 2 && segs[0] == "crates"
}

pub async fn extract(url: &str, _ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
    let parsed = url::Url::parse(url).map_err(|_| VerticalError::VerticalUnsupportedUrl {
        vertical: INFO.name,
        url: url.to_string(),
    })?;
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() < 2 || segs[0] != "crates" {
        return Err(VerticalError::VerticalUnsupportedUrl {
            vertical: INFO.name,
            url: url.to_string(),
        });
    }

    let name = segs[1];
    let api_url = format!("https://crates.io/api/v1/crates/{name}");

    let client = http_client().map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status: 0,
    })?;

    let resp = client
        .get(&api_url)
        .header("User-Agent", CRATES_UA)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|_| VerticalError::VerticalTargetUnavailable { vertical: INFO.name, status: 0 })?;

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

    let krate = &data["crate"];
    let crate_name = krate["name"].as_str().unwrap_or(name);
    let max_version = krate["max_stable_version"]
        .as_str()
        .or_else(|| krate["newest_version"].as_str())
        .unwrap_or("unknown");
    let description = krate["description"].as_str().unwrap_or("");
    let downloads = krate["downloads"].as_u64().unwrap_or(0);
    let license = data["versions"][0]["license"].as_str().unwrap_or("");
    let repository = krate["repository"].as_str().unwrap_or("");
    let homepage = krate["homepage"].as_str().unwrap_or("");

    let title = Some(format!("{crate_name} {max_version}"));
    let mut md = format!("# {crate_name} {max_version}\n\n");
    if !description.is_empty() {
        md.push_str(description);
        md.push_str("\n\n");
    }
    md.push_str(&format!("**Downloads:** {downloads}\n"));
    if !license.is_empty() {
        md.push_str(&format!("**License:** {license}\n"));
    }
    if !repository.is_empty() {
        md.push_str(&format!("**Repository:** {repository}\n"));
    }
    if !homepage.is_empty() {
        md.push_str(&format!("**Homepage:** {homepage}\n"));
    }
    md.push_str(&format!("\n**crates.io:** {url}\n"));

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title,
        extractor_name: INFO.name,
        extractor_version: 1,
        structured: Some(data),
    })
}
