//! Delete operations for Qdrant points.

use crate::core::config::Config;
use crate::core::http::internal_service_http_client;
use anyhow::Result;
use std::collections::HashSet;

use super::super::utils::qdrant_collection_endpoint;
use super::super::utils::qdrant_retry_delay;
use crate::core::logging::log_warn;
use anyhow::anyhow;
use reqwest::StatusCode;

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
pub(crate) async fn qdrant_delete_stale_tail(
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

pub(crate) async fn qdrant_delete_points(cfg: &Config, ids: &[String]) -> Result<usize> {
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
