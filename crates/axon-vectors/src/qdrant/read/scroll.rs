//! `/points/scroll` pagination primitives for current-contract payload reads.
//!
//! Three primitives, from lowest- to highest-level:
//! - [`QdrantVectorStore::scroll_page`] — exactly one page.
//! - [`QdrantVectorStore::scroll_pages`] — visits every page via an
//!   early-stoppable callback without buffering the whole result set.
//! - [`QdrantVectorStore::scroll_all`] — a `Vec`-collecting convenience over
//!   `scroll_pages`, bounded by an optional `max_points` cap.

use crate::qdrant::QdrantVectorStore;
use crate::store::Result;

/// One point returned from a raw payload-projected scroll. Kept as raw JSON
/// rather than a strongly-typed struct: these primitives read the caller's
/// requested payload projection, not a fixed schema.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QdrantScrolledPoint {
    pub id: serde_json::Value,
    pub payload: serde_json::Value,
}

/// One page of a scroll: the points plus Qdrant's opaque `next_page_offset`
/// (`None` once the collection is exhausted).
#[derive(Debug, Clone, Default)]
pub struct ScrollPage {
    pub points: Vec<QdrantScrolledPoint>,
    pub next_offset: Option<serde_json::Value>,
}

impl QdrantVectorStore {
    /// One page of `/points/scroll`.
    ///
    /// `with_payload` is passed through verbatim to Qdrant: `json!(true)`
    /// for the full payload, `json!(false)` for none, or
    /// `json!({"include": ["item_canonical_uri", "chunk_index"]})` to project
    /// specific fields (avoids transferring multi-KB `chunk_text` when only metadata
    /// is needed). `offset` is `None` for the first page and otherwise the
    /// previous page's `next_offset`, round-tripped verbatim.
    pub async fn scroll_page(
        &self,
        collection: &str,
        filter: Option<serde_json::Value>,
        with_payload: serde_json::Value,
        limit: usize,
        offset: Option<serde_json::Value>,
    ) -> Result<ScrollPage> {
        let stage = axon_error::ErrorStage::Retrieving;
        let http = self.http()?;
        let url = http.endpoint().collection_path(collection, "points/scroll");
        let mut body = serde_json::json!({
            "limit": limit,
            "with_payload": with_payload,
            "with_vector": false,
        });
        if let Some(filter) = filter {
            body["filter"] = filter;
        }
        if let Some(offset) = offset {
            body["offset"] = offset;
        }
        let response: ScrollResponse = http.post_json(stage, &url, &body, "qdrant_scroll").await?;
        Ok(ScrollPage {
            points: response
                .result
                .points
                .into_iter()
                .map(|point| QdrantScrolledPoint {
                    id: point.id,
                    payload: point.payload,
                })
                .collect(),
            next_offset: response.result.next_page_offset,
        })
    }

    /// Page through `/points/scroll`, invoking `on_page` once per non-empty
    /// page until the collection is exhausted or `on_page` returns `false`.
    ///
    /// This is the low-memory shape legacy's `qdrant_scroll_pages_selective`/
    /// `_while` gave aggregation-only callers (e.g. a domain/version facet
    /// scan) — it never materializes more than one page at a time.
    pub async fn scroll_pages(
        &self,
        collection: &str,
        filter: Option<serde_json::Value>,
        with_payload: serde_json::Value,
        page_limit: usize,
        mut on_page: impl FnMut(&[QdrantScrolledPoint]) -> bool,
    ) -> Result<()> {
        let mut offset = None;
        loop {
            let page = self
                .scroll_page(
                    collection,
                    filter.clone(),
                    with_payload.clone(),
                    page_limit,
                    offset,
                )
                .await?;
            if page.points.is_empty() {
                break;
            }
            if !on_page(&page.points) {
                break;
            }
            let Some(next) = page.next_offset else {
                break;
            };
            offset = Some(next);
        }
        Ok(())
    }

    /// Page through `/points/scroll` until exhausted or `max_points` is
    /// reached (`None` = unbounded, matching legacy `qdrant_scroll_pages_while`),
    /// collecting every visited point into one `Vec`.
    ///
    /// Prefer [`QdrantVectorStore::scroll_pages`] for large collections —
    /// this buffers the whole (possibly capped) result set in memory.
    pub async fn scroll_all(
        &self,
        collection: &str,
        filter: Option<serde_json::Value>,
        with_payload: serde_json::Value,
        page_limit: usize,
        max_points: Option<usize>,
    ) -> Result<Vec<QdrantScrolledPoint>> {
        let mut out = Vec::new();
        self.scroll_pages(collection, filter, with_payload, page_limit, |page| {
            out.extend_from_slice(page);
            max_points.is_none_or(|cap| out.len() < cap)
        })
        .await?;
        if let Some(cap) = max_points {
            out.truncate(cap);
        }
        Ok(out)
    }
}

#[derive(Debug, Default, serde::Deserialize)]
struct ScrollPointRaw {
    #[serde(default)]
    id: serde_json::Value,
    #[serde(default)]
    payload: serde_json::Value,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ScrollResult {
    #[serde(default)]
    points: Vec<ScrollPointRaw>,
    #[serde(default)]
    next_page_offset: Option<serde_json::Value>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ScrollResponse {
    #[serde(default)]
    result: ScrollResult,
}

#[cfg(test)]
#[path = "scroll_tests.rs"]
mod tests;
