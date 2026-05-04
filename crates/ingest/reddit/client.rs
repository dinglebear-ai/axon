use crate::crates::core::logging::log_warn;
use reqwest::Client;
use reqwest::header::{HeaderMap, RETRY_AFTER};
use std::error::Error;
use std::sync::LazyLock;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Reddit requires a descriptive User-Agent for API access.
/// Format: <platform>:<app id>:<version> (by /u/<username>)
pub(super) const REDDIT_USER_AGENT: &str = "axon-ingest/1.0 by /u/axon_bot";

/// Shared HTTP client for all Reddit API requests.
///
/// A single connection pool is reused across `get_access_token` and every
/// subsequent API call — avoids spawning a new pool per ingest invocation.
static REDDIT_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .user_agent(REDDIT_USER_AGENT)
        .https_only(true)
        .timeout(Duration::from_secs(30))
        .build()
        .expect("failed to build Reddit HTTP client")
});

const MAX_REDDIT_RETRIES: usize = 3;
const MAX_RETRY_AFTER_DELAY: Duration = Duration::from_secs(60);

/// Obtain an OAuth2 bearer token from Reddit using client credentials.
pub async fn get_access_token(
    client_id: &str,
    client_secret: &str,
) -> Result<String, Box<dyn Error>> {
    let resp: serde_json::Value = REDDIT_CLIENT
        .post("https://www.reddit.com/api/v1/access_token")
        .basic_auth(client_id, Some(client_secret))
        .form(&[("grant_type", "client_credentials")])
        .send()
        .await?
        .json()
        .await?;

    let token = resp["access_token"]
        .as_str()
        .ok_or_else(|| {
            let err = resp["error"].as_str().unwrap_or("unknown");
            format!("Reddit OAuth2 failed: {err}")
        })?
        .to_string();

    Ok(token)
}

pub(super) async fn fetch_reddit_json_with_cancel(
    url: &str,
    token: &str,
    cancel_token: Option<&CancellationToken>,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let mut attempt = 0usize;
    loop {
        check_cancelled(cancel_token)?;
        let resp = REDDIT_CLIENT.get(url).bearer_auth(token).send().await?;
        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            attempt += 1;
            if attempt > MAX_REDDIT_RETRIES {
                return Err(format!("Reddit rate limit exceeded for {url}").into());
            }
            let wait = retry_delay_for_429(resp.headers(), attempt);
            log_warn(&format!(
                "Reddit 429 rate limit attempt={attempt}/max={MAX_REDDIT_RETRIES} retry_delay_secs={} url={url}",
                wait.as_secs()
            ));
            sleep_or_cancel(wait, cancel_token).await?;
            continue;
        }
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Reddit API error ({status}): {body}").into());
        }
        return Ok(resp.json().await?);
    }
}

pub(super) fn retry_delay_for_429(headers: &HeaderMap, attempt: usize) -> Duration {
    retry_after_delay(headers).unwrap_or_else(|| exponential_retry_delay(attempt))
}

fn retry_after_delay(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    let seconds = value.parse::<u64>().ok()?;
    Some(Duration::from_secs(seconds).min(MAX_RETRY_AFTER_DELAY))
}

fn exponential_retry_delay(attempt: usize) -> Duration {
    let exponent = attempt.min(6) as u32;
    Duration::from_secs(2u64.saturating_pow(exponent)).min(MAX_RETRY_AFTER_DELAY)
}

async fn sleep_or_cancel(
    delay: Duration,
    cancel_token: Option<&CancellationToken>,
) -> Result<(), Box<dyn Error>> {
    let Some(cancel_token) = cancel_token else {
        tokio::time::sleep(delay).await;
        return Ok(());
    };

    tokio::select! {
        _ = tokio::time::sleep(delay) => Ok(()),
        _ = cancel_token.cancelled() => Err("reddit ingest canceled during retry backoff".into()),
    }
}

fn check_cancelled(cancel_token: Option<&CancellationToken>) -> Result<(), Box<dyn Error>> {
    if cancel_token.is_some_and(CancellationToken::is_cancelled) {
        return Err("reddit ingest canceled".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::retry_delay_for_429;
    use reqwest::header::{HeaderMap, HeaderValue, RETRY_AFTER};
    use std::time::Duration;

    #[test]
    fn retry_after_seconds_wins_when_within_cap() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("17"));

        assert_eq!(retry_delay_for_429(&headers, 1), Duration::from_secs(17));
    }

    #[test]
    fn retry_after_seconds_is_capped() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("999"));

        assert_eq!(retry_delay_for_429(&headers, 1), Duration::from_secs(60));
    }

    #[test]
    fn invalid_retry_after_uses_exponential_fallback() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("later"));

        assert_eq!(retry_delay_for_429(&headers, 2), Duration::from_secs(4));
    }
}
