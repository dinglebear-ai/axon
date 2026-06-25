//! Domains facet — indexed domains with vector / URL counts.

use crate::system::{PayloadParseError, normalize_domain_query};
use crate::types::{
    DetailedDomainFacet, DetailedDomainsResult, DomainFacet, DomainIndexedResult, DomainsResult,
    Pagination,
};
use axon_core::config::Config;
use axon_vector::ops::qdrant::{
    domains_payload, env_usize_clamped, payload_domain, payload_url, qdrant_domain_has_indexed_url,
    qdrant_scroll_pages_selective,
};
use std::collections::{HashMap, HashSet};
use std::error::Error;

const DEFAULT_DOMAINS_DETAILED_LIMIT: usize = 10_000_000;

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

#[must_use = "domains returns a Result that should be handled"]
pub async fn domains(
    cfg: &Config,
    pagination: Pagination,
) -> Result<DomainsResult, Box<dyn Error>> {
    let payload = domains_payload(cfg, pagination.limit, pagination.offset)
        .await
        .map_err(|e| -> Box<dyn Error> { format!("domains facet query failed: {e}").into() })?;
    Ok(map_domains_payload(&payload)?)
}

#[must_use = "domain_indexed returns a Result that should be handled"]
pub async fn domain_indexed(
    cfg: &Config,
    domain: &str,
) -> Result<DomainIndexedResult, Box<dyn Error>> {
    let normalized = normalize_domain_query(domain)?;
    let indexed = qdrant_domain_has_indexed_url(cfg, &normalized)
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
    // Selective payload: only fetch domain + url fields. Avoids transferring
    // multi-KB chunk_text per point — the detailed domains scan only aggregates
    // domain membership and URL sets.
    qdrant_scroll_pages_selective(
        cfg,
        serde_json::json!({"include": ["domain", "url"]}),
        |points: &[serde_json::Value]| {
            for point in points {
                if count >= limit {
                    return false;
                }
                if let Some(payload) = point.get("payload") {
                    let domain = payload_domain(payload);
                    let url = payload_url(payload);
                    let entry = by_domain.entry(domain).or_insert((0, HashSet::new()));
                    entry.0 += 1;
                    if !url.is_empty() {
                        entry.1.insert(url);
                    }
                    count += 1;
                }
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
