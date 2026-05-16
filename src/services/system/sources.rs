//! Sources facet — list of indexed URLs with chunk counts.

use crate::core::config::Config;
use crate::services::system::PayloadParseError;
use crate::services::types::{Pagination, SourcesResult};
use crate::vector::ops::qdrant::{qdrant_scroll_pages_selective, sources_payload};
use std::collections::BTreeMap;
use std::error::Error;

pub fn map_sources_payload(
    payload: &serde_json::Value,
) -> Result<SourcesResult, PayloadParseError> {
    let count = payload
        .get("count")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| PayloadParseError::new("missing count"))? as usize;
    let limit = payload
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| PayloadParseError::new("missing limit"))? as usize;
    let offset = payload
        .get("offset")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| PayloadParseError::new("missing offset"))? as usize;
    let urls = payload
        .get("urls")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| PayloadParseError::new("missing urls"))?
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let url = item
                .get("url")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| PayloadParseError::new(format!("urls[{i}]: missing url")))?
                .to_string();
            let chunks = item
                .get("chunks")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| PayloadParseError::new(format!("urls[{i}]: missing chunks")))?
                as usize;
            Ok((url, chunks))
        })
        .collect::<Result<Vec<_>, PayloadParseError>>()?;

    Ok(SourcesResult {
        count,
        limit,
        offset,
        urls,
        schema_version_breakdown: None,
    })
}

/// Scroll the collection counting points per `payload_schema_version`.
///
/// Points without the field (legacy pre-`axon_rust-lu6a` data) are tallied
/// under the key `1` (implicit version). This is a full scroll — expensive
/// on multi-million-point collections — and is only invoked when the
/// caller opts in via `--by-schema-version` on `axon sources`.
pub async fn sources_schema_version_breakdown(
    cfg: &Config,
) -> Result<BTreeMap<u32, usize>, Box<dyn Error>> {
    let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
    qdrant_scroll_pages_selective(
        cfg,
        serde_json::json!({"include": ["payload_schema_version"]}),
        |points: &[serde_json::Value]| {
            for point in points {
                let version = point
                    .get("payload")
                    .and_then(|p| p.get("payload_schema_version"))
                    .and_then(serde_json::Value::as_u64)
                    .map(|n| n as u32)
                    .unwrap_or(1);
                *counts.entry(version).or_insert(0) += 1;
            }
            true
        },
    )
    .await
    .map_err(|e| -> Box<dyn Error> {
        format!("schema-version breakdown scroll failed: {e}").into()
    })?;
    Ok(counts)
}

/// Like [`sources`] but additionally fills `schema_version_breakdown` via a
/// scroll-based aggregation. Use only when the caller explicitly opts in
/// (e.g. `axon sources --by-schema-version`) — the scan is O(N) over the
/// whole collection.
#[must_use = "sources_with_breakdown returns a Result that should be handled"]
pub async fn sources_with_breakdown(
    cfg: &Config,
    pagination: Pagination,
) -> Result<SourcesResult, Box<dyn Error>> {
    let mut result = sources(cfg, pagination).await?;
    let breakdown = sources_schema_version_breakdown(cfg).await?;
    result.schema_version_breakdown = Some(breakdown);
    Ok(result)
}

#[must_use = "sources returns a Result that should be handled"]
pub async fn sources(
    cfg: &Config,
    pagination: Pagination,
) -> Result<SourcesResult, Box<dyn Error>> {
    let payload = sources_payload(cfg, pagination.limit, pagination.offset)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("sources facet query failed: {e}").into() })?;
    Ok(map_sources_payload(&payload)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn map_sources_valid() {
        let payload = json!({
            "count": 2,
            "limit": 10,
            "offset": 0,
            "urls": [
                { "url": "https://example.com/a", "chunks": 3 },
                { "url": "https://example.com/b", "chunks": 7 }
            ]
        });
        let result = map_sources_payload(&payload).unwrap();
        assert_eq!(result.count, 2);
        assert_eq!(result.limit, 10);
        assert_eq!(result.offset, 0);
        assert_eq!(result.urls.len(), 2);
        assert_eq!(result.urls[0], ("https://example.com/a".to_string(), 3));
        assert_eq!(result.urls[1], ("https://example.com/b".to_string(), 7));
    }

    #[test]
    fn map_sources_missing_count() {
        let payload = json!({ "limit": 10, "offset": 0, "urls": [] });
        let err = map_sources_payload(&payload).unwrap_err();
        assert!(
            err.to_string().contains("count"),
            "error must mention 'count', got: {err}"
        );
    }

    #[test]
    fn map_sources_missing_urls() {
        let payload = json!({ "count": 0, "limit": 10, "offset": 0 });
        let err = map_sources_payload(&payload).unwrap_err();
        assert!(
            err.to_string().contains("urls"),
            "error must mention 'urls', got: {err}"
        );
    }

    #[test]
    fn map_sources_url_entry_missing_url_field() {
        let payload = json!({
            "count": 1,
            "limit": 10,
            "offset": 0,
            "urls": [{ "chunks": 5 }]
        });
        let err = map_sources_payload(&payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("urls[0]"),
            "error must reference urls[0], got: {msg}"
        );
    }

    #[test]
    fn map_sources_url_entry_missing_chunks_field() {
        let payload = json!({
            "count": 1,
            "limit": 10,
            "offset": 0,
            "urls": [{ "url": "https://example.com" }]
        });
        let err = map_sources_payload(&payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("chunks"),
            "error must mention 'chunks', got: {msg}"
        );
    }

    #[test]
    fn map_sources_empty_urls_array() {
        let payload = json!({ "count": 0, "limit": 50, "offset": 0, "urls": [] });
        let result = map_sources_payload(&payload).unwrap();
        assert_eq!(result.count, 0);
        assert!(result.urls.is_empty());
    }
}
