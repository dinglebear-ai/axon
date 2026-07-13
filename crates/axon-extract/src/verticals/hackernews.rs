//! Hacker News vertical extractor via Algolia HN API.
//!
//! Matches news.ycombinator.com/item?id=N and hn.algolia.com/items/N.
//! Returns the post + top-level comments formatted as markdown.
//!
//! auto_dispatch: true

use crate::context::VerticalContext;
use crate::error::VerticalError;
use crate::types::{ExtractorInfo, ScrapedDoc};
use axon_core::http::http_client;

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "hackernews",
    label: "Hacker News Post",
    description: "Fetches HN post + top comments from the Algolia HN API.",
    url_patterns: &[
        "https://news.ycombinator.com/item?id={id}",
        "https://hn.algolia.com/items/{id}",
    ],
    auto_dispatch: true,
};

pub fn matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if host == "news.ycombinator.com" {
        return parsed.path() == "/item" && parsed.query_pairs().any(|(k, _)| k == "id");
    }
    if host == "hn.algolia.com" {
        let segs: Vec<&str> = parsed
            .path()
            .trim_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        // Validate that the ID segment is numeric to avoid claiming invalid URLs
        return segs.len() == 2
            && segs[0] == "items"
            && segs[1].chars().all(|c| c.is_ascii_digit());
    }
    false
}

/// A single HN item (post or comment), with recursive children.
#[derive(Debug, serde::Deserialize)]
struct HnItem {
    #[allow(dead_code)]
    id: Option<u64>,
    #[serde(rename = "type")]
    item_type: Option<String>,
    title: Option<String>,
    url: Option<String>,
    author: Option<String>,
    points: Option<u64>,
    text: Option<String>,
    created_at: Option<String>,
    #[serde(default)]
    children: Vec<HnItem>,
}

fn extract_item_id(url: &str) -> Option<u64> {
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if host == "news.ycombinator.com" {
        for (k, v) in parsed.query_pairs() {
            if k == "id" {
                return v.parse::<u64>().ok();
            }
        }
    }
    if host == "hn.algolia.com" {
        let segs: Vec<&str> = parsed
            .path()
            .trim_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        if segs.len() == 2 {
            return segs[1].parse::<u64>().ok();
        }
    }
    None
}

fn count_comments(item: &HnItem) -> usize {
    item.children.iter().map(|c| 1 + count_comments(c)).sum()
}

fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => result.push(c),
            _ => {}
        }
    }
    // Decode basic HTML entities
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&apos;", "'")
}

pub async fn extract(url: &str, ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
    let item_id = extract_item_id(url).ok_or(VerticalError::VerticalUnsupportedUrl {
        vertical: INFO.name,
        url: url.to_string(),
    })?;

    let api_url = format!("https://hn.algolia.com/api/v1/items/{item_id}");
    let client = http_client().map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status: 0,
    })?;

    let resp = client
        .get(&api_url)
        .header("User-Agent", ctx.api_ua())
        .send()
        .await
        .map_err(|_| VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status: 0,
        })?;

    let status = resp.status().as_u16();
    if status == 404 {
        return Err(VerticalError::VerticalTargetNotFound {
            vertical: INFO.name,
            url: url.to_string(),
        });
    }
    if status != 200 {
        return Err(VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status,
        });
    }

    let item: HnItem = resp
        .json()
        .await
        .map_err(|_| VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status,
        })?;

    build_scraped_doc(url, item_id, &item)
}

fn infer_hn_type(item_type: Option<&str>, title: &str) -> &'static str {
    if item_type == Some("job") {
        return "job";
    }
    if title.starts_with("Ask HN:") {
        return "ask_hn";
    }
    if title.starts_with("Show HN:") {
        return "show_hn";
    }
    "story"
}

fn build_extra(
    item_id: u64,
    hn_type: &str,
    author: &str,
    points: u64,
    comment_count: u64,
    created_at: &str,
    external_url: Option<&str>,
) -> serde_json::Value {
    let mut obj = serde_json::json!({ "hn_id": item_id, "hn_type": hn_type, "hn_author": author, "hn_points": points, "hn_comment_count": comment_count });
    if !created_at.is_empty() {
        obj["hn_created_at"] = serde_json::Value::String(created_at.to_string());
    }
    if let Some(u) = external_url {
        obj["hn_external_url"] = serde_json::Value::String(u.to_string());
    }
    obj
}

fn build_scraped_doc(url: &str, item_id: u64, item: &HnItem) -> Result<ScrapedDoc, VerticalError> {
    let title = item
        .title
        .clone()
        .unwrap_or_else(|| format!("HN Item {item_id}"));
    let author = item.author.as_deref().unwrap_or("unknown");
    let points = item.points.unwrap_or(0);
    let comment_count = count_comments(item);
    let created_at = item.created_at.as_deref().unwrap_or("");
    let hn_type = infer_hn_type(item.item_type.as_deref(), &title);
    let extra = build_extra(
        item_id,
        hn_type,
        author,
        points,
        comment_count as u64,
        created_at,
        item.url.as_deref(),
    );
    let hn_url = format!("https://news.ycombinator.com/item?id={item_id}");

    let mut md = format!("# {title}\n\n");
    md.push_str(&format!(
        "**By:** {author} | **Points:** {points} | **Comments:** {comment_count} | **Posted:** {created_at}\n"
    ));

    // Show post body for Ask/Show/Tell HN posts
    if let Some(text) = &item.text
        && !text.is_empty()
    {
        let clean = strip_html_tags(text);
        md.push('\n');
        md.push_str(&clean);
        md.push('\n');
    }

    // Show external link for story posts
    if let Some(ext_url) = &item.url {
        md.push_str(&format!("\n**External URL:** {ext_url}\n"));
    }
    md.push_str(&format!("**HN:** {hn_url}\n"));

    // Top comments (bounded to 10 top-level)
    let top_comments: Vec<&HnItem> = item
        .children
        .iter()
        .filter(|c| c.text.is_some() || c.author.is_some())
        .take(10)
        .collect();

    if !top_comments.is_empty() {
        md.push_str("\n## Top Comments\n\n");
        for comment in top_comments {
            let c_author = comment.author.as_deref().unwrap_or("unknown");
            let c_text = comment
                .text
                .as_deref()
                .map(strip_html_tags)
                .unwrap_or_default();
            let c_excerpt: String = c_text.chars().take(300).collect();
            let ellipsis = if c_text.len() > 300 { "…" } else { "" };
            md.push_str(&format!("**{c_author}:** {c_excerpt}{ellipsis}\n\n"));
        }
    }

    let structured = serde_json::json!({
        "id": item_id,
        "type": item.item_type,
        "title": title,
        "author": author,
        "points": points,
        "comment_count": comment_count,
        "created_at": created_at,
        "url": item.url,
    });

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title: Some(title),
        extractor_name: INFO.name,
        extractor_version: 2,
        structured: Some(structured),
        follow_crawl_urls: vec![],
        extra: Some(extra),
    })
}

#[cfg(test)]
#[path = "hackernews_tests.rs"]
mod tests;
