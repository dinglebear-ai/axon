//! Delete operations for Qdrant points.

use crate::core::config::Config;
use crate::core::http::internal_service_http_client;
use crate::core::logging::log_warn;
use anyhow::{Result, anyhow};
use reqwest::StatusCode;
use std::collections::HashSet;

use super::super::utils::{qdrant_collection_endpoint, qdrant_retry_delay};
use super::scroll::scroll_url_set;

/// Delete with retry on 429/5xx (up to 4 attempts, 250 ms exponential backoff).
async fn qdrant_delete_with_retry(
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
    let message = match last_error {
        Some(error) => error,
        None => format!("{context}: unknown qdrant delete failure"),
    };
    Err(anyhow!("{}", message))
}

/// Delete all Qdrant points matching `url` via payload filter.
#[cfg(test)]
pub(crate) async fn qdrant_delete_by_url_filter(cfg: &Config, url: &str) -> Result<()> {
    let client = internal_service_http_client()?;
    let endpoint = qdrant_collection_endpoint(cfg, "points/delete?wait=true")?;
    qdrant_delete_with_retry(
        client,
        &endpoint,
        serde_json::json!({
            "filter": {"must": [{"key": "url", "match": {"value": url}}]}
        }),
        "qdrant_delete_by_url_filter",
    )
    .await?;
    Ok(())
}

/// Delete stale tail chunks for `url` — points with `chunk_index >= new_chunk_count`.
///
/// Called after a successful upsert to clean up orphan chunks from a prior run that
/// produced more chunks than the current one. If chunk count did not decrease, the
/// filter matches zero points and this is a cheap no-op.
///
/// Uses `wait=false` (async delete) — the preceding upsert already guaranteed data
/// consistency, so these orphan-chunk deletes do not need to block on index rebuild.
/// This avoids saturating Qdrant's HNSW indexer when many lanes run concurrently.
///
/// Never call before the upsert succeeds — doing so risks permanent data loss if the
/// upsert subsequently fails.
pub async fn qdrant_delete_stale_tail(
    cfg: &Config,
    url: &str,
    new_chunk_count: usize,
) -> Result<()> {
    let client = internal_service_http_client()?;
    let endpoint = qdrant_collection_endpoint(cfg, "points/delete?wait=false")?;
    qdrant_delete_with_retry(
        client,
        &endpoint,
        serde_json::json!({
            "filter": {
                "must": [
                    {"key": "url", "match": {"value": url}},
                    {"key": "chunk_index", "range": {"gte": new_chunk_count}}
                ]
            }
        }),
        "qdrant_delete_stale_tail",
    )
    .await?;
    Ok(())
}

/// Delete all Qdrant points for URLs that belong to `domain` but are NOT in `current_urls`.
/// Uses a single batch delete with a `should` filter instead of per-URL requests.
/// Returns the number of stale URLs whose points were deleted.
pub async fn qdrant_delete_stale_domain_urls(
    cfg: &Config,
    domain: &str,
    current_urls: &HashSet<String>,
) -> Result<usize> {
    let indexed = scroll_url_set(
        cfg,
        serde_json::json!({
            "must": [
                {"key": "domain", "match": {"value": domain}},
                {"key": "chunk_index", "match": {"value": 0}}
            ]
        }),
        None,
    )
    .await?;
    let stale: Vec<String> = indexed
        .into_iter()
        .filter(|url| !current_urls.contains(url))
        .collect();
    if stale.is_empty() {
        return Ok(0);
    }
    // Batch delete: build a single `should` filter matching all stale URLs at once.
    let url_conditions: Vec<serde_json::Value> = stale
        .iter()
        .map(|url| serde_json::json!({"key": "url", "match": {"value": url}}))
        .collect();
    let client = internal_service_http_client()?;
    // Use wait=false for maintenance deletes — matches qdrant_delete_stale_tail pattern.
    // The preceding scroll already verified which URLs are stale; no immediate
    // consistency is needed, and wait=true blocks on HNSW index rebuild per batch.
    let delete_url = qdrant_collection_endpoint(cfg, "points/delete?wait=false")?;
    // Qdrant filter limit is generous but chunk at 500 to be safe with large stale sets.
    for batch in url_conditions.chunks(500) {
        qdrant_delete_with_retry(
            client,
            &delete_url,
            serde_json::json!({
                "filter": {"should": batch}
            }),
            "qdrant_delete_stale_domain_urls",
        )
        .await?;
    }
    Ok(stale.len())
}

