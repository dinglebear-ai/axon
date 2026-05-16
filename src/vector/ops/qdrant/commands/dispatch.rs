use crate::core::config::Config;
use crate::vector::ops::sparse::{self, SparseVector};
use crate::vector::ops::tei::qdrant_store::{VectorMode, get_or_fetch_vector_mode};
use std::error::Error;

use super::super::filter::{
    build_schema_version_filter, build_scraped_at_filter, combine_must_filters,
};
use super::super::hybrid::{qdrant_hybrid_search, qdrant_named_dense_search};
use super::super::search;
use super::super::types::QdrantSearchHit;
use super::super::utils::validate_config_collection;

/// Hard cap on retrieval-query length. `compute_sparse_vector` and `tei_embed` are
/// otherwise unbounded; a multi-MB query would reach both Qdrant and TEI.
/// Bounded to ~64 KiB which is well above any reasonable NL or keyword query
/// (CWE-770, bd axon_rust-d71.7 / H3).
const MAX_QUERY_LEN_BYTES: usize = 64 * 1024;

pub(crate) struct VectorSearchRequest<'a> {
    pub(crate) dense: &'a [f32],
    pub(crate) sparse: Option<SparseVector>,
    pub(crate) filter: Option<serde_json::Value>,
    pub(crate) query_len: usize,
    pub(crate) limit: usize,
    pub(crate) candidates_override: Option<usize>,
}

impl<'a> VectorSearchRequest<'a> {
    pub(crate) fn from_query(
        cfg: &Config,
        dense: &'a [f32],
        query: &str,
        limit: usize,
    ) -> Result<Self, Box<dyn Error + Send + Sync>> {
        if query.len() > MAX_QUERY_LEN_BYTES {
            return Err(format!(
                "query exceeds {MAX_QUERY_LEN_BYTES}-byte cap (got {} bytes); \
                 retrieval queries must be reasonably-sized natural-language or keyword input",
                query.len()
            )
            .into());
        }
        let filter = build_scraped_at_filter(cfg.since.as_deref(), cfg.before.as_deref())
            .map_err(|e| -> Box<dyn Error + Send + Sync> { e.into() })?;
        Ok(Self {
            dense,
            sparse: Some(sparse::compute_sparse_vector(query)),
            filter,
            query_len: query.len(),
            limit,
            candidates_override: None,
        })
    }

    pub(crate) fn with_candidates_override(mut self, candidates: Option<usize>) -> Self {
        self.candidates_override = candidates;
        self
    }

    /// Add an optional `payload_schema_version >= min` filter to the request.
    ///
    /// Composes with any existing filter (e.g. `scraped_at` range) via
    /// `combine_must_filters`. Default ask/query retrieval passes `None` and
    /// applies no version filter, preserving backward compatibility with
    /// pre-`axon_rust-lu6a` points that lack the `payload_schema_version`
    /// field. Opt-in callers (vertical-aware queries from `xvu9`) pass
    /// `Some(N)` to restrict to points indexed under schema version N or
    /// later.
    #[allow(dead_code)] // wired for xvu9 / future vertical-aware retrieval paths
    pub(crate) fn with_payload_schema_version_min(mut self, min: Option<u32>) -> Self {
        if let Some(version_filter) = build_schema_version_filter(min) {
            self.filter = match self.filter.take() {
                Some(existing) => Some(combine_must_filters(&[existing, version_filter])),
                None => Some(version_filter),
            };
        }
        self
    }
}

/// Dispatch vector search based on collection mode and hybrid config.
///
/// Named + hybrid enabled + non-empty sparse -> hybrid search (dense + BM42 + RRF)
/// Named + hybrid disabled or empty sparse  -> named dense-only search
/// Unnamed                                   -> legacy `/points/search`
///
/// Shared by both `query` and `ask` command paths to avoid duplicated routing logic.
#[tracing::instrument(
    name = "vector.dispatch",
    skip(cfg, vector, query),
    fields(collection = %cfg.collection, query_len = query.len(), limit)
)]
#[cfg(test)]
pub(crate) async fn dispatch_vector_search(
    cfg: &Config,
    vector: &[f32],
    query: &str,
    limit: usize,
) -> Result<Vec<QdrantSearchHit>, Box<dyn Error + Send + Sync>> {
    let req = VectorSearchRequest::from_query(cfg, vector, query, limit)?;
    dispatch_vector_search_request(cfg, &req).await
}

