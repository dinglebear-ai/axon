//! Domains facet — indexed web domains with vector / canonical-item counts.

use crate::system::{PayloadParseError, canonical_uri_from_payload, normalize_domain_query};
use crate::types::{
    DetailedDomainFacet, DetailedDomainsResult, DomainFacet, DomainIndexedResult, DomainsResult,
    Pagination,
};
use axon_core::config::Config;
use axon_core::env::env_usize_clamped;
use axon_vectors::qdrant::QdrantVectorStore;
use std::collections::{HashMap, HashSet};
use std::error::Error;

const DEFAULT_DOMAINS_DETAILED_LIMIT: usize = 10_000_000;
/// Mirrors legacy `domains_payload`'s facet-fetch cap.
const DEFAULT_DOMAINS_FACET_LIMIT: usize = 100_000;
/// Payload page size for the detailed-domains scroll — matches legacy
/// `qdrant_scroll_pages_selective`'s fixed 256-point page.
const SCROLL_PAGE_LIMIT: usize = 256;

fn payload_domain(payload: &serde_json::Value) -> String {
    payload
        .get("web_domain")
        .and_then(serde_json::Value::as_str)
        .or_else(|| payload.get("domain").and_then(serde_json::Value::as_str))
        .unwrap_or("unknown")
        .to_string()
}

fn payload_url(payload: &serde_json::Value) -> String {
    let has_target_domain = payload
        .get("web_domain")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|domain| !domain.is_empty());
    let legacy_url = (!has_target_domain)
        .then(|| payload.get("url").and_then(serde_json::Value::as_str))
        .flatten();
    canonical_uri_from_payload(payload)
        .or(legacy_url)
        .unwrap_or_default()
        .to_string()
}

pub fn map_domains_payload(
    payload: &serde_json::Value,
) -> Result<DomainsResult, PayloadParseError> {
    let limit = payload
        .get("limit")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| PayloadParseError::new("missing limit"))? as usize;
    let offset = payload
        .get("offset")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| PayloadParseError::new("missing offset"))? as usize;

    let domains = payload
        .get("domains")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| PayloadParseError::new("missing domains"))?
        .iter()
        .enumerate()
        .map(|(i, item)| {
            Ok(DomainFacet {
                domain: item
                    .get("domain")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| PayloadParseError::new(format!("domains[{i}]: missing domain")))?
                    .to_string(),
                vectors: item
                    .get("vectors")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or_else(|| {
                        PayloadParseError::new(format!("domains[{i}]: missing vectors"))
                    })? as usize,
            })
        })
        .collect::<Result<Vec<_>, PayloadParseError>>()?;

    Ok(DomainsResult {
        domains,
        limit,
        offset,
    })
}

/// Fetch the `web_domain` facet
/// (capped by `AXON_DOMAINS_FACET_LIMIT`) and slice it into one
/// limit/offset page, in the same JSON shape [`map_domains_payload`] expects.
async fn domains_payload(
    store: &QdrantVectorStore,
    cfg: &Config,
    limit: usize,
    offset: usize,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let facet_cap = env_usize_clamped(
        "AXON_DOMAINS_FACET_LIMIT",
        DEFAULT_DOMAINS_FACET_LIMIT,
        1,
        1_000_000,
    );
    let fetch = limit.saturating_add(offset).max(1).min(facet_cap);
    let domains = store
        .facet(&cfg.collection, "web_domain", None, fetch)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("domains facet query failed: {e}").into() })?;
    let values = domains
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|(domain, vectors)| serde_json::json!({ "domain": domain, "vectors": vectors }))
        .collect::<Vec<_>>();
    Ok(serde_json::json!({
        "domains": values,
        "limit": limit,
        "offset": offset,
    }))
}

#[must_use = "domains returns a Result that should be handled"]
pub async fn domains(
    cfg: &Config,
    pagination: Pagination,
) -> Result<DomainsResult, Box<dyn Error>> {
    let store = QdrantVectorStore::new(cfg.qdrant_url.clone(), "qdrant".to_string());
    let payload = domains_payload(&store, cfg, pagination.limit, pagination.offset).await?;
    Ok(map_domains_payload(&payload)?)
}

