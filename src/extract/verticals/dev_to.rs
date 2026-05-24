//! dev.to article vertical extractor.
//!
//! Matches dev.to/{username}/{slug} and fetches article data from the
//! dev.to API. No authentication required for public articles.

use crate::core::http::http_client;
use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};

#[cfg(test)]
#[path = "dev_to_tests.rs"]
mod tests;

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "dev_to",
    label: "DEV Community Article",
    description: "Fetches article metadata and body from dev.to API — title, tags, reactions, reading time, full article body.",
    url_patterns: &["https://dev.to/{username}/{slug}"],
    auto_dispatch: true,
};

fn build_extra(
    username: &str,
    tags: &[&str],
    reactions: u64,
    reading_time_mins: u64,
    published_at: &str,
) -> serde_json::Value {
    let mut obj = serde_json::json!({
        "devto_author": username,
        "devto_reactions": reactions,
        "devto_reading_time_mins": reading_time_mins,
    });
    if !tags.is_empty() {
        obj["devto_tags"] = serde_json::json!(tags);
    }
    if !published_at.is_empty() {
        obj["devto_published_at"] = serde_json::Value::String(published_at.to_string());
    }
    obj
}

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
    !matches!(
        segs[0],
        "t" | "p" | "search" | "dashboard" | "admin" | "settings" | "enter"
    )
}

async fn get_json(
    client: &reqwest::Client,
    url: &str,
    ctx: &VerticalContext,
) -> Result<serde_json::Value, VerticalError> {
    let resp = client
        .get(url)
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
        404 => Err(VerticalError::VerticalTargetNotFound {
            vertical: INFO.name,
            url: url.to_string(),
        }),
        429 => Err(VerticalError::VerticalRateLimited {
            vertical: INFO.name,
            retry_after: None,
        }),
        200 => resp
            .json()
            .await
            .map_err(|_| VerticalError::VerticalTargetUnavailable {
                vertical: INFO.name,
                status,
            }),
        _ => Err(VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status,
        }),
    }
}

fn article_detail_api_url(article_id: u64) -> String {
    format!("https://dev.to/api/articles/{article_id}")
}

fn select_article_body(data: &serde_json::Value) -> &str {
    let body_markdown = data["body_markdown"].as_str().unwrap_or("");
    if !body_markdown.is_empty() {
        body_markdown
    } else {
        data["description"].as_str().unwrap_or("")
    }
}

pub async fn extract(url: &str, ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
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

    let articles = get_json(client, &api_url, ctx).await?;

    // Find the article matching our slug
    let article = articles
        .as_array()
        .and_then(|arr| {
            arr.iter()
                .find(|a| a["slug"].as_str().map(|s| s == slug).unwrap_or(false))
        })
        .cloned();

    let data = match article {
        Some(a) => a,
        None => {
            return Err(VerticalError::VerticalTargetNotFound {
                vertical: INFO.name,
                url: url.to_string(),
            });
        }
    };
    let article_id = data["id"]
        .as_u64()
        .ok_or(VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status: 0,
        })?;
    let detail_api_url = article_detail_api_url(article_id);
    let data = get_json(client, &detail_api_url, ctx).await?;
    tracing::debug!(
        article_id,
        body_markdown_len = data["body_markdown"].as_str().map(str::len).unwrap_or(0),
        description_len = data["description"].as_str().map(str::len).unwrap_or(0),
        "dev_to.detail_fetched"
    );

    let title_str = data["title"].as_str().unwrap_or("Unknown article");
    let reading_time = data["reading_time_minutes"].as_u64().unwrap_or(0);
    let reactions = data["positive_reactions_count"].as_u64().unwrap_or(0);
    let tags: Vec<&str> = data["tag_list"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    // The author listing endpoint only includes `description`; the per-article
    // endpoint includes the full `body_markdown`.
    let body = select_article_body(&data);

    let published_at = data["published_at"].as_str().unwrap_or("");
    let extra = build_extra(username, &tags, reactions, reading_time, published_at);

    let title = Some(title_str.to_string());
    let mut md = format!("# {title_str}\n\n");
    md.push_str(&format!("**Author:** {username} | **Reading time:** {reading_time} min | **Reactions:** {reactions}\n\n"));
    if !tags.is_empty() {
        md.push_str(&format!("**Tags:** {}\n\n", tags.join(", ")));
    }
    if !body.is_empty() {
        md.push_str(body);
        md.push('\n');
    }
    md.push_str(&format!("\n**DEV:** {url}\n"));

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
