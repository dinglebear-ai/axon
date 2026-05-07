use crate::core::config::Config;
use std::error::Error;

use super::super::client::{qdrant_domain_facets, qdrant_url_facets};
use super::super::utils::env_usize_clamped;

pub async fn sources_payload(
    cfg: &Config,
    limit: usize,
    offset: usize,
) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    let facet_cap = env_usize_clamped("AXON_SOURCES_FACET_LIMIT", 100_000, 1, 1_000_000);
    let fetch = limit.saturating_add(offset).max(1).min(facet_cap);
    let sources = qdrant_url_facets(cfg, fetch).await?;
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

pub async fn domains_payload(
    cfg: &Config,
    limit: usize,
    offset: usize,
) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    let facet_cap = env_usize_clamped("AXON_DOMAINS_FACET_LIMIT", 100_000, 1, 1_000_000);
    let fetch = limit.saturating_add(offset).max(1).min(facet_cap);
    let domains = qdrant_domain_facets(cfg, fetch).await?;
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
