//! Dual-query Qdrant batch search via `/points/query/batch`.
//!
//! Sends two queries (NL primary + keyword secondary) in a single HTTP request
//! to eliminate the second TLS+TCP handshake/header overhead on every `ask`.
//! Reuses the same VectorMode arm-selection logic as
//! [`dispatch_vector_search_request`](super::commands::dispatch_vector_search_request)
//! so post-migrate Named collections cannot accidentally fall back to
//! dense-only on the batch path while the single path goes hybrid (or
//! vice-versa).
//!
//! ## Failure semantics
//!
//! Per Qdrant issues #4048 and #5208, a batch is atomic at the transport
//! layer: if any single query body is malformed Qdrant returns a non-2xx and
//! the whole batch errors. Callers MUST fall back to the parallel-single
//! [`tokio::join!`](tokio::join) path on `Err` so a transient batch failure
//! does not turn off retrieval for the user. (bd axon_rust-j2c)
//!
//! ## Per-arm timing
//!
//! Qdrant's `/points/query/batch` only returns one aggregate `time` field —
//! per-arm latency is unavailable on this path. Callers should set the
//! aggregate dispatch time on a single timing slot and leave the secondary
//! slot as None on the batch path; only the fallback parallel-single path
//! emits true per-arm timings.
//!
//! ## Mode dispatch
//!
//! - `VectorMode::Named` + hybrid + non-empty sparse → RRF prefetch (dense + bm42)
//! - `VectorMode::Named` + hybrid disabled OR empty sparse → named dense-only (`using: "dense"`)
//! - `VectorMode::Unnamed` → returns Err — the legacy `/points/search/batch`
//!   shape is intentionally not supported here. retrieval.rs falls back to
//!   parallel-single in that case (one extra request only happens once per
//!   process for legacy collections, then the VectorMode cache kicks in).

use crate::core::config::Config;
use crate::core::http::internal_service_http_client;
use crate::core::logging::log_debug;
use crate::vector::ops::sparse::SparseVector;
use crate::vector::ops::tei::qdrant_store::{VectorMode, get_or_fetch_vector_mode};
use anyhow::{Result, anyhow};
use std::time::Instant;

use super::types::{QdrantBatchQueryResponse, QdrantSearchHit};
use super::utils::{qdrant_collection_endpoint, qdrant_post_json_with_retry};

/// One arm of a dual search. Mirrors the inputs of a single
/// `/points/query` call so each arm's mode dispatch can be decided
/// independently inside the batch.
pub(crate) struct DualSearchArm<'a> {
    pub(crate) dense: &'a [f32],
    /// Sparse vector for this arm. May be empty (e.g. all-stopword query) —
    /// in that case the arm falls through to dense-only inside the batch.
    pub(crate) sparse: &'a SparseVector,
    pub(crate) filter: Option<&'a serde_json::Value>,
}

/// Result of a dual search. Each field is the per-arm hit list, in the same
/// order the arms were submitted (Qdrant guarantees positional alignment of
/// `result[]` to `searches[]`).
#[derive(Debug)]
pub(crate) struct DualSearchResult {
    pub(crate) primary: Vec<QdrantSearchHit>,
    pub(crate) secondary: Vec<QdrantSearchHit>,
}

/// Build one batch entry — the same body shape as a single `/points/query`
/// call — selecting between RRF (hybrid) and named-dense per the VectorMode
/// dispatch rules. `VectorMode::Unnamed` is rejected upstream, so this only
/// has to handle the Named arms.
fn build_named_query_body(
    arm: &DualSearchArm<'_>,
    limit: usize,
    candidates: usize,
    hybrid_enabled: bool,
    hnsw_ef: usize,
) -> serde_json::Value {
    let dense_params = serde_json::json!({
        "hnsw_ef": hnsw_ef,
        "quantization": {"rescore": true, "oversampling": 1.5},
    });
    let use_hybrid = hybrid_enabled && !arm.sparse.is_empty();
    let mut body = if use_hybrid {
        serde_json::json!({
            "prefetch": [
                {
                    "query": arm.dense,
                    "using": "dense",
                    "limit": candidates,
                    "params": dense_params,
                },
                {
                    "query": arm.sparse,
                    "using": "bm42",
                    "limit": candidates,
                },
            ],
            "query": {"fusion": "rrf"},
            "limit": limit,
            "with_payload": true,
            "with_vector": false,
        })
    } else {
        serde_json::json!({
            "query": arm.dense,
            "using": "dense",
            "limit": limit,
            "with_payload": true,
            "with_vector": false,
            "params": dense_params,
        })
    };
    if let Some(f) = arm.filter {
        body["filter"] = f.clone();
    }
    body
}

