//! Sources facet — list of indexed URLs with chunk counts.

use crate::system::PayloadParseError;
use crate::types::{DomainSourcesResult, Pagination, SourcesResult};
use axon_core::config::Config;
use axon_vector::ops::qdrant::{
    qdrant_scroll_pages_selective, qdrant_urls_for_domain_page, sources_payload,
};
use std::collections::BTreeMap;
use std::error::Error;
use url::Url;

const DOMAIN_SOURCES_MAX_LIMIT: usize = 10_000;

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
    let (urls, next_cursor) = qdrant_urls_for_domain_page(cfg, &normalized, limit, cursor)
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
