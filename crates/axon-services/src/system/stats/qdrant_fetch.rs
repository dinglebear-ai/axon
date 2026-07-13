//! Qdrant-side data gathering for `stats`.
//!
//! Ports legacy `axon-vector`'s `ops::stats::qdrant_fetch` onto `axon-vectors`'
//! read primitives: [`QdrantVectorStore::count_collection_points`] for the
//! exact points count and [`QdrantVectorStore::scroll_pages`] for the docs
//! count (`chunk_index == 0`, one marker point per indexed document) and the
//! indexed-token-stats sample.
//!
//! `axon-vectors` does not yet expose a raw "collection info" read (status,
//! `indexed_vectors_count`/`segments_count`, dense vector config, payload
//! schema) or a *filtered* point count, so those two pieces still go through
//! a direct Qdrant HTTP call — the same `GET`-against-Qdrant pattern already
//! used by `system::collections` for `GET /collections`. This also means the
//! docs count changes from legacy's single approximate (`exact=false`)
//! server-side `points/count` call to an exact client-driven scroll walk over
//! every `chunk_index == 0` point — correct, but O(docs) network round-trips
//! instead of O(1); acceptable for `axon stats`, which is not a hot path, but
//! worth knowing on collections with a very large document count.

use axon_core::config::Config;
use axon_core::http::internal_service_http_client;
use axon_vectors::qdrant::QdrantVectorStore;
use std::collections::HashMap;
use std::error::Error;

use crate::system::canonical_uri_from_payload;

const TOKEN_STATS_SAMPLE_POINTS: usize = 5_000;
const CHARS_PER_TOKEN_ESTIMATE: f64 = 4.0;
/// Payload page size for the docs-count and token-stats scrolls — matches
/// legacy `qdrant_scroll_pages_selective`'s fixed 256-point page.
const SCROLL_PAGE_LIMIT: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct IndexedTokenStats {
    pub(super) sampled_points: usize,
    pub(super) sampled_docs: usize,
    pub(super) sample_limit_points: usize,
    pub(super) avg_chunk_chars: f64,
    pub(super) avg_chunk_tokens_estimate: f64,
    pub(super) avg_doc_chars: f64,
    pub(super) avg_doc_tokens_estimate: f64,
}

/// Raw `GET /collections/{collection}` info, the exact point count, and the
/// exact doc-marker (`chunk_index == 0`) point count.
pub(super) async fn fetch_qdrant_snapshots(
    cfg: &Config,
    store: &QdrantVectorStore,
) -> Result<(serde_json::Value, u64, u64), Box<dyn Error>> {
    let client = internal_service_http_client()?;
    let base = cfg.qdrant_url.trim_end_matches('/');
    let col = &cfg.collection;

    let info = client
        .get(format!("{base}/collections/{col}"))
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

    let points_count = store
        .count_collection_points(col, axon_error::ErrorStage::Observing)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("points count failed: {e}").into() })?;

    let docs_count = count_doc_marker_points(store, col).await?;

    Ok((info, points_count, docs_count))
}

/// Count points where `chunk_index == 0` (one marker per indexed document)
/// via a filtered, payload-free scroll walk. See the module docs for why
/// this replaces legacy's single filtered `points/count` call.
async fn count_doc_marker_points(
    store: &QdrantVectorStore,
    collection: &str,
) -> Result<u64, Box<dyn Error>> {
    let filter = serde_json::json!({
        "must": [{"key": "chunk_index", "match": { "value": 0 }}]
    });
    let mut count = 0u64;
    store
        .scroll_pages(
            collection,
            Some(filter),
            serde_json::json!(false),
            SCROLL_PAGE_LIMIT,
            |points| {
                count += points.len() as u64;
                true
            },
        )
        .await
        .map_err(|e| -> Box<dyn Error> { format!("docs count scroll failed: {e}").into() })?;
    Ok(count)
}

pub(super) async fn sample_indexed_token_stats(
    cfg: &Config,
    store: &QdrantVectorStore,
) -> Result<Option<IndexedTokenStats>, Box<dyn Error>> {
    let mut sampled_points = 0usize;
    let mut total_chunk_chars = 0usize;
    let mut doc_chars: HashMap<String, usize> = HashMap::new();

    store
        .scroll_pages(
            &cfg.collection,
            None,
            serde_json::json!({"include": [
                "item_canonical_uri",
                "source_canonical_uri",
                "source_item_key",
                "chunk_locator",
                "chunk_text",
                "text"
            ]}),
            SCROLL_PAGE_LIMIT,
            |points| {
                for point in points {
                    if sampled_points >= TOKEN_STATS_SAMPLE_POINTS {
                        return false;
                    }
                    let Some(uri) = canonical_uri_from_payload(&point.payload) else {
                        continue;
                    };
                    let text = point
                        .payload
                        .get("chunk_text")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .or_else(|| point.payload.get("text").and_then(|v| v.as_str()))
                        .unwrap_or("");
                    if text.is_empty() {
                        continue;
                    }
                    let chars = text.chars().count();
                    sampled_points += 1;
                    total_chunk_chars += chars;
                    *doc_chars.entry(uri.to_string()).or_default() += chars;
                }
                sampled_points < TOKEN_STATS_SAMPLE_POINTS
            },
        )
        .await
        .map_err(|e| -> Box<dyn Error> {
            format!("token-stats sample scroll failed: {e}").into()
        })?;

    Ok(indexed_token_stats_from_totals(
        sampled_points,
        total_chunk_chars,
        doc_chars,
        TOKEN_STATS_SAMPLE_POINTS,
    ))
}

fn indexed_token_stats_from_totals(
    sampled_points: usize,
    total_chunk_chars: usize,
    doc_chars: HashMap<String, usize>,
    sample_limit_points: usize,
) -> Option<IndexedTokenStats> {
    if sampled_points == 0 || doc_chars.is_empty() {
        return None;
    }
    let sampled_docs = doc_chars.len();
    let total_doc_chars = doc_chars.values().sum::<usize>();
    let avg_chunk_chars = total_chunk_chars as f64 / sampled_points as f64;
    let avg_doc_chars = total_doc_chars as f64 / sampled_docs as f64;
    Some(IndexedTokenStats {
        sampled_points,
        sampled_docs,
        sample_limit_points,
        avg_chunk_chars,
        avg_chunk_tokens_estimate: avg_chunk_chars / CHARS_PER_TOKEN_ESTIMATE,
        avg_doc_chars,
        avg_doc_tokens_estimate: avg_doc_chars / CHARS_PER_TOKEN_ESTIMATE,
    })
}

#[cfg(test)]
#[path = "qdrant_fetch_tests.rs"]
mod tests;
