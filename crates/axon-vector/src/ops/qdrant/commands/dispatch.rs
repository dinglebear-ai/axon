use crate::ops::sparse::{self, SparseVector};
use crate::ops::tei::qdrant_store::{VectorMode, get_or_fetch_vector_mode};
use axon_core::config::Config;
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

pub struct VectorSearchRequest<'a> {
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

    pub(crate) fn with_filter(mut self, filter: serde_json::Value) -> Self {
        self.filter = match self.filter.take() {
            Some(existing) => Some(combine_must_filters(&[existing, filter])),
            None => Some(filter),
        };
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
pub async fn dispatch_vector_search_request(
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
#[path = "dispatch_tests.rs"]
mod tests;