#[tracing::instrument(
    name = "vector.dispatch",
    skip(cfg, req),
    fields(collection = %cfg.collection, query_len = req.query_len, limit = req.limit)
)]
pub(crate) async fn dispatch_vector_search_request(
    cfg: &Config,
    req: &VectorSearchRequest<'_>,
) -> Result<Vec<QdrantSearchHit>, Box<dyn Error + Send + Sync>> {
    validate_config_collection(cfg).map_err(|e| -> Box<dyn Error + Send + Sync> { e.into() })?;
    let filter_ref = req.filter.as_ref();
    let mode =
        get_or_fetch_vector_mode(cfg)
            .await
            .map_err(|e| -> Box<dyn Error + Send + Sync> {
                format!(
                    "vector mode probe failed for collection '{}' at '{}': {e}. \
             verify Qdrant is reachable and the collection exists",
                    cfg.collection, cfg.qdrant_url
                )
                .into()
            })?;
    let started = std::time::Instant::now();
    let (arm, result): (
        &'static str,
        Result<Vec<QdrantSearchHit>, Box<dyn Error + Send + Sync>>,
    ) = match mode {
        VectorMode::Named => {
            if cfg.hybrid_search_enabled {
                let sv = req.sparse.as_ref().cloned().unwrap_or_default();
                if !sv.is_empty() {
                    (
                        "hybrid_rrf",
                        qdrant_hybrid_search(
                            cfg,
                            req.dense,
                            &sv,
                            req.limit,
                            req.candidates_override,
                            filter_ref,
                        )
                        .await
                        .map_err(|e| -> Box<dyn Error + Send + Sync> {
                            format!("hybrid search on '{}' failed: {e}", cfg.collection).into()
                        }),
                    )
                } else {
                    (
                        "named_dense_empty_sparse",
                        qdrant_named_dense_search(cfg, req.dense, req.limit, filter_ref)
                            .await
                            .map_err(|e| -> Box<dyn Error + Send + Sync> {
                                format!("named dense search on '{}' failed: {e}", cfg.collection)
                                    .into()
                            }),
                    )
                }
            } else {
                (
                    "named_dense",
                    qdrant_named_dense_search(cfg, req.dense, req.limit, filter_ref)
                        .await
                        .map_err(|e| -> Box<dyn Error + Send + Sync> {
                            format!("named dense search on '{}' failed: {e}", cfg.collection).into()
                        }),
                )
            }
        }
        VectorMode::Unnamed => (
            "unnamed_dense",
            search::qdrant_search(cfg, req.dense, req.limit, filter_ref)
                .await
                .map_err(|e| -> Box<dyn Error + Send + Sync> {
                    format!("vector search on '{}' failed: {e}", cfg.collection).into()
                }),
        ),
    };
    let latency_ms = started.elapsed().as_millis();
    match &result {
        Ok(hits) => {
            // Score-distribution telemetry — operators can grep these to detect
            // threshold no-op (top-1 score way below `ask_min_relevance_score`)
            // and arm-divergence (RRF vs cosine score scales differ). The
            // numbers are inherently arm-dependent: cosine sits in [0, 1],
            // RRF fusion tends to land in much smaller magnitudes.
            // (bd axon_rust-d71.31 / M-OBS-2)
            let top1 = hits.first().map(|h| h.score);
            let top10_avg = if hits.is_empty() {
                None
            } else {
                let n = hits.len().min(10);
                Some(hits.iter().take(n).map(|h| h.score).sum::<f64>() / n as f64)
            };
            tracing::debug!(
                arm,
                collection = %cfg.collection,
                latency_ms,
                hits = hits.len(),
                limit = req.limit,
                top1_score = top1,
                top10_avg_score = top10_avg,
                "vector dispatch ok"
            );
        }
        Err(err) => tracing::warn!(
            arm,
            collection = %cfg.collection,
            latency_ms,
            error = %err,
            "vector dispatch failed"
        ),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::Config;
    use crate::vector::ops::qdrant::utils::validate_collection_name;
    use httpmock::prelude::*;

    fn named_collection_body() -> serde_json::Value {
        serde_json::json!({
            "result": {
                "config": {
                    "params": {
                        "vectors": {
                            "dense": {"size": 4, "distance": "Cosine"}
                        },
                        "sparse_vectors": {
                            "bm42": {"modifier": "idf"}
                        }
                    }
                }
            }
        })
    }

    fn unnamed_collection_body() -> serde_json::Value {
        serde_json::json!({
            "result": {
                "config": {
                    "params": {
                        "vectors": {"size": 4, "distance": "Cosine"}
                    }
                }
            }
        })
    }

    fn query_response(url: &str, score: f64) -> serde_json::Value {
        serde_json::json!({
            "result": {
                "points": [
                    {"id": "hit", "score": score, "payload": {"url": url, "chunk_text": "chunk"}}
                ]
            }
        })
    }

    fn search_response(url: &str, score: f64) -> serde_json::Value {
        serde_json::json!({
            "result": [
                {"id": "hit", "score": score, "payload": {"url": url, "chunk_text": "chunk"}}
            ]
        })
    }

    #[test]
    fn collection_name_accepts_legal_values() {
        assert!(validate_collection_name("cortex").is_ok());
        assert!(validate_collection_name("axon_v2").is_ok());
        assert!(validate_collection_name("my-collection").is_ok());
        assert!(validate_collection_name("a.b.c").is_ok());
        assert!(validate_collection_name("a").is_ok());
    }

    #[test]
    fn collection_name_rejects_path_traversal() {
        assert!(validate_collection_name("..").is_err());
        assert!(validate_collection_name("../etc/passwd").is_err());
        assert!(validate_collection_name("a/b").is_err());
        assert!(validate_collection_name("a..b").is_err());
        assert!(validate_collection_name(".hidden").is_err());
        assert!(validate_collection_name("trailing.").is_err());
    }

    #[test]
    fn collection_name_rejects_url_delimiters() {
        assert!(validate_collection_name("a?x=1").is_err());
        assert!(validate_collection_name("a#frag").is_err());
        assert!(validate_collection_name("a b").is_err());
        assert!(validate_collection_name("a%2e%2e").is_err());
    }

    #[test]
    fn collection_name_rejects_empty_and_oversize() {
        assert!(validate_collection_name("").is_err());
        let huge = "a".repeat(256);
        assert!(validate_collection_name(&huge).is_err());
    }

    #[tokio::test]
    async fn dispatch_rejects_invalid_collection_name() {
        let mut cfg = Config::test_default();
        cfg.collection = "../etc/passwd".to_string();
        let vec = vec![0.0f32; 4];
        let err = dispatch_vector_search(&cfg, &vec, "ok", 5)
            .await
            .expect_err("path-traversal collection name must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("invalid collection name"),
            "error should mention invalid collection: {msg}"
        );
    }

    #[tokio::test]
    async fn dispatch_rejects_query_over_max_len() {
        let cfg = Config::test_default();
        let huge = "a".repeat(MAX_QUERY_LEN_BYTES + 1);
        let vec = vec![0.0f32; 4];
        let err = dispatch_vector_search(&cfg, &vec, &huge, 5)
            .await
            .expect_err("query over cap must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("64-byte cap")
                || msg.contains("cap")
                || msg.contains(&MAX_QUERY_LEN_BYTES.to_string()),
            "error should mention the cap: {msg}"
        );
    }

    #[tokio::test]
    async fn dispatch_accepts_query_at_max_len() {
        // We can't actually run the search without a Qdrant mock, but we can confirm
        // the length guard does not trip at the boundary.
        let cfg = Config::test_default();
        let at_cap = "a".repeat(MAX_QUERY_LEN_BYTES);
        let vec = vec![0.0f32; 4];
        let res = dispatch_vector_search(&cfg, &vec, &at_cap, 5).await;
        // Either succeeds (impossible without a mock) or fails for a downstream reason
        // (vector mode probe / network) — but the failure must NOT be the length cap.
        if let Err(e) = res {
            let msg = e.to_string();
            assert!(
                !msg.contains("cap"),
                "boundary-length query must not trip the length cap: {msg}"
            );
        }
    }

    #[tokio::test]
    async fn dispatch_routes_named_with_sparse_to_hybrid() {
        let server = MockServer::start_async().await;
        let collection = "dispatch_named_sparse_hybrid";

        server
            .mock_async(|when, then| {
                when.method(GET).path(format!("/collections/{collection}"));
                then.status(200).json_body(named_collection_body());
            })
            .await;
        let hybrid = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path(format!("/collections/{collection}/points/query"))
                    .json_body_includes(r#"{"query":{"fusion":"rrf"}}"#);
                then.status(200)
                    .json_body(query_response("https://example.com/hybrid", 0.666));
            })
            .await;
        let legacy = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path(format!("/collections/{collection}/points/search"));
                then.status(500);
            })
            .await;

        let mut cfg = Config::test_default();
        cfg.qdrant_url = server.base_url();
        cfg.collection = collection.to_string();
        cfg.hybrid_search_enabled = true;

        let vector = vec![0.1, 0.2, 0.3, 0.4];
        let request =
            VectorSearchRequest::from_query(&cfg, &vector, "hybrid retrieval pipeline", 5)
                .expect("request builds");
        let hits = dispatch_vector_search_request(&cfg, &request)
            .await
            .expect("hybrid dispatch succeeds");

        assert_eq!(hits[0].payload.url, "https://example.com/hybrid");
        assert_eq!(hybrid.calls_async().await, 1);
        assert_eq!(legacy.calls_async().await, 0);
    }

    #[tokio::test]
    async fn dispatch_routes_named_empty_sparse_to_named_dense() {
        let server = MockServer::start_async().await;
        let collection = "dispatch_named_empty_sparse";

        server
            .mock_async(|when, then| {
                when.method(GET).path(format!("/collections/{collection}"));
                then.status(200).json_body(named_collection_body());
            })
            .await;
        let named_dense = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path(format!("/collections/{collection}/points/query"))
                    .json_body_includes(r#"{"using":"dense"}"#);
                then.status(200)
                    .json_body(query_response("https://example.com/named-dense", 0.88));
            })
            .await;

        let mut cfg = Config::test_default();
        cfg.qdrant_url = server.base_url();
        cfg.collection = collection.to_string();
        cfg.hybrid_search_enabled = true;

        let vector = vec![0.1, 0.2, 0.3, 0.4];
        let request =
            VectorSearchRequest::from_query(&cfg, &vector, "of to in", 5).expect("request builds");
        assert!(request.sparse.as_ref().is_none_or(|sv| sv.is_empty()));

        let hits = dispatch_vector_search_request(&cfg, &request)
            .await
            .expect("named dense dispatch succeeds");

        assert_eq!(hits[0].payload.url, "https://example.com/named-dense");
        assert_eq!(named_dense.calls_async().await, 1);
    }

    #[tokio::test]
    async fn dispatch_routes_unnamed_to_legacy_search() {
        let server = MockServer::start_async().await;
        let collection = "dispatch_unnamed_legacy";

        server
            .mock_async(|when, then| {
                when.method(GET).path(format!("/collections/{collection}"));
                then.status(200).json_body(unnamed_collection_body());
            })
            .await;
        let legacy = server
            .mock_async(|when, then| {
                when.method(POST)
                    .path(format!("/collections/{collection}/points/search"));
                then.status(200)
                    .json_body(search_response("https://example.com/legacy", 0.77));
            })
            .await;

        let mut cfg = Config::test_default();
        cfg.qdrant_url = server.base_url();
        cfg.collection = collection.to_string();
        cfg.hybrid_search_enabled = true;

        let vector = vec![0.1, 0.2, 0.3, 0.4];
        let request =
            VectorSearchRequest::from_query(&cfg, &vector, "hybrid retrieval pipeline", 5)
                .expect("request builds");
        let hits = dispatch_vector_search_request(&cfg, &request)
            .await
            .expect("legacy dispatch succeeds");

        assert_eq!(hits[0].payload.url, "https://example.com/legacy");
        assert_eq!(legacy.calls_async().await, 1);
    }
}
