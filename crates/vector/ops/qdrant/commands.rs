use crate::crates::core::config::Config;
use crate::crates::core::logging::log_warn;
use crate::crates::vector::ops::tei::qdrant_store::{VectorMode, get_or_fetch_vector_mode};
use futures_util::stream::{FuturesUnordered, StreamExt};
use std::collections::HashMap;
use std::error::Error;

use super::client::{
    qdrant_delete_points, qdrant_domain_facets, qdrant_retrieve_by_url, qdrant_scroll_pages,
    qdrant_url_facets,
};
use super::hybrid::{qdrant_hybrid_search, qdrant_named_dense_search};
use super::types::QdrantSearchHit;
use super::utils::{
    env_usize_clamped, payload_url, render_full_doc_from_points, retrieve_max_points,
};

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
) -> Result<Vec<QdrantSearchHit>, Box<dyn Error>> {
    let filter =
        super::filter::build_scraped_at_filter(cfg.since.as_deref(), cfg.before.as_deref());
    let filter_ref = filter.as_ref();
    let mode = get_or_fetch_vector_mode(cfg)
        .await
        .map_err(|e| -> Box<dyn Error> {
            format!(
                "vector mode probe failed for collection '{}' at '{}': {e}. \
             verify Qdrant is reachable and the collection exists",
                cfg.collection, cfg.qdrant_url
            )
            .into()
        })?;
    match mode {
        VectorMode::Named => {
            if cfg.hybrid_search_enabled {
                let sv = crate::crates::vector::ops::sparse::compute_sparse_vector(query);
                if !sv.is_empty() {
                    qdrant_hybrid_search(cfg, vector, &sv, limit, filter_ref)
                        .await
                        .map_err(|e| -> Box<dyn Error> {
                            format!("hybrid search on '{}' failed: {e}", cfg.collection).into()
                        })
                } else {
                    qdrant_named_dense_search(cfg, vector, limit, filter_ref)
                        .await
                        .map_err(|e| -> Box<dyn Error> {
                            format!("named dense search on '{}' failed: {e}", cfg.collection).into()
                        })
                }
            } else {
                qdrant_named_dense_search(cfg, vector, limit, filter_ref)
                    .await
                    .map_err(|e| -> Box<dyn Error> {
                        format!("named dense search on '{}' failed: {e}", cfg.collection).into()
                    })
            }
        }
        VectorMode::Unnamed => super::client::qdrant_search(cfg, vector, limit, filter_ref)
            .await
            .map_err(|e| -> Box<dyn Error> {
                format!("vector search on '{}' failed: {e}", cfg.collection).into()
            }),
    }
}

pub async fn retrieve_result(
    cfg: &Config,
    target: &str,
    max_points: Option<usize>,
) -> Result<(usize, String), Box<dyn Error>> {
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
) -> Result<serde_json::Value, Box<dyn Error>> {
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
) -> Result<serde_json::Value, Box<dyn Error>> {
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
pub async fn dedupe_payload(cfg: &Config) -> Result<serde_json::Value, Box<dyn Error>> {
    let mut by_key: HashMap<(String, i64), Vec<DedupeRecord>> = HashMap::new();
    qdrant_scroll_pages(cfg, |points| {
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
    })
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
