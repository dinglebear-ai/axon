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
#[path = "sources_tests.rs"]
mod tests;
