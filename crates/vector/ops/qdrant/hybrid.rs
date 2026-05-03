//! Hybrid search via Qdrant `/query` endpoint with RRF fusion.
//!
//! Sends two prefetch arms (dense + BM42 sparse) and fuses with Reciprocal Rank Fusion.
//! Only called for collections in `VectorMode::Named` (named dense + sparse vectors).

use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use crate::crates::core::logging::{log_debug, log_warn};
use crate::crates::vector::ops::sparse::SparseVector;
use anyhow::Result;
use serde::Serialize;
use std::time::Instant;

use super::types::{QdrantQueryResponse, QdrantSearchHit};
use super::utils::{HNSW_EF_SEARCH, qdrant_base};

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
    let client = http_client()?;
    let url = format!(
        "{}/collections/{}/points/query",
        qdrant_base(cfg),
        cfg.collection
    );

    let candidates = candidates_override
        .unwrap_or(cfg.hybrid_search_candidates)
        .max(limit);
    let hnsw_ef = *HNSW_EF_SEARCH;

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
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .inspect_err(|e| {
            log_warn(&format!(
                "qdrant_hybrid_search transport_error collection={} duration_ms={} err={e}",
                cfg.collection,
                search_start.elapsed().as_millis()
            ));
        })?
        .error_for_status()
        .map_err(|e| {
            log_warn(&format!(
                "qdrant_hybrid_search status_error collection={} duration_ms={} err={e}",
                cfg.collection,
                search_start.elapsed().as_millis()
            ));
            anyhow::Error::from(e)
        })?;

    let parsed: QdrantQueryResponse = resp.json().await?;
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
    let client = http_client()?;
    let url = format!(
        "{}/collections/{}/points/query",
        qdrant_base(cfg),
        cfg.collection
    );

    let hnsw_ef = *HNSW_EF_SEARCH;
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
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .inspect_err(|e| {
            log_warn(&format!(
                "qdrant_named_dense_search transport_error collection={} duration_ms={} err={e}",
                cfg.collection,
                search_start.elapsed().as_millis()
            ));
        })?
        .error_for_status()
        .map_err(|e| {
            log_warn(&format!(
                "qdrant_named_dense_search status_error collection={} duration_ms={} err={e}",
                cfg.collection,
                search_start.elapsed().as_millis()
            ));
            anyhow::Error::from(e)
        })?;

    let parsed: QdrantQueryResponse = resp.json().await?;
    log_debug(&format!(
        "qdrant search_complete mode=named_dense collection={} hits={} latency_ms={}",
        cfg.collection,
        parsed.result.points.len(),
        search_start.elapsed().as_millis()
    ));
    Ok(parsed.result.points)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::core::config::Config;
    use crate::crates::vector::ops::sparse::compute_sparse_vector;
    use httpmock::prelude::*;

    fn make_search_response(hits: Vec<(&str, f64)>) -> serde_json::Value {
        let points: Vec<serde_json::Value> = hits
            .iter()
            .map(|(url, score)| {
                serde_json::json!({
                    "id": "test-id",
                    "score": score,
                    "payload": {"url": url, "chunk_text": "test chunk text"}
                })
            })
            .collect();
        serde_json::json!({"result": {"points": points}})
    }

    #[tokio::test]
    async fn qdrant_hybrid_search_sends_prefetch_rrf_query() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/query")
                    .json_body_includes(r#"{"query":{"fusion":"rrf"}}"#);
                then.status(200)
                    .json_body(make_search_response(vec![("https://example.com/a", 0.9)]));
            })
            .await;

        let mut cfg = Config::test_default();
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let dense = vec![0.1f32, 0.2, 0.3, 0.4];
        let sparse = compute_sparse_vector("hybrid search test");
        let result = qdrant_hybrid_search(&cfg, &dense, &sparse, 5, None, None).await;

        mock.assert_async().await;
        assert!(
            result.is_ok(),
            "hybrid search must succeed: {:?}",
            result.err()
        );
        let hits = result.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].payload.url, "https://example.com/a");
    }

    #[tokio::test]
    async fn qdrant_named_dense_search_uses_query_endpoint_with_dense_using() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/query")
                    .json_body_includes(r#"{"using":"dense"}"#);
                then.status(200).json_body(make_search_response(vec![(
                    "https://example.com/dense",
                    0.88,
                )]));
            })
            .await;

        let mut cfg = Config::test_default();
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let dense = vec![0.1f32, 0.2, 0.3, 0.4];
        let result = qdrant_named_dense_search(&cfg, &dense, 5, None).await;

        mock.assert_async().await;
        assert!(
            result.is_ok(),
            "named dense search must succeed: {:?}",
            result.err()
        );
        let hits = result.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].payload.url, "https://example.com/dense");
    }

    #[tokio::test]
    async fn qdrant_named_dense_search_propagates_error() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(POST).path("/collections/test_col/points/query");
                then.status(500).body("internal server error");
            })
            .await;

        let mut cfg = Config::test_default();
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let result = qdrant_named_dense_search(&cfg, &[0.1f32], 5, None).await;
        assert!(result.is_err(), "HTTP 500 must propagate as Err");
    }

    #[tokio::test]
    async fn qdrant_hybrid_search_propagates_qdrant_error() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(POST).path("/collections/test_col/points/query");
                then.status(500).body("internal server error");
            })
            .await;

        let mut cfg = Config::test_default();
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let result =
            qdrant_hybrid_search(&cfg, &[0.1f32], &SparseVector::default(), 5, None, None).await;
        assert!(result.is_err(), "HTTP 500 must propagate as Err");
    }

    #[tokio::test]
    async fn qdrant_hybrid_search_includes_filter_when_some() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/query")
                    .json_body_includes(r#"{"filter":{"must":[{"key":"scraped_at"}]}}"#);
                then.status(200)
                    .json_body(make_search_response(vec![("https://example.com/a", 0.9)]));
            })
            .await;

        let mut cfg = Config::test_default();
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let dense = vec![0.1f32, 0.2, 0.3, 0.4];
        let sparse = compute_sparse_vector("hybrid search test");
        let filter = serde_json::json!({
            "must": [{"key": "scraped_at", "range": {"gte": "2026-01-01T00:00:00+00:00"}}]
        });
        let result = qdrant_hybrid_search(&cfg, &dense, &sparse, 5, None, Some(&filter)).await;

        mock.assert_async().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn qdrant_hybrid_search_sends_hnsw_ef_on_dense_prefetch_arm() {
        let server = MockServer::start_async().await;
        // hnsw_ef and quantization params must be on the dense prefetch arm,
        // not at the top level -- the fusion stage doesn't do HNSW traversal.
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/query")
                    .json_body_includes(r#"{"prefetch":[{"params":{"hnsw_ef":128,"quantization":{"rescore":true}}}]}"#);
                then.status(200)
                    .json_body(make_search_response(vec![("https://example.com/a", 0.9)]));
            })
            .await;

        let mut cfg = Config::test_default();
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let dense = vec![0.1f32, 0.2, 0.3, 0.4];
        let sparse = compute_sparse_vector("hybrid hnsw ef test");
        let result = qdrant_hybrid_search(&cfg, &dense, &sparse, 5, None, None).await;

        mock.assert_async().await;
        assert!(
            result.is_ok(),
            "hybrid search must succeed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn qdrant_named_dense_search_sends_hnsw_ef_param() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/query")
                    .json_body_includes(r#"{"params":{"hnsw_ef":128}}"#);
                then.status(200).json_body(make_search_response(vec![(
                    "https://example.com/dense",
                    0.88,
                )]));
            })
            .await;

        let mut cfg = Config::test_default();
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let result = qdrant_named_dense_search(&cfg, &[0.1f32, 0.2, 0.3, 0.4], 5, None).await;

        mock.assert_async().await;
        assert!(
            result.is_ok(),
            "named dense search must succeed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn qdrant_named_dense_search_sends_quantization_rescore_param() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/query")
                    .json_body_includes(r#"{"params":{"quantization":{"rescore":true}}}"#);
                then.status(200).json_body(make_search_response(vec![(
                    "https://example.com/dense",
                    0.88,
                )]));
            })
            .await;

        let mut cfg = Config::test_default();
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let result = qdrant_named_dense_search(&cfg, &[0.1f32, 0.2, 0.3, 0.4], 5, None).await;

        mock.assert_async().await;
        assert!(
            result.is_ok(),
            "named dense search must succeed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn qdrant_hybrid_search_uses_candidates_from_config() {
        // Verify cfg.hybrid_search_candidates controls the prefetch window size.
        // json_body_includes does deep partial matching: the body must contain a prefetch
        // array where at least one arm has "limit": 77 and "using": "dense".
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/query")
                    .json_body_includes(r#"{"prefetch":[{"using":"dense","limit":77}]}"#);
                then.status(200)
                    .json_body(make_search_response(vec![("https://example.com/a", 0.9)]));
            })
            .await;

        let mut cfg = Config::test_default();
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();
        cfg.hybrid_search_candidates = 77;

        let dense = vec![0.1f32, 0.2, 0.3, 0.4];
        let sparse = compute_sparse_vector("hybrid search test");
        let result = qdrant_hybrid_search(&cfg, &dense, &sparse, 5, None, None).await;

        mock.assert_async().await;
        assert!(
            result.is_ok(),
            "hybrid search must succeed: {:?}",
            result.err()
        );
    }

    #[test]
    fn search_complete_log_format_is_valid() {
        let collection = "cortex";
        let hits = 10usize;
        let latency_ms = 42u128;
        let line = format!(
            "qdrant search_complete mode=hybrid collection={collection} hits={hits} latency_ms={latency_ms}"
        );
        assert!(line.contains("mode=hybrid"));
        assert!(line.contains("collection=cortex"));
        assert!(line.contains("hits=10"));
        assert!(line.contains("latency_ms=42"));
    }

    #[tokio::test]
    async fn qdrant_named_dense_search_includes_filter_when_some() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/query")
                    .json_body_includes(r#"{"filter":{"must":[{"key":"scraped_at"}]}}"#);
                then.status(200).json_body(make_search_response(vec![(
                    "https://example.com/dense",
                    0.88,
                )]));
            })
            .await;

        let mut cfg = Config::test_default();
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let filter = serde_json::json!({
            "must": [{"key": "scraped_at", "range": {"gte": "2026-01-01T00:00:00+00:00"}}]
        });
        let result =
            qdrant_named_dense_search(&cfg, &[0.1f32, 0.2, 0.3, 0.4], 5, Some(&filter)).await;

        mock.assert_async().await;
        assert!(result.is_ok());
    }
}
