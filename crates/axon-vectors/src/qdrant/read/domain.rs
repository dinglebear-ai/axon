//! Domain-scoped listing helpers built on [`super::scroll`].
//!
//! Ports legacy `axon-vector`'s `qdrant_domain_has_indexed_url` and
//! `qdrant_urls_for_domain_page` onto the target vector payload fields:
//! `web_domain` for the domain facet/filter and canonical URI fields for item
//! listings.

use crate::qdrant::QdrantVectorStore;
use crate::store::Result;

impl QdrantVectorStore {
    /// Whether any point for `domain` has been indexed (`chunk_index == 0` —
    /// one row per unique document).
    ///
    /// Ports legacy `qdrant_domain_has_indexed_url`. Note legacy's signature
    /// takes only `domain`, not a separate `url` — this matches that, not the
    /// `(collection, domain, url)` shape sketched in the task description.
    pub async fn domain_has_indexed_url(&self, collection: &str, domain: &str) -> Result<bool> {
        let page = self
            .scroll_page(
                collection,
                Some(domain_chunk0_filter(domain)),
                serde_json::json!(false),
                1,
                None,
            )
            .await?;
        Ok(!page.points.is_empty())
    }

    /// One page of the domain's unique indexed item canonical URIs
    /// (`chunk_index == 0`), deduped within the page and returned alongside an
    /// opaque next-page cursor (`None` once exhausted).
    pub async fn urls_for_domain_page(
        &self,
        collection: &str,
        domain: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<(Vec<String>, Option<String>)> {
        let offset = cursor.map(decode_scroll_cursor);
        let page = self
            .scroll_page(
                collection,
                Some(domain_chunk0_filter(domain)),
                serde_json::json!({"include": [
                    "item_canonical_uri",
                    "source_canonical_uri",
                    "source_item_key",
                    "chunk_locator"
                ]}),
                limit,
                offset,
            )
            .await?;
        let mut seen = std::collections::HashSet::new();
        let mut urls = Vec::new();
        for point in &page.points {
            if let Some(url) = canonical_uri_from_payload(&point.payload)
                && seen.insert(url.to_string())
            {
                urls.push(url.to_string());
            }
        }
        Ok((urls, page.next_offset.map(encode_scroll_cursor)))
    }
}

pub(super) fn domain_chunk0_filter(domain: &str) -> serde_json::Value {
    serde_json::json!({
        "must": [
            {"key": "web_domain", "match": {"value": domain}},
            {"key": "chunk_index", "match": {"value": 0}}
        ]
    })
}

fn canonical_uri_from_payload(payload: &serde_json::Value) -> Option<&str> {
    [
        "item_canonical_uri",
        "source_canonical_uri",
        "source_item_key",
    ]
    .into_iter()
    .find_map(|field| {
        payload
            .get(field)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
    })
    .or_else(|| {
        payload
            .get("chunk_locator")
            .and_then(serde_json::Value::as_object)
            .and_then(|locator| locator.get("canonical_uri"))
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
    })
}

/// Decode a cursor string produced by [`encode_scroll_cursor`] back into a
/// Qdrant scroll offset. Falls back to a bare JSON string when the cursor
/// isn't valid JSON (mirrors legacy `parse_scroll_cursor`).
fn decode_scroll_cursor(cursor: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(cursor)
        .unwrap_or_else(|_| serde_json::Value::String(cursor.to_string()))
}

/// Encode a Qdrant `next_page_offset` value as an opaque cursor string.
/// String offsets round-trip bare (no quoting); any other JSON shape is
/// serialized verbatim so [`decode_scroll_cursor`] can parse it back.
fn encode_scroll_cursor(value: serde_json::Value) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

#[cfg(test)]
#[path = "domain_tests.rs"]
mod tests;
