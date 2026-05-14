//! Retrieve operations for Qdrant points.

use crate::core::config::Config;
use crate::core::http::internal_service_http_client;
use crate::core::logging::log_warn;
use anyhow::Result;

use super::super::types::{QdrantPoint, QdrantRetrieveByUrlResult};
use super::super::utils::qdrant_collection_endpoint;
use super::scroll::scroll_pages_raw;

pub(crate) fn retrieve_scroll_limit(max_points: Option<usize>) -> usize {
    super::super::utils::retrieve_max_points(max_points).clamp(1, 256)
}

pub(crate) fn parse_retrieve_scroll_points(
    points: &[serde_json::Value],
) -> (Vec<QdrantPoint>, usize) {
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
pub(crate) async fn qdrant_retrieve_by_url_details(
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

pub(crate) async fn qdrant_retrieve_by_url(
    cfg: &Config,
    url_match: &str,
    max_points: Option<usize>,
) -> Result<Vec<QdrantPoint>> {
    Ok(qdrant_retrieve_by_url_details(cfg, url_match, max_points)
        .await?
        .points)
}

#[cfg(test)]
mod tests {
    use super::{parse_retrieve_scroll_points, retrieve_scroll_limit};

    #[test]
    fn retrieve_scroll_limit_honors_small_max_points() {
        assert_eq!(retrieve_scroll_limit(Some(1)), 1);
        assert_eq!(retrieve_scroll_limit(Some(42)), 42);
        assert_eq!(retrieve_scroll_limit(Some(0)), 1);
        assert_eq!(retrieve_scroll_limit(None), 256);
        assert_eq!(retrieve_scroll_limit(Some(500)), 256);
    }

    #[test]
    fn parse_retrieve_scroll_points_counts_malformed_points() {
        let points = vec![
            serde_json::json!({
                "id": "ok",
                "payload": {
                    "url": "https://example.com",
                    "chunk_text": "hello",
                    "chunk_index": 0
                }
            }),
            serde_json::json!({
                "id": "bad",
                "payload": {
                    "url": 123,
                    "chunk_text": "bad"
                }
            }),
        ];
        let (parsed, malformed) = parse_retrieve_scroll_points(&points);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].payload.url, "https://example.com");
        assert_eq!(malformed, 1);
    }
}
