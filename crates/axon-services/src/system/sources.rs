//! Sources facet — list of indexed item canonical URIs with chunk counts.

use crate::system::PayloadParseError;
use crate::types::{DomainSourcesResult, Pagination, SourcesResult};
use axon_core::config::Config;
use axon_core::env::env_usize_clamped;
use axon_vectors::qdrant::QdrantVectorStore;
use std::collections::BTreeMap;
use std::error::Error;
use url::Url;

const DOMAIN_SOURCES_MAX_LIMIT: usize = 10_000;
/// Mirrors legacy `sources_payload`'s facet-fetch cap.
const DEFAULT_SOURCES_FACET_LIMIT: usize = 100_000;
/// Payload page size for the schema-version-breakdown scroll — matches
/// legacy `qdrant_scroll_pages_selective`'s fixed 256-point page.
const SCROLL_PAGE_LIMIT: usize = 256;

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

pub fn normalize_domain_query(input: &str) -> Result<String, PayloadParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(PayloadParseError::new("domain must not be empty"));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(PayloadParseError::new("domain contains invalid characters"));
    }

    let host = if trimmed.contains("://") {
        Url::parse(trimmed)
            .ok()
            .and_then(|url| url.host_str().map(str::to_string))
    } else {
        let candidate = trimmed.trim_end_matches('.').trim_end_matches('/');
        candidate
            .split_once('/')
            .map(|(host, _)| host)
            .or(Some(candidate))
            .map(str::to_string)
    }
    .ok_or_else(|| PayloadParseError::new("invalid domain"))?;

    let normalized = host.trim().trim_end_matches('.').to_ascii_lowercase();
    let normalized = if let Some((host, port)) = normalized.rsplit_once(':') {
        if !host.contains(':') && port.chars().all(|c| c.is_ascii_digit()) {
            host.to_string()
        } else {
            normalized
        }
    } else {
        normalized
    };
    if normalized.is_empty()
        || normalized == "unknown"
        || normalized.contains('*')
        || normalized.contains('/')
        || normalized.contains(char::is_whitespace)
    {
        return Err(PayloadParseError::new("invalid domain"));
    }
    Ok(normalized)
}

pub fn domain_sources_from_urls(
    domain: String,
    urls: Vec<String>,
    limit: usize,
    cursor: Option<String>,
    next_cursor: Option<String>,
) -> DomainSourcesResult {
    let truncated = next_cursor.is_some();
    DomainSourcesResult {
        domain,
        count: urls.len(),
        limit,
        cursor,
        next_cursor,
        truncated,
        urls,
    }
}

#[must_use = "sources_for_domain returns a Result that should be handled"]
pub async fn sources_for_domain(
    cfg: &Config,
    domain: &str,
    pagination: Pagination,
    cursor: Option<&str>,
) -> Result<DomainSourcesResult, Box<dyn Error>> {
    let normalized = normalize_domain_query(domain)?;
    if pagination.offset > 0 {
        return Err(PayloadParseError::new("domain sources use cursor, not offset").into());
    }
    let limit = pagination.limit.clamp(1, DOMAIN_SOURCES_MAX_LIMIT);
    let store = QdrantVectorStore::new(cfg.qdrant_url.clone(), "qdrant".to_string());
    let (urls, next_cursor) = store
        .urls_for_domain_page(&cfg.collection, &normalized, limit, cursor)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("domain sources scroll failed: {e}").into() })?;
    Ok(domain_sources_from_urls(
        normalized,
        urls,
        limit,
        cursor.map(str::to_string),
        next_cursor,
    ))
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
    let store = QdrantVectorStore::new(cfg.qdrant_url.clone(), "qdrant".to_string());
    store
        .scroll_pages(
            &cfg.collection,
            None,
            serde_json::json!({"include": ["payload_schema_version"]}),
            SCROLL_PAGE_LIMIT,
            |points| {
                for point in points {
                    let version = point
                        .payload
                        .get("payload_schema_version")
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

/// Fetch the target `item_canonical_uri` facet
/// (capped by `AXON_SOURCES_FACET_LIMIT`) and slice it into one limit/offset
/// page, in the same JSON shape [`map_sources_payload`] expects.
async fn sources_payload(
    store: &QdrantVectorStore,
    cfg: &Config,
    limit: usize,
    offset: usize,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let facet_cap = env_usize_clamped(
        "AXON_SOURCES_FACET_LIMIT",
        DEFAULT_SOURCES_FACET_LIMIT,
        1,
        1_000_000,
    );
    let fetch = limit.saturating_add(offset).max(1).min(facet_cap);
    let sources = store
        .facet(&cfg.collection, "item_canonical_uri", None, fetch)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("sources facet query failed: {e}").into() })?;
    let total = sources.len();
    let urls: Vec<serde_json::Value> = sources
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|(url, chunks)| serde_json::json!({"url": url, "chunks": chunks}))
        .collect();
    Ok(serde_json::json!({
        "count": total,
        "limit": limit,
        "offset": offset,
        "urls": urls,
    }))
}

#[must_use = "sources returns a Result that should be handled"]
pub async fn sources(
    cfg: &Config,
    pagination: Pagination,
) -> Result<SourcesResult, Box<dyn Error>> {
    let store = QdrantVectorStore::new(cfg.qdrant_url.clone(), "qdrant".to_string());
    let payload = sources_payload(&store, cfg, pagination.limit, pagination.offset).await?;
    Ok(map_sources_payload(&payload)?)
}

#[cfg(test)]
#[path = "sources_tests.rs"]
mod tests;
