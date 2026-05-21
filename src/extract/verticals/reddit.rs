//! Reddit vertical extractor.
//!
//! Appends `.json` to any reddit URL to get structured post/listing data.
//! Uses OAuth client_credentials when REDDIT_CLIENT_ID + REDDIT_CLIENT_SECRET
//! are set; otherwise falls back to unauthenticated requests.
//! Reddit throttles requests without a proper User-Agent.
//!
//! ## Rate limiting
//! OAuth tokens are cached process-wide for their full lifetime (from the
//! `expires_in` field, default 24 h) so a crawl touching N reddit URLs issues
//! one token request, not N. Requests are spaced by a minimum inter-request
//! delay. On 429 the extractor retries up to MAX_RETRIES times, honouring the
//! `Retry-After` response header.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

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

/// Minimum delay between successive Reddit API requests (polite crawling).
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(500);

/// Maximum retry attempts on 429.
const MAX_RETRIES: u32 = 3;

/// Default wait on 429 when no Retry-After header is present.
const DEFAULT_RETRY_AFTER: Duration = Duration::from_secs(10);

// ── OAuth token cache ────────────────────────────────────────────────────────

struct CachedToken {
    token: String,
    expires_at: Instant,
}

struct RedditRateState {
    cached_token: Option<CachedToken>,
    last_request_at: Option<Instant>,
}

static RATE_STATE: OnceLock<Mutex<RedditRateState>> = OnceLock::new();

fn rate_state() -> &'static Mutex<RedditRateState> {
    RATE_STATE.get_or_init(|| {
        Mutex::new(RedditRateState {
            cached_token: None,
            last_request_at: None,
        })
    })
}

/// Return a valid OAuth Bearer token, reusing the cached one if not yet expired.
/// Returns None when credentials are absent or the token request fails.
async fn get_oauth_token() -> Option<String> {
    let id = std::env::var("REDDIT_CLIENT_ID")
        .ok()
        .filter(|s| !s.is_empty())?;
    let secret = std::env::var("REDDIT_CLIENT_SECRET")
        .ok()
        .filter(|s| !s.is_empty())?;

    // Check cache with a short-lived lock — drop the guard before any network I/O
    // to avoid holding the mutex across .await (serializes all Reddit calls on slow responses).
    {
        let state = rate_state().lock().await;
        if let Some(ref cached) = state.cached_token
            && cached.expires_at > Instant::now() + Duration::from_secs(60)
        {
            return Some(cached.token.clone());
        }
    } // lock released here before any async I/O

    // Fetch a fresh token (no lock held during network calls).
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
    let token = body["access_token"].as_str()?.to_string();
    let expires_in = body["expires_in"].as_u64().unwrap_or(86400);
    let expires_at = Instant::now() + Duration::from_secs(expires_in);

    // Re-acquire lock only to update the cache.
    {
        let mut state = rate_state().lock().await;
        state.cached_token = Some(CachedToken {
            token: token.clone(),
            expires_at,
        });
    }
    Some(token)
}

/// Enforce the minimum inter-request delay, then record the request time.
async fn wait_for_rate_slot() {
    let mut state = rate_state().lock().await;
    if let Some(last) = state.last_request_at {
        let elapsed = last.elapsed();
        if elapsed < MIN_REQUEST_INTERVAL {
            let wait = MIN_REQUEST_INTERVAL - elapsed;
            drop(state);
            tokio::time::sleep(wait).await;
            state = rate_state().lock().await;
        }
    }
    state.last_request_at = Some(Instant::now());
}

// ── URL matching ─────────────────────────────────────────────────────────────

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

// ── Extraction ───────────────────────────────────────────────────────────────

pub async fn extract(url: &str, _ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
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

    let token = get_oauth_token().await;

    let data = fetch_with_retry(client, &json_url, token.as_deref(), url).await?;

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
    let num_comments = post_data["num_comments"].as_u64().unwrap_or(0);

    let mut md = format!("# {}\n\n", title.as_deref().unwrap_or("Reddit post"));
    if !subreddit.is_empty() {
        md.push_str(&format!(
            "**{subreddit}** by u/{author} | Score: {score} | Comments: {num_comments}\n\n"
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
        extractor_version: 2,
        structured: Some(data),
        follow_crawl_urls: vec![],
        extra: None,
    })
}

async fn fetch_with_retry(
    client: &reqwest::Client,
    json_url: &str,
    token: Option<&str>,
    original_url: &str,
) -> Result<serde_json::Value, VerticalError> {
    for attempt in 0..=MAX_RETRIES {
        wait_for_rate_slot().await;

        let mut req = client.get(json_url).header("User-Agent", REDDIT_UA);
        if let Some(t) = token {
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
            200 => {
                return resp
                    .json()
                    .await
                    .map_err(|_| VerticalError::VerticalTargetUnavailable {
                        vertical: INFO.name,
                        status,
                    });
            }
            429 => {
                if attempt == MAX_RETRIES {
                    let retry_after = parse_retry_after(&resp);
                    return Err(VerticalError::VerticalRateLimited {
                        vertical: INFO.name,
                        retry_after,
                    });
                }
                let wait = parse_retry_after(&resp).unwrap_or(DEFAULT_RETRY_AFTER);
                tokio::time::sleep(wait).await;
            }
            403 | 404 => {
                return Err(VerticalError::VerticalTargetNotFound {
                    vertical: INFO.name,
                    url: original_url.to_string(),
                });
            }
            _ => {
                return Err(VerticalError::VerticalTargetUnavailable {
                    vertical: INFO.name,
                    status,
                });
            }
        }
    }
    unreachable!()
}

fn parse_retry_after(resp: &reqwest::Response) -> Option<Duration> {
    let secs = resp
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())?;
    Some(Duration::from_secs(secs.min(120)))
}
