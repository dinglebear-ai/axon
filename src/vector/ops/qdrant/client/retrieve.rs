//! Retrieve operations for Qdrant points.

use crate::core::config::Config;
use crate::core::http::internal_service_http_client;
use crate::core::logging::log_warn;
use anyhow::{Result, anyhow};
use std::time::Instant;

use super::super::types::{QdrantBatchQueryResponse, QdrantPoint, QdrantRetrieveByUrlResult};
use super::super::utils::{qdrant_collection_endpoint, qdrant_post_json_with_retry};
use super::scroll::scroll_pages_raw;

pub fn retrieve_scroll_limit(max_points: Option<usize>) -> usize {
    super::super::utils::retrieve_max_points(max_points).clamp(1, 256)
}

pub fn parse_retrieve_scroll_points(points: &[serde_json::Value]) -> (Vec<QdrantPoint>, usize) {
    let mut out = Vec::new();
    let mut malformed = 0usize;
    for p in points {
        // Clone required: scroll_pages_raw yields &[Value] (borrowed from response JSON).
        // from_value consumes the value, so we must clone from the slice reference.
        match serde_json::from_value::<QdrantPoint>(p.clone()) {
            Ok(point) => out.push(point),
            Err(err) => {
                malformed += 1;
                log_warn(&format!(
                    "qdrant_retrieve_by_url: malformed point skipped: {err}"
                ));
            }
        }
    }
    (out, malformed)
}

fn safe_url_hash(url: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    hasher.finish()
}

#[tracing::instrument(
    skip(cfg),
    fields(
        collection = %cfg.collection,
        url_hash = safe_url_hash(url_match),
        max_points = tracing::field::Empty,
        date_filter = cfg.since.is_some() || cfg.before.is_some(),
        returned_count = tracing::field::Empty,
        malformed_count = tracing::field::Empty,
        truncated = tracing::field::Empty,
    )
)]
pub async fn qdrant_retrieve_by_url_details(
    cfg: &Config,
    url_match: &str,
    max_points: Option<usize>,
) -> Result<QdrantRetrieveByUrlResult> {
    let client = internal_service_http_client()?;
    let endpoint = qdrant_collection_endpoint(cfg, "points/scroll")?;
    let url_filter = super::super::filter::url_filter(url_match);
    let filter = match super::super::filter::build_scraped_at_filter(
        cfg.since.as_deref(),
        cfg.before.as_deref(),
    ) {
        Ok(Some(date_filter)) => {
            super::super::filter::combine_must_filters(&[url_filter, date_filter])
        }
        Ok(None) => url_filter,
        Err(err) => return Err(anyhow::anyhow!(err)),
    };
    let max_points = super::super::utils::retrieve_max_points(max_points);
    let page_limit = retrieve_scroll_limit(Some(max_points));
    tracing::Span::current().record("max_points", max_points as u64);
    let body = serde_json::json!({
        "limit": page_limit,
        "with_payload": true,
        "with_vector": false,
        "filter": filter
    });
    let mut out = Vec::new();
    let mut malformed_points = 0usize;
    scroll_pages_raw(client, &endpoint, body, |points| {
        let (mut page_points, page_malformed) = parse_retrieve_scroll_points(points);
        malformed_points += page_malformed;
        out.append(&mut page_points);
        out.len() < max_points
    })
    .await?;
    let truncated = out.len() >= max_points;
    out.truncate(max_points);
    tracing::Span::current().record("returned_count", out.len() as u64);
    tracing::Span::current().record("malformed_count", malformed_points as u64);
    tracing::Span::current().record("truncated", truncated);
    if malformed_points > 0 {
        tracing::warn!(
            malformed_count = malformed_points,
            returned_count = out.len(),
            "qdrant_retrieve_by_url skipped malformed points"
        );
    }
    Ok(QdrantRetrieveByUrlResult {
        url_match: url_match.to_string(),
        points: out,
        max_points,
        malformed_points,
        truncated,
    })
}

