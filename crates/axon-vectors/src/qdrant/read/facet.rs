//! Facet aggregation over an explicit payload key plus an optional Qdrant
//! filter. Callers choose current-contract keys such as `web_domain` or
//! `item_canonical_uri`; this module intentionally does not expose old
//! URL/domain-specific wrappers.

use crate::qdrant::QdrantVectorStore;
use crate::store::Result;

impl QdrantVectorStore {
    /// `POST /collections/{collection}/facet` — aggregate distinct values of
    /// `key` (optionally scoped by `filter`) with per-value point counts.
    ///
    /// Results are sorted by value ascending; a missing facet value is
    /// reported as `"unknown"`, and an empty-string value is dropped.
    pub async fn facet(
        &self,
        collection: &str,
        key: &str,
        filter: Option<serde_json::Value>,
        limit: usize,
    ) -> Result<Vec<(String, u64)>> {
        let stage = axon_error::ErrorStage::Retrieving;
        let http = self.http()?;
        let url = http.endpoint().collection_path(collection, "facet");
        let mut body = serde_json::json!({ "key": key, "limit": limit });
        if let Some(filter) = filter
            && filter.as_object().is_some_and(|object| !object.is_empty())
        {
            body["filter"] = filter;
        }
        let response: FacetResponse = http.post_json(stage, &url, &body, "qdrant_facet").await?;
        Ok(parse_facet_hits(response.result.hits))
    }
}

#[derive(Debug, serde::Deserialize)]
struct FacetHit {
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    count: Option<u64>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct FacetResult {
    #[serde(default)]
    hits: Vec<FacetHit>,
}

#[derive(Debug, serde::Deserialize)]
struct FacetResponse {
    result: FacetResult,
}

fn parse_facet_hits(hits: Vec<FacetHit>) -> Vec<(String, u64)> {
    let mut out: Vec<(String, u64)> = hits
        .into_iter()
        .filter_map(|hit| {
            let value = hit.value.unwrap_or_else(|| "unknown".to_string());
            if value.is_empty() {
                return None;
            }
            Some((value, hit.count.unwrap_or(0)))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[cfg(test)]
#[path = "facet_tests.rs"]
mod tests;
