//! dev.to article vertical extractor.
//!
//! Matches dev.to/{username}/{slug} and fetches article data from the
//! dev.to API. No authentication required for public articles.

use crate::core::http::http_client;
use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "dev_to",
    label: "DEV Community Article",
    description: "Fetches article metadata from dev.to API — title, tags, reactions, reading time.",
    url_patterns: &["https://dev.to/{username}/{slug}"],
    auto_dispatch: true,
};

pub fn matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if host != "dev.to" {
        return false;
    }
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    // Exactly 2 segments: {username}/{slug}
    // Reject pure-numeric first segment (tag pages look like /t/tag)
    if segs.len() != 2 {
        return false;
    }
    // Reject known non-article paths
    !matches!(segs[0], "t" | "p" | "search" | "dashboard" | "admin" | "settings" | "enter")
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

    let (username, slug) = (segs[0], segs[1]);
    // Try the article by path API (returns array filtered by username+slug)
    let api_url = format!("https://dev.to/api/articles?username={username}&per_page=100");

    let client = http_client().map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status: 0,
    })?;

    let resp = client
        .get(&api_url)
        .header("User-Agent", format!("axon/{} (+https://github.com/jmagar/axon_rust)", env!("CARGO_PKG_VERSION")))
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

    let articles: serde_json::Value = resp.json().await.map_err(|_| {
        VerticalError::VerticalTargetUnavailable { vertical: INFO.name, status }
    })?;

    // Find the article matching our slug
    let article = articles
        .as_array()
        .and_then(|arr| {
            arr.iter().find(|a| {
                a["slug"].as_str().map(|s| s == slug).unwrap_or(false)
            })
        })
        .cloned();

    let data = match article {
        Some(a) => a,
        None => return Err(VerticalError::VerticalTargetNotFound {
            vertical: INFO.name,
            url: url.to_string(),
        }),
    };

    let title_str = data["title"].as_str().unwrap_or("Unknown article");
    let description = data["description"].as_str().unwrap_or("");
    let reading_time = data["reading_time_minutes"].as_u64().unwrap_or(0);
    let reactions = data["positive_reactions_count"].as_u64().unwrap_or(0);
    let tags: Vec<&str> = data["tag_list"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let title = Some(title_str.to_string());
    let mut md = format!("# {title_str}\n\n");
    md.push_str(&format!("**Author:** {username} | **Reading time:** {reading_time} min | **Reactions:** {reactions}\n\n"));
    if !description.is_empty() {
        md.push_str(description);
        md.push('\n');
    }
    if !tags.is_empty() {
        md.push_str(&format!("\n**Tags:** {}\n", tags.join(", ")));
    }
    md.push_str(&format!("\n**DEV:** {url}\n"));

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title,
        extractor_name: INFO.name,
        extractor_version: 1,
        structured: Some(data),
    })
}
