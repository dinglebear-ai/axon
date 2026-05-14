//! Retry logic for Qdrant operations.

use crate::core::logging::log_warn;
use anyhow::{Result, anyhow};
use reqwest::StatusCode;

use super::super::utils::qdrant_retry_delay;

/// Delete with retry on 429/5xx (up to 4 attempts, 250 ms exponential backoff).
pub(super) async fn qdrant_delete_with_retry(
    client: &reqwest::Client,
    endpoint: &str,
    body: serde_json::Value,
    context: &str,
) -> Result<()> {
    const MAX_ATTEMPTS: usize = 4;
    let mut last_error: Option<String> = None;
    for attempt in 1..=MAX_ATTEMPTS {
        match client.post(endpoint).json(&body).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    return Ok(());
                }
                let status = resp.status();
                let retryable = status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
                if retryable && attempt < MAX_ATTEMPTS {
                    log_warn(&format!(
                        "{context}: retrying qdrant delete after status={status} attempt={attempt}/{MAX_ATTEMPTS}"
                    ));
                    last_error = Some(format!(
                        "{context}: qdrant status={status} attempt={attempt}"
                    ));
                    tokio::time::sleep(qdrant_retry_delay(attempt)).await;
                    continue;
                }
                return Err(anyhow!(
                    "{context}: qdrant request failed with status {status} on attempt {attempt}"
                ));
            }
            Err(err) => {
                if attempt < MAX_ATTEMPTS {
                    log_warn(&format!(
                        "{context}: retrying qdrant delete after transport error attempt={attempt}/{MAX_ATTEMPTS}: {err}"
                    ));
                }
                last_error = Some(format!("{context}: send error attempt={attempt}: {err}"));
                if attempt < MAX_ATTEMPTS {
                    tokio::time::sleep(qdrant_retry_delay(attempt)).await;
                    continue;
                }
            }
        }
    }
    Err(anyhow!(
        "{}",
        last_error.unwrap_or_else(|| format!("{context}: unknown qdrant delete failure"))
    ))
}

/// Fetch one scroll page with retry on 429/5xx (up to 4 attempts, 250 ms exponential backoff).
pub(super) async fn scroll_page_with_retry(
    client: &reqwest::Client,
    endpoint: &str,
    body: &serde_json::Value,
) -> Result<serde_json::Value> {
    const MAX_ATTEMPTS: usize = 4;
    let mut last_err: Option<anyhow::Error> = None;
    for attempt in 1..=MAX_ATTEMPTS {
        match client.post(endpoint).json(body).send().await {
            Ok(resp) => {
                let status = resp.status();
                let retryable = status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
                if retryable && attempt < MAX_ATTEMPTS {
                    log_warn(&format!(
                        "scroll_pages_raw: retrying after status={status} attempt={attempt}/{MAX_ATTEMPTS}"
                    ));
                    tokio::time::sleep(qdrant_retry_delay(attempt)).await;
                    last_err = Some(anyhow!("qdrant scroll status={status} attempt={attempt}"));
                    continue;
                }
                let val = resp.error_for_status()?.json::<serde_json::Value>().await?;
                return Ok(val);
            }
            Err(err) => {
                if attempt < MAX_ATTEMPTS {
                    log_warn(&format!(
                        "scroll_pages_raw: retrying after transport error attempt={attempt}/{MAX_ATTEMPTS}: {err}"
                    ));
                    tokio::time::sleep(qdrant_retry_delay(attempt)).await;
                }
                last_err = Some(err.into());
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("scroll_pages_raw: unknown failure")))
}
