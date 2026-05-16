//! HuggingFace model vertical extractor.
//!
//! Matches huggingface.co/{org}/{model} (2-segment path, not datasets/ or spaces/).
//! Uses the HuggingFace Hub API. Optional HF_TOKEN env var for higher rate limits.

use crate::core::http::http_client;
use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "huggingface_model",
    label: "HuggingFace Model",
    description: "Fetches model metadata from huggingface.co/api/models — downloads, likes, tasks, architecture.",
    url_patterns: &["https://huggingface.co/{org}/{model}"],
    auto_dispatch: true,
};

/// Top-level namespace paths that are NOT model repos.
const RESERVED_NAMESPACES: &[&str] = &[
    "datasets", "spaces", "blog", "docs", "learn", "tasks",
    "models", "transformers", "huggingface", "pricing", "enterprise",
    "join", "login", "settings", "organizations",
];

pub fn matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if host != "huggingface.co" {
        return false;
    }
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segs.len() != 2 {
        return false;
    }
    let ns = segs[0].to_lowercase();
    !RESERVED_NAMESPACES.contains(&ns.as_str())
}

pub async fn extract(url: &str, _ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
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

    let (org, model) = (segs[0], segs[1]);
    let model_id = format!("{org}/{model}");
    let api_url = format!("https://huggingface.co/api/models/{model_id}");

    let client = http_client().map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status: 0,
    })?;

    let mut req = client
        .get(&api_url)
        .header("User-Agent", format!("axon/{} (+https://github.com/jmagar/axon_rust)", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/json");

    // Optional HF_TOKEN for higher rate limits
    if let Ok(token) = std::env::var("HF_TOKEN") {
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

    let id = data["id"].as_str().unwrap_or(&model_id);
    let downloads = data["downloads"].as_u64().unwrap_or(0);
    let likes = data["likes"].as_u64().unwrap_or(0);
    let pipeline_tag = data["pipeline_tag"].as_str().unwrap_or("");
    let library_name = data["library_name"].as_str().unwrap_or("");
    let tags: Vec<&str> = data["tags"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let title = Some(id.to_string());
    let mut md = format!("# {id}\n\n");
    if !pipeline_tag.is_empty() {
        md.push_str(&format!("**Task:** {pipeline_tag}\n"));
    }
    if !library_name.is_empty() {
        md.push_str(&format!("**Library:** {library_name}\n"));
    }
    md.push_str(&format!("**Downloads:** {downloads} | **Likes:** {likes}\n"));
    if !tags.is_empty() {
        let relevant: Vec<&str> = tags.iter().take(10).copied().collect();
        md.push_str(&format!("\n**Tags:** {}\n", relevant.join(", ")));
    }
    md.push_str(&format!("\n**HuggingFace:** {url}\n"));

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title,
        extractor_name: INFO.name,
        extractor_version: 1,
        structured: Some(data),
    })
}