/// Issue both arms of an ask dual-embed search in a single
/// `/collections/{name}/points/query/batch` request.
///
/// On any transport / parse failure the function returns `Err`; callers MUST
/// catch and fall back to the parallel-single (`tokio::join!`) path. This is
/// not negotiable — Qdrant's batch endpoint is atomic at the transport layer
/// (issues #4048, #5208), so one transient hiccup must not silently disable
/// retrieval for the user.
///
/// `VectorMode::Unnamed` is rejected with a clear error so retrieval.rs's
/// fallback path takes over. Implementing legacy-collection batch via
/// `/points/search/batch` is deliberately out of scope; legacy collections
/// see at most one extra request per worker process before the VectorMode
/// cache short-circuits.
#[tracing::instrument(
    name = "vector.dual_search",
    skip(cfg, primary, secondary),
    fields(
        collection = %cfg.collection,
        primary_sparse_terms = primary.sparse.indices.len(),
        secondary_sparse_terms = secondary.sparse.indices.len(),
        candidates = cfg.hybrid_search_candidates,
        limit,
    )
)]
pub(crate) async fn qdrant_dual_search(
    cfg: &Config,
    primary: DualSearchArm<'_>,
    secondary: DualSearchArm<'_>,
    limit: usize,
    candidates_override: Option<usize>,
) -> Result<DualSearchResult> {
    let mode = get_or_fetch_vector_mode(cfg)
        .await
        .map_err(|e| anyhow!("vector mode probe for dual search: {e}"))?;
    if matches!(mode, VectorMode::Unnamed) {
        return Err(anyhow!(
            "qdrant_dual_search: unnamed-mode collections are not supported on \
             the batch path; caller must fall back to parallel-single"
        ));
    }

    let candidates = candidates_override
        .unwrap_or(cfg.hybrid_search_candidates)
        .max(limit);
    let hybrid_enabled = cfg.hybrid_search_enabled;
    let hnsw_ef = cfg.hnsw_ef_search;
    let primary_body = build_named_query_body(&primary, limit, candidates, hybrid_enabled, hnsw_ef);
    let secondary_body =
        build_named_query_body(&secondary, limit, candidates, hybrid_enabled, hnsw_ef);

    let body = serde_json::json!({
        "searches": [primary_body, secondary_body],
    });

    let client = internal_service_http_client()?;
    let url = qdrant_collection_endpoint(cfg, "points/query/batch")?;

    let started = Instant::now();
    let parsed: QdrantBatchQueryResponse = qdrant_post_json_with_retry(
        client,
        &url,
        &body,
        "qdrant_dual_search",
        &cfg.collection,
        started,
    )
    .await?;

    if parsed.result.len() < 2 {
        return Err(anyhow!(
            "qdrant_dual_search: expected 2 result arrays, got {}",
            parsed.result.len()
        ));
    }
    let mut iter = parsed.result.into_iter();
    let primary_hits = iter.next().expect("len checked").points;
    let secondary_hits = iter.next().expect("len checked").points;

    log_debug(&format!(
        "qdrant search_complete mode=dual_batch collection={} primary_hits={} secondary_hits={} latency_ms={}",
        cfg.collection,
        primary_hits.len(),
        secondary_hits.len(),
        started.elapsed().as_millis()
    ));

    Ok(DualSearchResult {
        primary: primary_hits,
        secondary: secondary_hits,
    })
}

#[cfg(test)]
#[path = "dual_search_tests.rs"]
mod tests;
