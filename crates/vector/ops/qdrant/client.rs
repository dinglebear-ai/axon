//! Low-level Qdrant HTTP client operations.
//!
//! ## Key invariants
//! - Use [`qdrant_url_facets`] (O(1) `/facet` POST) for URL counting and aggregation.
//!   Never use full scroll for aggregation — it loads the entire collection into memory.
//! - [`ensure_collection`](super::tei::ensure_collection) issues GET-first, PUT only on 404.
//!   Safe to call on every embed.
//! - All delete operations use [`qdrant_delete_with_retry`] with exponential backoff.

use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use crate::crates::core::logging::{log_debug, log_warn};
use anyhow::{Result, anyhow};
use reqwest::StatusCode;
use std::collections::HashSet;
use std::time::Duration;
use std::time::Instant;

use super::types::{QdrantPoint, QdrantSearchHit, QdrantSearchResponse};
use super::utils::{qdrant_base, retrieve_max_points};

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
                    tokio::time::sleep(Duration::from_millis(250 * (1 << (attempt - 1)))).await;
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
                    tokio::time::sleep(Duration::from_millis(250 * (1 << (attempt - 1)))).await;
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
                    tokio::time::sleep(Duration::from_millis(250 * (1 << (attempt - 1)))).await;
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
                    tokio::time::sleep(Duration::from_millis(250 * (1 << (attempt - 1)))).await;
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
async fn scroll_pages_raw(
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

pub(crate) async fn qdrant_scroll_pages(
    cfg: &Config,
    mut process_page: impl FnMut(&[serde_json::Value]),
) -> Result<()> {
    let client = http_client()?;
    let endpoint = format!(
        "{}/collections/{}/points/scroll",
        qdrant_base(cfg),
        cfg.collection
    );
    let body = serde_json::json!({
        "limit": 256,
        "with_payload": true,
        "with_vector": false
    });
    scroll_pages_raw(client, &endpoint, body, |points| {
        process_page(points);
        true
    })
    .await
}

/// Scroll the collection keeping only the URL field (one entry per unique URL via chunk_index==0
/// filter) and collect into a HashSet. The `filter` value is passed directly as the Qdrant
/// filter body so callers control which subset of documents is scanned.
async fn scroll_url_set(
    cfg: &Config,
    filter: serde_json::Value,
    limit: Option<usize>,
) -> Result<HashSet<String>> {
    let client = http_client()?;
    let endpoint = format!(
        "{}/collections/{}/points/scroll",
        qdrant_base(cfg),
        cfg.collection
    );
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

pub(crate) async fn qdrant_urls_for_domain(cfg: &Config, domain: &str) -> Result<HashSet<String>> {
    let filter = serde_json::json!({
        "must": [
            {"key": "domain", "match": {"value": domain}},
            {"key": "chunk_index", "match": {"value": 0}}
        ]
    });
    scroll_url_set(cfg, filter, None).await
}

/// Delete all Qdrant points matching `url` via payload filter.
#[cfg(test)]
pub(crate) async fn qdrant_delete_by_url_filter(cfg: &Config, url: &str) -> Result<()> {
    let client = http_client()?;
    let endpoint = format!(
        "{}/collections/{}/points/delete?wait=true",
        qdrant_base(cfg),
        cfg.collection
    );
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
    let client = http_client()?;
    let endpoint = format!(
        "{}/collections/{}/points/delete?wait=false",
        qdrant_base(cfg),
        cfg.collection
    );
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
    let indexed = qdrant_urls_for_domain(cfg, domain).await?;
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
    let client = http_client()?;
    let delete_url = format!(
        "{}/collections/{}/points/delete?wait=true",
        qdrant_base(cfg),
        cfg.collection
    );
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
    let client = http_client()?;
    let url = format!(
        "{}/collections/{}/points/delete?wait=true",
        qdrant_base(cfg),
        cfg.collection
    );
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

pub(crate) async fn qdrant_domain_facets(
    cfg: &Config,
    limit: usize,
) -> Result<Vec<(String, usize)>> {
    let client = http_client()?;
    let url = format!("{}/collections/{}/facet", qdrant_base(cfg), cfg.collection);
    let value = client
        .post(url)
        .json(&serde_json::json!({
            "key": "domain",
            "limit": limit,
        }))
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

    let mut out = Vec::new();
    if let Some(hits) = value["result"]["hits"].as_array() {
        for hit in hits {
            let domain = hit
                .get("value")
                .and_then(|v| v.as_str())
                .map_or_else(|| "unknown".to_string(), str::to_string);
            let vectors = hit.get("count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            out.push((domain, vectors));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

pub(crate) async fn qdrant_url_facets(cfg: &Config, limit: usize) -> Result<Vec<(String, usize)>> {
    let client = http_client()?;
    let url = format!("{}/collections/{}/facet", qdrant_base(cfg), cfg.collection);
    let value = client
        .post(url)
        .json(&serde_json::json!({
            "key": "url",
            "limit": limit,
        }))
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

    let mut out = Vec::new();
    if let Some(hits) = value["result"]["hits"].as_array() {
        for hit in hits {
            let source_url = hit
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let chunks = hit.get("count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            if !source_url.is_empty() {
                out.push((source_url, chunks));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

pub(crate) async fn qdrant_search(
    cfg: &Config,
    vector: &[f32],
    limit: usize,
) -> Result<Vec<QdrantSearchHit>> {
    let client = http_client()?;
    let url = format!(
        "{}/collections/{}/points/search",
        qdrant_base(cfg),
        cfg.collection
    );
    let search_start = Instant::now();
    let res = client
        .post(&url)
        .json(&serde_json::json!({
            "vector": vector,
            "limit": limit,
            "with_payload": true,
            "with_vector": false
        }))
        .send()
        .await
        .map_err(|e| {
            log_warn(&format!(
                "qdrant_search failed collection={} duration_ms={} error={e}",
                cfg.collection,
                search_start.elapsed().as_millis()
            ));
            e
        })?
        .error_for_status()
        .map_err(|e| {
            log_warn(&format!(
                "qdrant_search failed collection={} duration_ms={} error={e}",
                cfg.collection,
                search_start.elapsed().as_millis()
            ));
            e
        })?
        .json::<QdrantSearchResponse>()
        .await?;
    log_debug(&format!(
        "qdrant search hits={} collection={}",
        res.result.len(),
        cfg.collection
    ));
    Ok(res.result)
}

pub(crate) async fn qdrant_retrieve_by_url(
    cfg: &Config,
    url_match: &str,
    max_points: Option<usize>,
) -> Result<Vec<QdrantPoint>> {
    let client = http_client()?;
    let endpoint = format!(
        "{}/collections/{}/points/scroll",
        qdrant_base(cfg),
        cfg.collection
    );
    let body = serde_json::json!({
        "limit": 256,
        "with_payload": true,
        "with_vector": false,
        "filter": {
            "must": [
                {
                    "key": "url",
                    "match": {"value": url_match}
                }
            ]
        }
    });
    let max_points = retrieve_max_points(max_points);
    let mut out = Vec::new();
    scroll_pages_raw(client, &endpoint, body, |points| {
        for p in points {
            // Clone required: scroll_pages_raw yields &[Value] (borrowed from response JSON).
            // from_value consumes the value, so we must clone from the slice reference.
            if let Ok(point) = serde_json::from_value::<QdrantPoint>(p.clone()) {
                out.push(point);
            }
        }
        out.len() < max_points
    })
    .await?;
    out.truncate(max_points);
    Ok(out)
}