pub async fn qdrant_retrieve_by_url(
    cfg: &Config,
    url_match: &str,
    max_points: Option<usize>,
) -> Result<Vec<QdrantPoint>> {
    Ok(qdrant_retrieve_by_url_details(cfg, url_match, max_points)
        .await?
        .points)
}

/// Retrieve full documents for multiple URLs in a single batch request.
///
/// Sends one `/points/query/batch` POST with N filter-only queries (no
/// vectors). Results are returned in the same order as `urls` (Qdrant
/// guarantees positional alignment). Each inner `Vec` holds the chunks
/// for the corresponding URL; an empty inner Vec means no indexed content
/// was found for that URL.
///
/// The per-URL `limit` is `retrieve_max_points(max_points)` (ceiling 500).
/// Unlike the scroll path ([`qdrant_retrieve_by_url`]), this function does
/// NOT paginate — documents with more than 500 chunks are truncated at 500.
/// For typical RAG full-doc context (ask pipeline, `doc_chunk_limit` ≤ 200)
/// this ceiling is never reached.
///
/// On any transport or parse failure returns `Err`; callers MUST fall back
/// to the per-URL scroll path ([`qdrant_retrieve_by_url`]) so a transient
/// batch error does not silently elide the full-doc context.
///
/// Note: VectorMode does not affect this path — filter-only retrieval
/// works identically for both Named and Unnamed collections.
/// Maximum number of URLs accepted in a single batch retrieve call.
/// Callers with more URLs must fall back to buffer_unordered streaming.
pub const BATCH_RETRIEVE_URL_CAP: usize = 64;

pub async fn qdrant_batch_retrieve_by_urls(
    cfg: &Config,
    urls: &[String],
    max_points: Option<usize>,
) -> Result<Vec<Vec<QdrantPoint>>> {
    if urls.is_empty() {
        return Ok(Vec::new());
    }
    if urls.len() > BATCH_RETRIEVE_URL_CAP {
        return Err(anyhow!(
            "qdrant_batch_retrieve_by_urls: batch too large ({} URLs, cap is {}); use buffer_unordered instead",
            urls.len(),
            BATCH_RETRIEVE_URL_CAP
        ));
    }
    // Use the total-points ceiling (500), not the per-page scroll limit (256).
    // The query endpoint returns all matches in one shot; no pagination loop.
    let limit = super::super::utils::retrieve_max_points(max_points);
    let date_filter =
        super::super::filter::build_scraped_at_filter(cfg.since.as_deref(), cfg.before.as_deref())
            .map_err(|e| anyhow!(e))?;
    let searches: Vec<serde_json::Value> = urls
        .iter()
        .map(|url| {
            let url_f = super::super::filter::url_filter(url);
            let filter = match &date_filter {
                Some(df) => super::super::filter::combine_must_filters(&[url_f, df.clone()]),
                None => url_f,
            };
            serde_json::json!({
                "filter": filter,
                "limit": limit,
                "with_payload": true,
                "with_vector": false,
            })
        })
        .collect();
    let body = serde_json::json!({ "searches": searches });
    let client = internal_service_http_client()?;
    let endpoint = qdrant_collection_endpoint(cfg, "points/query/batch")?;
    let started = Instant::now();
    let parsed: QdrantBatchQueryResponse = qdrant_post_json_with_retry(
        client,
        &endpoint,
        &body,
        "qdrant_batch_retrieve",
        &cfg.collection,
        started,
    )
    .await?;
    if parsed.result.len() != urls.len() {
        return Err(anyhow!(
            "qdrant_batch_retrieve: expected {} result sets, got {}",
            urls.len(),
            parsed.result.len()
        ));
    }
    Ok(parsed
        .result
        .into_iter()
        .map(|qr| {
            qr.points
                .into_iter()
                .map(|hit| QdrantPoint {
                    id: hit.id,
                    payload: hit.payload,
                })
                .collect()
        })
        .collect())
}

#[cfg(test)]
#[path = "retrieve_tests.rs"]
mod tests;
