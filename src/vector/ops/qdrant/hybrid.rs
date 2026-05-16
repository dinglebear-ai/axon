//! Hybrid search via Qdrant `/query` endpoint with RRF fusion.
//!
//! Sends two prefetch arms (dense + BM42 sparse) and fuses with Reciprocal Rank Fusion.
//! Only called for collections in `VectorMode::Named` (named dense + sparse vectors).

use crate::core::config::Config;
use crate::core::http::internal_service_http_client;
use crate::core::logging::log_debug;
use crate::vector::ops::sparse::SparseVector;
use anyhow::Result;
use serde::Serialize;
use std::time::Instant;

use super::types::{QdrantQueryResponse, QdrantSearchHit};
use super::utils::{qdrant_collection_endpoint, qdrant_post_json_with_retry};

// Typed request bodies for Qdrant `/points/query`. Replaces serde_json::json!{...}
// macro allocations on the retrieval hot path. (bd axon_rust-d71.25)

#[derive(Serialize)]
struct HybridQueryBody<'a> {
    prefetch: [PrefetchArm<'a>; 2],
    query: FusionSpec,
    limit: usize,
    with_payload: bool,
    with_vector: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<&'a serde_json::Value>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum PrefetchArm<'a> {
    Dense {
        query: &'a [f32],
        using: &'static str,
        limit: usize,
        params: DenseParams,
    },
    Sparse {
        query: &'a SparseVector,
        using: &'static str,
        limit: usize,
    },
}

#[derive(Serialize)]
struct DenseParams {
    hnsw_ef: usize,
    quantization: QuantizationParams,
}

#[derive(Serialize)]
struct QuantizationParams {
    rescore: bool,
    oversampling: f32,
}

#[derive(Serialize)]
struct FusionSpec {
    fusion: &'static str,
}

#[derive(Serialize)]
struct NamedDenseQueryBody<'a> {
    query: &'a [f32],
    using: &'static str,
    limit: usize,
    with_payload: bool,
    with_vector: bool,
    params: DenseParams,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<&'a serde_json::Value>,
}

/// Perform hybrid search using dense + BM42 sparse prefetch with RRF fusion.
///
/// Issues a single POST to `/collections/{name}/points/query` with two `prefetch` arms
/// (one dense, one sparse) and `"query": {"fusion": "rrf"}` to combine them.
/// `limit` is the final number of results after fusion. Each prefetch arm fetches
/// `cfg.hybrid_search_candidates` candidates before RRF fusion. Requires a Named-mode collection.
#[tracing::instrument(
    name = "vector.hybrid",
    skip(cfg, dense_vector, sparse_vector, filter),
    fields(
        collection = %cfg.collection,
        sparse_terms = sparse_vector.indices.len(),
        candidates = cfg.hybrid_search_candidates,
        limit,
        filtered = filter.is_some(),
    )
)]
pub(crate) async fn qdrant_hybrid_search(
    cfg: &Config,
    dense_vector: &[f32],
    sparse_vector: &SparseVector,
    limit: usize,
    candidates_override: Option<usize>,
    filter: Option<&serde_json::Value>,
) -> Result<Vec<QdrantSearchHit>> {
    let client = internal_service_http_client()?;
    let url = qdrant_collection_endpoint(cfg, "points/query")?;

    let candidates = candidates_override
        .unwrap_or(cfg.hybrid_search_candidates)
        .max(limit);
    let hnsw_ef = cfg.hnsw_ef_search;

    let body = HybridQueryBody {
        prefetch: [
            PrefetchArm::Dense {
                query: dense_vector,
                using: "dense",
                limit: candidates,
                params: DenseParams {
                    hnsw_ef,
                    quantization: QuantizationParams {
                        rescore: true,
                        oversampling: 1.5,
                    },
                },
            },
            PrefetchArm::Sparse {
                query: sparse_vector,
                using: "bm42",
                limit: candidates,
            },
        ],
        query: FusionSpec { fusion: "rrf" },
        limit,
        with_payload: true,
        with_vector: false,
        filter,
    };

    let search_start = Instant::now();
    let parsed: QdrantQueryResponse = qdrant_post_json_with_retry(
        client,
        &url,
        &body,
        "qdrant_hybrid_search",
        &cfg.collection,
        search_start,
    )
    .await?;
    log_debug(&format!(
        "qdrant search_complete mode=hybrid collection={} hits={} latency_ms={}",
        cfg.collection,
        parsed.result.points.len(),
        search_start.elapsed().as_millis()
    ));
    Ok(parsed.result.points)
}

/// Dense-only search for Named collections using the `/points/query` endpoint.
///
/// Named collections reject `/points/search` requests that send a flat `"vector": [...]`
/// payload — they expect requests that address a named vector config. The `/points/query`
/// endpoint accepts a bare array in `"query": [...]` with `"using": "dense"` to specify
/// which named vector to search against. This function uses that form to run dense-only
/// retrieval when sparse vectors are unavailable (empty query, hybrid disabled).
///
/// Use `qdrant_hybrid_search` when a sparse vector is available for RRF fusion.
#[tracing::instrument(
    name = "vector.named_dense",
    skip(cfg, dense_vector, filter),
    fields(collection = %cfg.collection, limit, filtered = filter.is_some())
)]
pub(crate) async fn qdrant_named_dense_search(
    cfg: &Config,
    dense_vector: &[f32],
    limit: usize,
    filter: Option<&serde_json::Value>,
) -> Result<Vec<QdrantSearchHit>> {
    let client = internal_service_http_client()?;
    let url = qdrant_collection_endpoint(cfg, "points/query")?;

    let hnsw_ef = cfg.hnsw_ef_search;
    let body = NamedDenseQueryBody {
        query: dense_vector,
        using: "dense",
        limit,
        with_payload: true,
        with_vector: false,
        params: DenseParams {
            hnsw_ef,
            quantization: QuantizationParams {
                rescore: true,
                oversampling: 1.5,
            },
        },
        filter,
    };

    let search_start = Instant::now();
    let parsed: QdrantQueryResponse = qdrant_post_json_with_retry(
        client,
        &url,
        &body,
        "qdrant_named_dense_search",
        &cfg.collection,
        search_start,
    )
    .await?;
    log_debug(&format!(
        "qdrant search_complete mode=named_dense collection={} hits={} latency_ms={}",
        cfg.collection,
        parsed.result.points.len(),
        search_start.elapsed().as_millis()
    ));
    Ok(parsed.result.points)
}

#[cfg(test)]
#[path = "hybrid_tests.rs"]
mod tests;
