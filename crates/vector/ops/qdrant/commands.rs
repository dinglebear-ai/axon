use crate::crates::core::config::Config;
use crate::crates::core::logging::log_warn;
use crate::crates::vector::ops::tei::qdrant_store::{VectorMode, get_or_fetch_vector_mode};
use futures_util::stream::{FuturesUnordered, StreamExt};
use std::collections::HashMap;
use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};

static DEDUPE_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// RAII guard that resets DEDUPE_IN_PROGRESS to false when dropped.
/// Ensures the flag is cleared even if dedupe_payload returns an error.
struct DedupeGuard;

impl Drop for DedupeGuard {
    fn drop(&mut self) {
        DEDUPE_IN_PROGRESS.store(false, Ordering::Release);
    }
}

use super::client::{
    qdrant_delete_points, qdrant_domain_facets, qdrant_retrieve_by_url,
    qdrant_scroll_pages_selective, qdrant_url_facets,
};
use super::hybrid::{qdrant_hybrid_search, qdrant_named_dense_search};
use super::types::QdrantSearchHit;
use super::utils::{
    env_usize_clamped, payload_url, render_full_doc_from_points, retrieve_max_points,
};

/// Hard cap on retrieval-query length. `compute_sparse_vector` and `tei_embed` are
/// otherwise unbounded; a multi-MB query would reach both Qdrant and TEI.
/// Bounded to ~64 KiB which is well above any reasonable NL or keyword query
/// (CWE-770, bd axon_rust-d71.7 / H3).
const MAX_QUERY_LEN_BYTES: usize = 64 * 1024;

/// Validate a Qdrant collection name against URL-injection / path-traversal.
///
/// Allows alphanumerics, underscore, hyphen, and dot; rejects path separators,
/// query/fragment delimiters, leading/trailing dots, and `..`. Length capped
/// at 255. Called at every dispatch entry — see crates/vector/ops/qdrant/hybrid.rs
/// and crates/vector/ops/qdrant/commands.rs which interpolate the name into URLs
/// without percent-encoding (CWE-22, bd axon_rust-d71.6 / H2).
fn validate_collection_name(name: &str) -> Result<(), &'static str> {
    if name.is_empty() {
        return Err("empty");
    }
    if name.len() > 255 {
        return Err("exceeds 255 characters");
    }
    if name == "." || name == ".." || name.starts_with('.') || name.ends_with('.') {
        return Err("leading/trailing dot or path component");
    }
    if name.contains("..") {
        return Err("contains '..'");
    }
    for c in name.chars() {
        let ok = c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.');
        if !ok {
            return Err("contains a character outside [A-Za-z0-9_.-]");
        }
    }
    Ok(())
}