/// Delete a repo's previously-indexed `git_content_kind="file"` points whose
/// `url` is not in `current_urls`. Returns the count of stale URLs deleted.
///
/// Must be called only after the current file set has been embedded — the
/// surviving set is defined by `current_urls`, so running it before embedding
/// would delete live chunks. Uses `wait=false` (async delete): the preceding
/// embed already guaranteed consistency, so this maintenance delete need not
/// block on HNSW index rebuild.
pub async fn qdrant_delete_stale_repo_file_urls(
    cfg: &Config,
    provider: &str,
    owner: &str,
    repo: &str,
    current_urls: &HashSet<String>,
) -> Result<usize> {
    let indexed = scroll_url_set(cfg, repo_file_points_filter(provider, owner, repo), None).await?;
    let stale: Vec<String> = indexed
        .into_iter()
        .filter(|url| !current_urls.contains(url))
        .collect();
    if stale.is_empty() {
        return Ok(0);
    }

    let url_conditions: Vec<serde_json::Value> = stale
        .iter()
        .map(|url| serde_json::json!({"key": "url", "match": {"value": url}}))
        .collect();
    let client = internal_service_http_client()?;
    let endpoint = qdrant_collection_endpoint(cfg, "points/delete?wait=false")?;
    for batch in url_conditions.chunks(500) {
        // Scope the delete to this repo's file points (must) AND a stale URL
        // (should), so a URL collision can never delete another repo's points.
        qdrant_delete_with_retry(
            client,
            &endpoint,
            serde_json::json!({
                "filter": {
                    "must": [
                        {"key": "provider", "match": {"value": provider}},
                        {"key": "git_owner", "match": {"value": owner}},
                        {"key": "git_repo", "match": {"value": repo}},
                        {"key": "git_content_kind", "match": {"value": "file"}}
                    ],
                    "should": batch
                }
            }),
            "qdrant_delete_stale_repo_file_urls",
        )
        .await?;
    }
    Ok(stale.len())
}

/// Test helper: build the delete body that `qdrant_delete_stale_tail` would
/// send for the given `url` and `new_chunk_count`. Used by `delete_tests.rs`
/// to assert the filter shape without making live HTTP calls (T-H3).
#[cfg(test)]
pub(crate) fn stale_tail_filter_body(url: &str, new_chunk_count: usize) -> serde_json::Value {
    serde_json::json!({
        "filter": {
            "must": [
                {"key": "url", "match": {"value": url}},
                {"key": "chunk_index", "range": {"gte": new_chunk_count}}
            ]
        }
    })
}

#[cfg(test)]
fn repo_code_points_delete_body(provider: &str, owner: &str, repo: &str) -> serde_json::Value {
    serde_json::json!({
        "filter": repo_file_points_filter(provider, owner, repo)
    })
}

fn repo_file_points_filter(provider: &str, owner: &str, repo: &str) -> serde_json::Value {
    serde_json::json!({
        "must": [
            {"key": "provider", "match": {"value": provider}},
            {"key": "git_owner", "match": {"value": owner}},
            {"key": "git_repo", "match": {"value": repo}},
            {"key": "git_content_kind", "match": {"value": "file"}}
        ]
    })
}

pub async fn qdrant_delete_points(cfg: &Config, ids: &[String]) -> Result<usize> {
    if ids.is_empty() {
        return Ok(0);
    }
    let client = internal_service_http_client()?;
    let url = qdrant_collection_endpoint(cfg, "points/delete?wait=true")?;
    for batch in ids.chunks(1000) {
        qdrant_delete_with_retry(
            client,
            &url,
            serde_json::json!({"points": batch}),
            "qdrant_delete_points",
        )
        .await?;
    }
    Ok(ids.len())
}

#[cfg(test)]
#[path = "delete_tests.rs"]
mod repo_code_delete_tests;
