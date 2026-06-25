//! Legacy dense-only search for Unnamed collections.
//!
//! Unnamed collections (created before named-vector support) use `/points/search`
//! with a flat `"vector"` field. Named collections use `/points/query` via
//! [`hybrid`](super::hybrid).

use anyhow::Result;
use axon_core::config::Config;
use axon_core::http::internal_service_http_client;
use axon_core::logging::log_debug;
use std::time::Instant;

use super::types::{QdrantSearchHit, QdrantSearchResponse};
use super::utils::{qdrant_collection_endpoint, qdrant_post_json_with_retry};

/// Dense-only vector search for Unnamed (legacy) collections.
///
/// Issues a POST to `/collections/{name}/points/search` with a flat `"vector"` field.
/// Named collections must use [`qdrant_hybrid_search`](super::hybrid::qdrant_hybrid_search)
/// or [`qdrant_named_dense_search`](super::hybrid::qdrant_named_dense_search) instead.
///
/// `hnsw_ef` is sourced from `cfg.hnsw_ef_search_legacy` (env > TOML > default 64, clamped 16..=256).
/// The `quantization.rescore` field in `params` is harmless for collections without
/// quantization configured — Qdrant ignores it silently.
pub(crate) async fn qdrant_search(
    cfg: &Config,
    vector: &[f32],
    limit: usize,
    filter: Option<&serde_json::Value>,
) -> Result<Vec<QdrantSearchHit>> {
    let client = internal_service_http_client()?;
    let url = qdrant_collection_endpoint(cfg, "points/search")?;
    let hnsw_ef = cfg.hnsw_ef_search_legacy;
    let search_start = Instant::now();
    let mut body = serde_json::json!({
        "vector": vector,
        "limit": limit,
        "with_payload": true,
        "with_vector": false,
        "params": {
            "hnsw_ef": hnsw_ef,
            "quantization": {
                "rescore": true,
                "oversampling": 1.5
            }
        }
    });
    if let Some(f) = filter {
        body["filter"] = f.clone();
    }
    let res: QdrantSearchResponse = qdrant_post_json_with_retry(
        client,
        &url,
        &body,
        "qdrant_search",
        &cfg.collection,
        search_start,
    )
    .await?;
    log_debug(&format!(
        "qdrant search_complete mode=unnamed_dense collection={} hits={} latency_ms={}",
        cfg.collection,
        res.result.len(),
        search_start.elapsed().as_millis()
    ));
    Ok(res.result)
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
