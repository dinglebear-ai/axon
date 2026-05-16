//! Reddit vertical extractor.
//!
//! Appends `.json` to any reddit URL to get structured post/listing data.
//! Uses OAuth client_credentials when REDDIT_CLIENT_ID + REDDIT_CLIENT_SECRET
//! are set; otherwise falls back to unauthenticated requests.
//! Reddit throttles requests without a proper User-Agent.

use crate::core::http::http_client;
use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "reddit",
    label: "Reddit Post / Subreddit",
    description: "Fetches post or listing data from Reddit's public JSON API (.json suffix).",
    url_patterns: &[
        "https://reddit.com/r/{sub}/comments/{id}/*",
        "https://reddit.com/r/{sub}/*",
        "https://old.reddit.com/*",
    ],
    auto_dispatch: true,
};

const REDDIT_UA: &str = concat!(
    "rust:io.github.jmagar.axon:",
    env!("CARGO_PKG_VERSION"),
    " (by /u/jmagar)"
);

pub fn matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    matches!(
        host.as_str(),
        "reddit.com" | "www.reddit.com" | "old.reddit.com" | "m.reddit.com" | "np.reddit.com"
    )
}

/// Try OAuth client_credentials; return the Bearer token on success.
async fn maybe_oauth_token() -> Option<String> {
    let id = std::env::var("REDDIT_CLIENT_ID")
        .ok()
        .filter(|s| !s.is_empty())?;
    let secret = std::env::var("REDDIT_CLIENT_SECRET")
        .ok()
        .filter(|s| !s.is_empty())?;
    let client = http_client().ok()?;
    let resp = client
        .post("https://www.reddit.com/api/v1/access_token")
        .basic_auth(&id, Some(&secret))
        .header("User-Agent", REDDIT_UA)
        .form(&[("grant_type", "client_credentials")])
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: serde_json::Value = resp.json().await.ok()?;
    body["access_token"].as_str().map(str::to_string)
}

pub async fn extract(url: &str, _ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
    // Strip fragment, then append .json
    let Ok(mut parsed) = url::Url::parse(url) else {
        return Err(VerticalError::VerticalUnsupportedUrl {
            vertical: INFO.name,
            url: url.to_string(),
        });
    };
    parsed.set_fragment(None);
    let base = parsed.as_str().trim_end_matches('/');
    let json_url = format!("{base}.json");

    let client = http_client().map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status: 0,
    })?;

    let token = maybe_oauth_token().await;
    let mut req = client.get(&json_url).header("User-Agent", REDDIT_UA);

    if let Some(ref t) = token {
        req = req.header("Authorization", format!("Bearer {t}"));
    }

    let resp = req
        .send()
        .await
        .map_err(|_| VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status: 0,
        })?;

    let status = resp.status().as_u16();
    match status {
        429 => {
            return Err(VerticalError::VerticalRateLimited {
                vertical: INFO.name,
                retry_after: None,
            });
        }
        403 | 404 => {
            return Err(VerticalError::VerticalTargetNotFound {
                vertical: INFO.name,
                url: url.to_string(),
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

    // Reddit returns either a listing or [post_listing, comment_listing]
    let post_data = if data.is_array() {
        data[0]["data"]["children"][0]["data"].clone()
    } else {
        data["data"]["children"][0]["data"].clone()
    };

    let title = post_data["title"].as_str().map(str::to_string).or_else(|| {
        post_data["subreddit_name_prefixed"]
            .as_str()
            .map(str::to_string)
    });
    let selftext = post_data["selftext"].as_str().unwrap_or("").to_string();
    let author = post_data["author"].as_str().unwrap_or("[deleted]");
    let score = post_data["score"].as_i64().unwrap_or(0);
    let subreddit = post_data["subreddit_name_prefixed"].as_str().unwrap_or("");

    let mut md = format!("# {}\n\n", title.as_deref().unwrap_or("Reddit post"));
    if !subreddit.is_empty() {
        md.push_str(&format!(
            "**{subreddit}** by u/{author} | score: {score}\n\n"
        ));
    }
    if !selftext.is_empty() {
        md.push_str(selftext.trim());
        md.push('\n');
    }
    md.push_str(&format!("\n**Source:** {url}\n"));

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title,
        extractor_name: INFO.name,
        extractor_version: 1,
        structured: Some(data),
    })
}
