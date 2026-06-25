//! Facet aggregation for Qdrant collections.

use anyhow::Result;
use axon_core::config::Config;
use axon_core::http::internal_service_http_client;

use super::super::utils::qdrant_collection_endpoint;

pub async fn qdrant_domain_facets(cfg: &Config, limit: usize) -> Result<Vec<(String, usize)>> {
    qdrant_facet(cfg, "domain", limit).await
}

pub async fn qdrant_url_facets(cfg: &Config, limit: usize) -> Result<Vec<(String, usize)>> {
    qdrant_facet(cfg, "url", limit).await
}

pub async fn qdrant_facet(cfg: &Config, key: &str, limit: usize) -> Result<Vec<(String, usize)>> {
    qdrant_facet_filtered(cfg, key, limit, serde_json::json!({})).await
}

pub async fn qdrant_facet_filtered(
    cfg: &Config,
    key: &str,
    limit: usize,
    filter: serde_json::Value,
) -> Result<Vec<(String, usize)>> {
    let client = internal_service_http_client()?;
    let url = qdrant_collection_endpoint(cfg, "facet")?;
    let mut body = serde_json::json!({
        "key": key,
        "limit": limit,
    });
    if filter.as_object().is_some_and(|o| !o.is_empty()) {
        body["filter"] = filter;
    }
    let value = client
        .post(url)
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    parse_facet_response(&value)
}

fn parse_facet_response(value: &serde_json::Value) -> Result<Vec<(String, usize)>> {
    let mut out = Vec::new();
    if let Some(hits) = value["result"]["hits"].as_array() {
        for hit in hits {
            let facet_value = hit
                .get("value")
                .and_then(|v| v.as_str())
                .map_or_else(|| "unknown".to_string(), str::to_string);
            let chunks = hit.get("count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            if !facet_value.is_empty() {
                out.push((facet_value, chunks));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}
