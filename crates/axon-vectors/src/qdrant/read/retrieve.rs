//! Retrieve-by-URL and full-document rendering.
//!
//! Ports legacy `axon-vector`'s `retrieve_result` (the URL-variant-expanding
//! command) plus its underlying `qdrant_retrieve_by_url_details` (the
//! scroll-by-filter primitive) into one method,
//! [`QdrantVectorStore::retrieve_by_url`], and separately ports
//! `render_full_doc_from_points` so a caller can turn the returned points
//! into a document's full markdown/text.

use axon_api::source::ApiError;

use crate::qdrant::QdrantVectorStore;
use crate::store::Result;

use super::QdrantScrolledPoint;

/// Ceiling on how many points [`QdrantVectorStore::retrieve_by_url`] will
/// ever return for a single URL, matching legacy `RETRIEVE_MAX_POINTS_CEILING`.
const RETRIEVE_MAX_POINTS_CEILING: usize = 500;

/// One URL variant that failed transport-level, paired with its error
/// message. Ports legacy `axon-vector`'s `RetrieveVariantError`.
#[derive(Debug, Clone, PartialEq)]
pub struct QdrantUrlVariantError {
    pub url: String,
    pub error: String,
}

/// Result of [`QdrantVectorStore::retrieve_by_url`]. Ports legacy
/// `axon-vector`'s `DirectRetrieveResult`, minus the rendered `content`
/// field — call [`render_full_doc_from_points`] on `points` to get that,
/// keeping "fetch the chunks" and "render them" separate.
#[derive(Debug, Clone, Default)]
pub struct QdrantRetrieveByUrlResult {
    pub requested_url: String,
    pub matched_url: Option<String>,
    pub points: Vec<QdrantScrolledPoint>,
    pub max_points: usize,
    /// `true` once the accumulated point count reached `max_points` — note
    /// this is an "at cap" flag, not "definitely truncated": a document with
    /// *exactly* `max_points` chunks and no more also reports `true` here,
    /// matching legacy's identical off-by-one-ish semantics.
    pub truncated: bool,
    pub variant_errors: Vec<QdrantUrlVariantError>,
}

impl QdrantVectorStore {
    /// Retrieve every stored chunk for `target`, trying canonical URL
    /// variants in order (normalized, trailing-slash-trimmed, trailing-slash
    /// appended, and the raw input) and returning the chunks for the first
    /// variant with any indexed content. Ports legacy `retrieve_result` +
    /// `qdrant_retrieve_by_url_details` as one call.
    ///
    /// Points are excluded when `source_committed == false` (an uncommitted
    /// source's chunks are not yet visible), matching legacy's
    /// `retrieve_visibility_filter`.
    ///
    /// Returns `Err` only when every URL variant failed at the transport
    /// level; a URL that is simply not indexed comes back as `Ok` with empty
    /// `points` and `matched_url: None`.
    pub async fn retrieve_by_url(
        &self,
        collection: &str,
        target: &str,
        max_points: Option<usize>,
    ) -> Result<QdrantRetrieveByUrlResult> {
        let max_points = retrieve_max_points(max_points);
        let page_limit = max_points.clamp(1, 256);
        let mut variant_errors = Vec::new();
        let mut had_success = false;

        for candidate in canonical_first_url_candidates(target) {
            match self
                .retrieve_url_variant(collection, &candidate, page_limit, max_points)
                .await
            {
                Ok(points) => {
                    had_success = true;
                    if points.is_empty() {
                        continue;
                    }
                    let truncated = points.len() >= max_points;
                    let mut points = points;
                    points.truncate(max_points);
                    return Ok(QdrantRetrieveByUrlResult {
                        requested_url: target.to_string(),
                        matched_url: Some(candidate),
                        points,
                        max_points,
                        truncated,
                        variant_errors,
                    });
                }
                Err(err) => variant_errors.push(QdrantUrlVariantError {
                    url: candidate,
                    error: err.message.clone(),
                }),
            }
        }

        if !had_success {
            let message = variant_errors
                .first()
                .map(|err| err.error.as_str())
                .unwrap_or("no URL variants were available");
            return Err(ApiError::new(
                "vector.retrieve_failed",
                axon_error::ErrorStage::Retrieving,
                format!("retrieve failed for all URL variants: {message}"),
            ));
        }
        Ok(QdrantRetrieveByUrlResult {
            requested_url: target.to_string(),
            matched_url: None,
            points: Vec::new(),
            max_points,
            truncated: false,
            variant_errors,
        })
    }

