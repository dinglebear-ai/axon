//! Scroll pagination for Qdrant collections.

use crate::core::config::Config;
use crate::core::http::internal_service_http_client;
use crate::core::logging::log_warn;
use anyhow::{Result, anyhow};
use reqwest::StatusCode;
use std::collections::HashSet;

use super::super::utils::{qdrant_collection_endpoint, qdrant_retry_delay};

/// Fetch one scroll page with retry on 429/5xx (up to 4 attempts, 250 ms exponential backoff).
async fn scroll_page_with_retry(
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

/// Shared scroll pagination loop. POSTs to the given `endpoint` with `initial_body`,
/// reads `result.points` as raw JSON, and invokes `on_page` for each non-empty page.
/// The callback returns `true` to continue scrolling or `false` to stop early.
/// Each page request is retried up to 4 times on 429/5xx.
pub(super) async fn scroll_pages_raw(
    client: &reqwest::Client,
    endpoint: &str,
    initial_body: serde_json::Value,
    mut on_page: impl FnMut(&[serde_json::Value]) -> bool,
) -> Result<()> {
    let mut body = initial_body;
    loop {
        let val = scroll_page_with_retry(client, endpoint, &body).await?;

        let points = val["result"]["points"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if points.is_empty() {
            break;
        }
        if !on_page(points) {
            break;
        }

        let Some(next) = val["result"]
            .get("next_page_offset")
            .cloned()
            .filter(|v| !v.is_null())
        else {
            break;
        };
        body["offset"] = next;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) async fn qdrant_scroll_pages(
    cfg: &Config,
    mut process_page: impl FnMut(&[serde_json::Value]),
) -> Result<()> {
    qdrant_scroll_pages_while(cfg, |points| {
        process_page(points);
        true
    })
    .await
}

#[cfg(test)]
pub(crate) async fn qdrant_scroll_pages_while(
    cfg: &Config,
    process_page: impl FnMut(&[serde_json::Value]) -> bool,
) -> Result<()> {
    qdrant_scroll_pages_selective(cfg, serde_json::json!(true), process_page).await
}

/// Scroll with selective payload inclusion. `with_payload` controls which fields
/// are fetched — use `json!(true)` for full payload or
/// `json!({"include": ["url", "chunk_index"]})` for selective fields.
/// This avoids transferring multi-KB `chunk_text` fields when only metadata is needed.
pub async fn qdrant_scroll_pages_selective(
    cfg: &Config,
    with_payload: serde_json::Value,
    process_page: impl FnMut(&[serde_json::Value]) -> bool,
) -> Result<()> {
    let client = internal_service_http_client()?;
    let endpoint = qdrant_collection_endpoint(cfg, "points/scroll")?;
    let body = serde_json::json!({
        "limit": 256,
        "with_payload": with_payload,
        "with_vector": false
    });
    scroll_pages_raw(client, &endpoint, body, process_page).await
}

/// Scroll the collection keeping only the URL field (one entry per unique URL via chunk_index==0
/// filter) and collect into a HashSet. The `filter` value is passed directly as the Qdrant
/// filter body so callers control which subset of documents is scanned.
pub(super) async fn scroll_url_set(
    cfg: &Config,
    filter: serde_json::Value,
    limit: Option<usize>,
) -> Result<HashSet<String>> {
    let client = internal_service_http_client()?;
    let endpoint = qdrant_collection_endpoint(cfg, "points/scroll")?;
    let mut seen = HashSet::new();
    let body = serde_json::json!({
        "limit": 1000,
        "with_payload": {"include": ["url"]},
        "with_vector": false,
        "filter": filter,
    });
    scroll_pages_raw(client, &endpoint, body, |points| {
        for p in points {
            if let Some(url) = p
                .get("payload")
                .and_then(|pl| pl.get("url"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                seen.insert(url.to_string());
            }
            if limit.is_some_and(|cap| seen.len() >= cap) {
                return false;
            }
        }
        true
    })
    .await?;
    Ok(seen)
}

pub async fn qdrant_indexed_urls(cfg: &Config, limit: Option<usize>) -> Result<Vec<String>> {
    let filter = serde_json::json!({
        "must": [{"key": "chunk_index", "match": {"value": 0}}]
    });
    scroll_url_set(cfg, filter, limit)
        .await
        .map(|s| s.into_iter().collect())
}

pub async fn qdrant_urls_for_domain(cfg: &Config, domain: &str) -> Result<HashSet<String>> {
    let filter = serde_json::json!({
        "must": [
            {"key": "domain", "match": {"value": domain}},
            {"key": "chunk_index", "match": {"value": 0}}
        ]
    });
    scroll_url_set(cfg, filter, None).await
}