#[must_use = "domain_indexed returns a Result that should be handled"]
pub async fn domain_indexed(
    cfg: &Config,
    domain: &str,
) -> Result<DomainIndexedResult, Box<dyn Error>> {
    let normalized = normalize_domain_query(domain)?;
    let store = QdrantVectorStore::new(cfg.qdrant_url.clone(), "qdrant".to_string());
    let indexed = store
        .domain_has_indexed_url(&cfg.collection, &normalized)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("domain indexed check failed: {e}").into() })?;
    Ok(DomainIndexedResult {
        domain: normalized,
        indexed,
    })
}

pub fn summarize_detailed_domains(payloads: &[serde_json::Value]) -> DetailedDomainsResult {
    summarize_detailed_domains_limited(payloads, None)
}

pub fn summarize_detailed_domains_limited(
    payloads: &[serde_json::Value],
    limit: Option<usize>,
) -> DetailedDomainsResult {
    let mut by_domain: HashMap<String, (usize, HashSet<String>)> = HashMap::new();
    for payload in payloads.iter().take(limit.unwrap_or(payloads.len())) {
        let domain = payload_domain(payload);
        let url = payload_url(payload);
        let entry = by_domain.entry(domain).or_insert((0, HashSet::new()));
        entry.0 += 1;
        if !url.is_empty() {
            entry.1.insert(url);
        }
    }

    let mut domains: Vec<DetailedDomainFacet> = by_domain
        .into_iter()
        .map(|(domain, (vectors, urls))| DetailedDomainFacet {
            domain,
            vectors,
            urls: urls.len(),
        })
        .collect();
    domains.sort_by(|a, b| a.domain.cmp(&b.domain));
    DetailedDomainsResult { domains }
}

#[must_use = "detailed_domains returns a Result that should be handled"]
pub async fn detailed_domains(cfg: &Config) -> Result<DetailedDomainsResult, Box<dyn Error>> {
    let limit = env_usize_clamped(
        "AXON_DOMAINS_DETAILED_LIMIT",
        DEFAULT_DOMAINS_DETAILED_LIMIT,
        1,
        10_000_000,
    );
    // Aggregate directly inside the scroll callback to avoid buffering all payloads.
    // Previous implementation cloned every payload into a Vec before summarizing,
    // spiking memory on large collections.
    let mut by_domain: HashMap<String, (usize, HashSet<String>)> = HashMap::new();
    let mut count = 0usize;
    let store = QdrantVectorStore::new(cfg.qdrant_url.clone(), "qdrant".to_string());
    // Selective payload: only fetch domain + canonical URI fields. Avoids
    // transferring multi-KB chunk_text per point — the detailed domains scan
    // only aggregates domain membership and item URI sets.
    store
        .scroll_pages(
            &cfg.collection,
            None,
            serde_json::json!({"include": [
                "web_domain",
                "item_canonical_uri",
                "source_canonical_uri",
                "source_item_key",
                "chunk_locator"
            ]}),
            SCROLL_PAGE_LIMIT,
            |points| {
                for point in points {
                    if count >= limit {
                        return false;
                    }
                    let domain = payload_domain(&point.payload);
                    let url = payload_url(&point.payload);
                    let entry = by_domain.entry(domain).or_insert((0, HashSet::new()));
                    entry.0 += 1;
                    if !url.is_empty() {
                        entry.1.insert(url);
                    }
                    count += 1;
                }
                count < limit
            },
        )
        .await
        .map_err(|e| -> Box<dyn Error> { format!("detailed domains scroll failed: {e}").into() })?;

    let mut domains: Vec<DetailedDomainFacet> = by_domain
        .into_iter()
        .map(|(domain, (vectors, urls))| DetailedDomainFacet {
            domain,
            vectors,
            urls: urls.len(),
        })
        .collect();
    domains.sort_by(|a, b| a.domain.cmp(&b.domain));
    Ok(DetailedDomainsResult { domains })
}

#[cfg(test)]
#[path = "domains_tests.rs"]
mod tests;
