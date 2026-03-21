//! Legacy dense-only search for Unnamed collections.
//!
//! Unnamed collections (created before named-vector support) use `/points/search`
//! with a flat `"vector"` field. Named collections use `/points/query` via
//! [`hybrid`](super::hybrid).

use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use crate::crates::core::logging::{log_debug, log_warn};
use anyhow::Result;
use std::time::Instant;

use super::types::{QdrantSearchHit, QdrantSearchResponse};
use super::utils::{env_usize_clamped, qdrant_base};

/// Dense-only vector search for Unnamed (legacy) collections.
///
/// Issues a POST to `/collections/{name}/points/search` with a flat `"vector"` field.
/// Named collections must use [`qdrant_hybrid_search`](super::hybrid::qdrant_hybrid_search)
/// or [`qdrant_named_dense_search`](super::hybrid::qdrant_named_dense_search) instead.
///
/// `hnsw_ef` is read from `AXON_HNSW_EF_SEARCH_LEGACY` (default 64, clamped [32, 512]).
/// The `quantization.rescore` field in `params` is harmless for collections without
/// quantization configured — Qdrant ignores it silently.
pub(crate) async fn qdrant_search(
    cfg: &Config,
    vector: &[f32],
    limit: usize,
    filter: Option<&serde_json::Value>,
) -> Result<Vec<QdrantSearchHit>> {
    let client = http_client()?;
    let url = format!(
        "{}/collections/{}/points/search",
        qdrant_base(cfg),
        cfg.collection
    );
    let hnsw_ef = env_usize_clamped("AXON_HNSW_EF_SEARCH_LEGACY", 64, 32, 512);
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
    let res = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .inspect_err(|e| {
            log_warn(&format!(
                "qdrant_search failed collection={} duration_ms={} error={e}",
                cfg.collection,
                search_start.elapsed().as_millis()
            ));
        })?
        .error_for_status()
        .map_err(|e| {
            log_warn(&format!(
                "qdrant_search failed collection={} duration_ms={} error={e}",
                cfg.collection,
                search_start.elapsed().as_millis()
            ));
            anyhow::Error::from(e)
        })?
        .json::<QdrantSearchResponse>()
        .await?;
    log_debug(&format!(
        "qdrant search hits={} collection={}",
        res.result.len(),
        cfg.collection
    ));
    Ok(res.result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::jobs::common::test_config;
    use httpmock::prelude::*;

    fn make_search_response(hits: Vec<(&str, f64)>) -> serde_json::Value {
        let result: Vec<serde_json::Value> = hits
            .iter()
            .map(|(url, score)| {
                serde_json::json!({
                    "id": "test-id",
                    "score": score,
                    "payload": {"url": url, "chunk_text": "test chunk"}
                })
            })
            .collect();
        serde_json::json!({"result": result})
    }

    #[tokio::test]
    async fn qdrant_search_sends_hnsw_ef_param() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/search")
                    .json_body_includes(r#"{"params":{"hnsw_ef":64}}"#);
                then.status(200).json_body(make_search_response(vec![(
                    "https://example.com/legacy",
                    0.85,
                )]));
            })
            .await;

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let result = qdrant_search(&cfg, &[0.1f32, 0.2, 0.3, 0.4], 5, None).await;

        mock.assert_async().await;
        assert!(
            result.is_ok(),
            "qdrant_search must succeed: {:?}",
            result.err()
        );
        assert_eq!(result.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn qdrant_search_sends_quantization_rescore_param() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/search")
                    .json_body_includes(r#"{"params":{"quantization":{"rescore":true}}}"#);
                then.status(200)
                    .json_body(make_search_response(vec![("https://example.com/x", 0.77)]));
            })
            .await;

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let result = qdrant_search(&cfg, &[0.1f32, 0.2, 0.3, 0.4], 5, None).await;

        mock.assert_async().await;
        assert!(
            result.is_ok(),
            "rescore param test must succeed: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn qdrant_search_propagates_filter_when_some() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/search")
                    .json_body_includes(r#"{"filter":{"must":[{"key":"domain"}]}}"#);
                then.status(200)
                    .json_body(make_search_response(vec![("https://example.com/f", 0.80)]));
            })
            .await;

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let filter = serde_json::json!({
            "must": [{"key": "domain", "match": {"value": "example.com"}}]
        });
        let result = qdrant_search(&cfg, &[0.1f32, 0.2, 0.3, 0.4], 5, Some(&filter)).await;

        mock.assert_async().await;
        assert!(
            result.is_ok(),
            "filter propagation must succeed: {:?}",
            result.err()
        );
        assert_eq!(result.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn qdrant_search_propagates_http_error() {
        let server = MockServer::start_async().await;
        server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/search");
                then.status(500).body("internal error");
            })
            .await;

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let result = qdrant_search(&cfg, &[0.1f32], 5, None).await;
        assert!(result.is_err(), "HTTP 500 must propagate as Err");
    }

    #[tokio::test]
    async fn qdrant_search_sends_oversampling_param() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path("/collections/test_col/points/search")
                    .json_body_includes(r#"{"params":{"quantization":{"oversampling":1.5}}}"#);
                then.status(200)
                    .json_body(make_search_response(vec![("https://example.com/x", 0.77)]));
            })
            .await;

        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = server.base_url();
        cfg.collection = "test_col".to_string();

        let result = qdrant_search(&cfg, &[0.1f32, 0.2, 0.3, 0.4], 5, None).await;

        mock.assert_async().await;
        assert!(
            result.is_ok(),
            "oversampling param test must succeed: {:?}",
            result.err()
        );
    }
}