    async fn retrieve_url_variant(
        &self,
        collection: &str,
        url_match: &str,
        page_limit: usize,
        max_points: usize,
    ) -> Result<Vec<QdrantScrolledPoint>> {
        let filter = retrieve_visibility_filter(url_match_filter(url_match));
        let mut out = Vec::new();
        self.scroll_pages(
            collection,
            Some(filter),
            serde_json::json!(true),
            page_limit,
            |page| {
                out.extend_from_slice(page);
                out.len() < max_points
            },
        )
        .await?;
        Ok(out)
    }
}

/// Concatenate a document's chunks into full markdown/text, ordered by
/// ascending `chunk_index` (chunks missing that field sort last). Ports
/// legacy `axon-vector`'s `render_full_doc_from_points`, reading raw JSON
/// payload fields (`chunk_text`/`text`, `chunk_index`) instead of a
/// strongly-typed payload struct.
pub fn render_full_doc_from_points(points: &[QdrantScrolledPoint]) -> String {
    let mut ordered: Vec<&QdrantScrolledPoint> = points.iter().collect();
    ordered.sort_by_key(|point| chunk_index_of(point).unwrap_or(i64::MAX));
    let mut text = String::new();
    for point in ordered {
        let chunk = chunk_text_of(point);
        if chunk.is_empty() {
            continue;
        }
        text.push_str(chunk);
        text.push('\n');
    }
    text.trim().to_string()
}

fn chunk_index_of(point: &QdrantScrolledPoint) -> Option<i64> {
    point
        .payload
        .get("chunk_index")
        .and_then(serde_json::Value::as_i64)
}

/// `chunk_text` when non-empty, else `text`, else `""` — mirrors legacy
/// `payload_text_typed`'s fallback order.
fn chunk_text_of(point: &QdrantScrolledPoint) -> &str {
    point
        .payload
        .get("chunk_text")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            point
                .payload
                .get("text")
                .and_then(serde_json::Value::as_str)
        })
        .unwrap_or("")
}

fn retrieve_max_points(max_points: Option<usize>) -> usize {
    max_points
        .unwrap_or(RETRIEVE_MAX_POINTS_CEILING)
        .min(RETRIEVE_MAX_POINTS_CEILING)
}

fn url_match_filter(url_match: &str) -> serde_json::Value {
    serde_json::json!({ "must": [{ "key": "url", "match": { "value": url_match } }] })
}

/// Layer legacy's uncommitted-source visibility exclusion onto a base
/// filter: `must_not source_committed == false`. `base` is always the
/// single-key object from [`url_match_filter`], so a direct `must_not`
/// insert is safe (no need for a general multi-filter merge).
fn retrieve_visibility_filter(mut base: serde_json::Value) -> serde_json::Value {
    let must_not = serde_json::json!([{ "key": "source_committed", "match": { "value": false } }]);
    if let Some(object) = base.as_object_mut() {
        object.insert("must_not".to_string(), must_not);
    }
    base
}

/// Canonical-first URL candidate list: normalized, trailing-slash-trimmed,
/// trailing-slash-appended, and the raw input — in that order, deduped.
/// Ports legacy `retrieve_result`'s `canonical_first_url_candidates`.
fn canonical_first_url_candidates(target: &str) -> Vec<String> {
    let normalized = axon_core::http::normalize_url(target).into_owned();
    let trimmed = normalized.trim_end_matches('/').to_string();
    let slashed = format!("{trimmed}/");
    let mut out = Vec::new();
    for variant in [normalized, trimmed, slashed, target.to_string()] {
        if variant.is_empty() || out.contains(&variant) {
            continue;
        }
        out.push(variant);
    }
    out
}

#[cfg(test)]
#[path = "retrieve_tests.rs"]
mod tests;
