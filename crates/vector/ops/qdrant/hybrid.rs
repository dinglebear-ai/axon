//! Hybrid search via Qdrant `/query` endpoint with RRF fusion.
//!
//! Sends two prefetch arms (dense + BM42 sparse) and fuses with Reciprocal Rank Fusion.
//! Only called for collections in `VectorMode::Named` (named dense + sparse vectors).

use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use crate::crates::core::logging::{log_debug, log_warn};
use crate::crates::vector::ops::sparse::SparseVector;
use anyhow::{Result, anyhow};
use std::time::Instant;

use super::types::{QdrantSearchHit, QdrantSearchResponse};
use super::utils::qdrant_base;

/// Perform hybrid search using dense + BM42 sparse prefetch with RRF fusion.
///
/// Issues a single POST to `/collections/{name}/points/query` with two `prefetch` arms
/// (one dense, one sparse) and `"query": {"fusion": "rrf"}` to combine them.
/// `limit` is the final number of results after fusion. Each prefetch arm fetches
/// `cfg.hybrid_search_candidates` candidates before RRF fusion. Requires a Named-mode collection.
pub(crate) async fn qdrant_hybrid_search(
    cfg: &Config,
    dense_vector: &[f32],
    sparse_vector: &SparseVector,
    limit: usize,
) -> Result<Vec<QdrantSearchHit>> {
    let client = http_client()?;
    let url = format!(
        "{}/collections/{}/points/query",
        qdrant_base(cfg),
        cfg.collection
    );

    let candidates = cfg.hybrid_search_candidates.max(limit);

    let body = serde_json::json!({
        "prefetch": [
            {
                "query": dense_vector,
                "using": "dense",
                "limit": candidates
            },
            {
                "query": sparse_vector.to_json(),
                "using": "bm42",
                "limit": candidates
            }
        ],
        "query": {"fusion": "rrf"},
        "limit": limit,
        "with_payload": true,
        "with_vector": false
    });

    let search_start = Instant::now();
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            log_warn(&format!(
                "qdrant_hybrid_search transport_error collection={} duration_ms={} err={e}",
                cfg.collection,
                search_start.elapsed().as_millis()
            ));
            anyhow!(e.to_string())
        })?
        .error_for_status()
        .map_err(|e| {
            log_warn(&format!(
                "qdrant_hybrid_search status_error collection={} duration_ms={} err={e}",
                cfg.collection,
                search_start.elapsed().as_millis()
            ));
            anyhow!(e.to_string())
        })?;

    let parsed: QdrantSearchResponse = resp.json().await?;
    log_debug(&format!(
        "qdrant hybrid_search hits={} collection={}",
        parsed.result.len(),
        cfg.collection
    ));
    Ok(parsed.result)
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
pub(crate) async fn qdrant_named_dense_search(
    cfg: &Config,
    dense_vector: &[f32],
    limit: usize,
) -> Result<Vec<QdrantSearchHit>> {
    let client = http_client()?;
    let url = format!(
        "{}/collections/{}/points/query",
        qdrant_base(cfg),
        cfg.collection
    );

    let body = serde_json::json!({
        "query": dense_vector,
        "using": "dense",
        "limit": limit,
        "with_payload": true,
        "with_vector": false
    });

    let search_start = Instant::now();
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| {
            log_warn(&format!(
                "qdrant_named_dense_search transport_error collection={} duration_ms={} err={e}",
                cfg.collection,
                search_start.elapsed().as_millis()
            ));
            anyhow!(e.to_string())
        })?
        .error_for_status()
        .map_err(|e| {
            log_warn(&format!(
                "qdrant_named_dense_search status_error collection={} duration_ms={} err={e}",
                cfg.collection,
                search_start.elapsed().as_millis()
            ));
            anyhow!(e.to_string())
        })?;

    let parsed: QdrantSearchResponse = resp.json().await?;
    log_debug(&format!(
        "qdrant named_dense_search hits={} collection={}",
        parsed.result.len(),
        cfg.collection
    ));
    Ok(parsed.result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::jobs::common::test_config;
    use crate::crates::vector::ops::sparse::compute_sparse_vector;
    use httpmock::prelude::*;

    fn make_search_response(hits: Vec<(&str, f64)>) -> serde_json::Value {
        let result: Vec<serde_json::Value> = hits
            .iter()
            .map(|(url, score)| {
                serde_json::json!({
                    "id": "test-id",
                    "score": score,
                    "payload": {"url": url, "chunk_text": "test chunk text"}
                })
            })
            .collect();
        serde_json::json!({"result": result})
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

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let dense = vec![0.1f32, 0.2, 0.3, 0.4];
        let sparse = compute_sparse_vector("hybrid search test");
        let result = qdrant_hybrid_search(&cfg, &dense, &sparse, 5).await;

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

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let dense = vec![0.1f32, 0.2, 0.3, 0.4];
        let result = qdrant_named_dense_search(&cfg, &dense, 5).await;

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

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let result = qdrant_named_dense_search(&cfg, &[0.1f32], 5).await;
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

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let result = qdrant_hybrid_search(&cfg, &[0.1f32], &SparseVector::default(), 5).await;
        assert!(result.is_err(), "HTTP 500 must propagate as Err");
    }
}