/// Dispatch vector search based on collection mode and hybrid config.
///
/// Named + hybrid enabled + non-empty sparse -> hybrid search (dense + BM42 + RRF)
/// Named + hybrid disabled or empty sparse  -> named dense-only search
/// Unnamed                                   -> legacy `/points/search`
///
/// Shared by both `query` and `ask` command paths to avoid duplicated routing logic.
pub(crate) async fn dispatch_vector_search(
    cfg: &Config,
    vector: &[f32],
    query: &str,
    limit: usize,
) -> Result<Vec<QdrantSearchHit>, Box<dyn Error + Send + Sync>> {
    if let Err(reason) = validate_collection_name(&cfg.collection) {
        return Err(format!(
            "invalid collection name {:?}: {reason} (CWE-22 path injection guard)",
            cfg.collection
        )
        .into());
    }
    if query.len() > MAX_QUERY_LEN_BYTES {
        return Err(format!(
            "query exceeds {MAX_QUERY_LEN_BYTES}-byte cap (got {} bytes); \
             retrieval queries must be reasonably-sized natural-language or keyword input",
            query.len()
        )
        .into());
    }
    let filter =
        super::filter::build_scraped_at_filter(cfg.since.as_deref(), cfg.before.as_deref())
            .map_err(|e| -> Box<dyn Error + Send + Sync> { e.into() })?;
    let filter_ref = filter.as_ref();
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
                let sv = crate::crates::vector::ops::sparse::compute_sparse_vector(query);
                if !sv.is_empty() {
                    (
                        "hybrid_rrf",
                        qdrant_hybrid_search(cfg, vector, &sv, limit, filter_ref)
                            .await
                            .map_err(|e| -> Box<dyn Error + Send + Sync> {
                                format!("hybrid search on '{}' failed: {e}", cfg.collection).into()
                            }),
                    )
                } else {
                    (
                        "named_dense_empty_sparse",
                        qdrant_named_dense_search(cfg, vector, limit, filter_ref)
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
                    qdrant_named_dense_search(cfg, vector, limit, filter_ref)
                        .await
                        .map_err(|e| -> Box<dyn Error + Send + Sync> {
                            format!("named dense search on '{}' failed: {e}", cfg.collection).into()
                        }),
                )
            }
        }
        VectorMode::Unnamed => (
            "unnamed_dense",
            super::search::qdrant_search(cfg, vector, limit, filter_ref)
                .await
                .map_err(|e| -> Box<dyn Error + Send + Sync> {
                    format!("vector search on '{}' failed: {e}", cfg.collection).into()
                }),
        ),
    };
    let latency_ms = started.elapsed().as_millis();
    match &result {
        Ok(hits) => tracing::debug!(
            arm,
            collection = %cfg.collection,
            latency_ms,
            hits = hits.len(),
            limit,
            "vector dispatch ok"
        ),
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

pub async fn retrieve_result(
    cfg: &Config,
    target: &str,
    max_points: Option<usize>,
) -> Result<(usize, String), Box<dyn Error + Send + Sync>> {
    let max_points = retrieve_max_points(max_points);
    let candidates = crate::crates::vector::ops::input::url_lookup_candidates(target);

    let mut lookups: FuturesUnordered<_> = candidates
        .iter()
        .map(|candidate| qdrant_retrieve_by_url(cfg, candidate, Some(max_points)))
        .collect();

    let mut points = Vec::new();
    let mut had_success = false;
    let mut first_error: Option<String> = None;
    while let Some(result) = lookups.next().await {
        match result {
            Ok(candidate_points) => {
                had_success = true;
                if !candidate_points.is_empty() {
                    points = candidate_points;
                    break;
                }
            }
            Err(err) => {
                if first_error.is_none() {
                    first_error = Some(err.to_string());
                }
                log_warn(&format!(
                    "retrieve variant lookup failed for {}: {err}",
                    target
                ));
            }
        }
    }
    if points.is_empty()
        && !had_success
        && let Some(err) = first_error
    {
        return Err(format!("retrieve failed for all URL variants: {err}").into());
    }
    if points.is_empty() {
        return Ok((0, String::new()));
    }
    let chunk_count = points.len();
    let out = render_full_doc_from_points(points);
    Ok((chunk_count, out))
}

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

struct DedupeRecord {
    id: String,
    scraped_at: String,
}

/// Remove duplicate points that share the same (url, chunk_index) key.
///
/// **Performance**: O(n) full collection scroll -- on large collections (millions of
/// points) this can take 60-120+ seconds. This is inherent to deduplication and
/// cannot be replaced with a facet query.
pub async fn dedupe_payload(
    cfg: &Config,
) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    // Prevent concurrent deduplication runs — two simultaneous full-collection
    // scrolls race on deletes and produce misleading duplicate counts.
    if DEDUPE_IN_PROGRESS
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        return Err("deduplication already in progress for this process".into());
    }
    let _guard = DedupeGuard;

    let mut by_key: HashMap<(String, i64), Vec<DedupeRecord>> = HashMap::new();
    // Selective payload: only fetch the fields needed for dedup (url, chunk_index,
    // scraped_at). Avoids transferring multi-KB chunk_text per point — ~28x less
    // data on a 7M-point collection.
    qdrant_scroll_pages_selective(
        cfg,
        serde_json::json!({"include": ["url", "chunk_index", "scraped_at"]}),
        |points| {
            for p in points {
                let id = p
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                if id.is_empty() {
                    continue;
                }
                let Some(payload) = p.get("payload") else {
                    continue;
                };
                let url = payload_url(payload);
                if url.is_empty() {
                    continue;
                }
                let chunk_index = payload
                    .get("chunk_index")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let scraped_at = payload
                    .get("scraped_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                by_key
                    .entry((url, chunk_index))
                    .or_default()
                    .push(DedupeRecord { id, scraped_at });
            }
            true
        },
    )
    .await?;

    let mut to_delete: Vec<String> = Vec::new();
    let mut dup_groups = 0usize;
    for mut records in by_key.into_values() {
        if records.len() <= 1 {
            continue;
        }
        dup_groups += 1;
        records.sort_unstable_by(|a, b| b.scraped_at.cmp(&a.scraped_at));
        to_delete.extend(records.into_iter().skip(1).map(|r| r.id));
    }

    let deleted = qdrant_delete_points(cfg, &to_delete).await?;

    Ok(serde_json::json!({
        "duplicate_groups": dup_groups,
        "deleted": deleted,
        "collection": cfg.collection,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crates::core::config::Config;

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
}
