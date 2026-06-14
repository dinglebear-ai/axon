use crate::core::config::Config;
use crate::core::logging::log_warn;
use crate::vector::cache::{
    DocCache, DocCacheConfig, DocCacheKey, current_generation, doc_cache_for_config,
};
use crate::vector::ops::qdrant;
use anyhow::Result;
use futures_util::stream::{self, StreamExt};
use std::sync::Arc;

/// The element type returned per fetched document.  The points are wrapped in
/// `Arc` so that cache-hits share the allocation without cloning the entire
/// `Vec<QdrantPoint>`.
pub type FetchedDoc = (usize, String, Arc<Vec<qdrant::QdrantPoint>>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullDocFetchError {
    pub url: String,
    pub error: String,
}

#[derive(Debug, Default)]
pub struct FetchedDocsResult {
    pub docs: Vec<FetchedDoc>,
    pub errors: Vec<FullDocFetchError>,
}

pub async fn fetch_full_docs(
    cfg: &Config,
    reranked: &[crate::vector::ops::ranking::AskCandidate],
    top_full_doc_indices: &[usize],
    context_char_count: usize,
    max_context_chars: usize,
    doc_chunk_limit: usize,
    doc_fetch_concurrency: usize,
) -> Result<FetchedDocsResult> {
    let mut fetched_docs = Vec::new();
    let mut errors = Vec::new();
    if context_char_count >= max_context_chars {
        return Ok(FetchedDocsResult::default());
    }

    // Fast path: when the doc cache is not in use (CLI one-shots, serve mode
    // with cache disabled) and there are multiple URLs to fetch, send all URL
    // filters in one /points/query/batch request instead of N sequential
    // /points/scroll calls. Qdrant guarantees positional alignment of
    // results, so no reordering is needed before returning. Falls back to
    // the buffer_unordered path on any transport or parse error. (bd axon_rust-cmm)
    let cache_enabled = cfg.ask_cache_enabled;
    if !cache_enabled && top_full_doc_indices.len() > 1 {
        let urls: Vec<String> = top_full_doc_indices
            .iter()
            .copied()
            .map(|doc_idx| reranked[doc_idx].url.clone())
            .collect();
        match qdrant::qdrant_batch_retrieve_by_urls(cfg, &urls, Some(doc_chunk_limit)).await {
            Ok(results) => {
                for (order, (doc_idx, points)) in top_full_doc_indices
                    .iter()
                    .copied()
                    .zip(results)
                    .enumerate()
                {
                    let url = reranked[doc_idx].url.clone();
                    if points.is_empty() {
                        record_empty_doc_fetch(&mut errors, url);
                    } else {
                        fetched_docs.push((order, url, Arc::new(points)));
                    }
                }
                fetched_docs.sort_by_key(|(order, _, _)| *order);
                return Ok(FetchedDocsResult {
                    docs: fetched_docs,
                    errors,
                });
            }
            Err(e) => {
                log_warn(&format!(
                    "ask: batch doc fetch failed ({e}), falling back to concurrent per-URL scroll"
                ));
            }
        }
    }

    // PERF-H1: borrow `cfg` directly into the per-item futures instead of
    // deep-cloning the 208-field `Config` per ask. `buffer_unordered` (unlike
    // `tokio::spawn`) does NOT require its futures to be `'static` — they run on
    // `fetch_stream.next().await` within this same `async fn`, so the borrow of
    // `cfg` stays alive for the whole stream. `&Config` is `Copy`, so each
    // `move` closure copies the shared reference rather than cloning the struct.
    let cfg: &Config = cfg;
    // Cache enable gate. The cache itself is process-global; we only consult
    // it when `cfg.ask_cache_enabled`. Snapshot the per-collection generation
    // once for this fetch batch so all concurrent lookups in this stream see
    // a consistent key. (axon_rust-pmc)
    let doc_cache = cache_enabled.then(|| ask_doc_cache(cfg));
    let collection = cfg.collection.clone();
    let generation = if cache_enabled {
        current_generation(&collection)
    } else {
        0
    };
    // Collect owned `(order, doc_idx)` pairs before mapping to async tasks so
    // the map closure receives `(usize, usize)` (no lifetime-parameterised
    // `&usize`).  The reference pattern `|(order, &doc_idx)|` or even receiving
    // `(usize, &usize)` causes an HRTB `FnOnce` diagnostic when the resulting
    // future is verified for `Send + 'static` by `tokio::spawn`.
    let tasks: Vec<(usize, usize)> = top_full_doc_indices.iter().copied().enumerate().collect();
    let mut fetch_stream = stream::iter(tasks.into_iter().map(|(order, doc_idx)| {
        // `&Config` is `Copy`; the closure captures the shared reference, not a
        // clone of the struct. The borrow lives as long as the stream is driven.
        let cfg_for_task: &Config = cfg;
        let url = reranked[doc_idx].url.clone();
        let collection = collection.clone();
        let doc_cache = doc_cache.clone();
        async move {
            let points: Result<Arc<Vec<qdrant::QdrantPoint>>> = if cache_enabled {
                let key = DocCacheKey {
                    collection,
                    url: url.clone(),
                    generation,
                };
                let cfg_for_fetch: &Config = cfg_for_task;
                let url_for_fetch = url.clone();
                doc_cache
                    .expect("doc cache must exist when cache is enabled")
                    .get_or_fetch(key, move || async move {
                        qdrant::qdrant_retrieve_by_url(
                            cfg_for_fetch,
                            &url_for_fetch,
                            Some(doc_chunk_limit),
                        )
                        .await
                    })
                    .await
                // Cache returns Arc<Vec<_>> directly — no clone needed.
            } else {
                qdrant::qdrant_retrieve_by_url(cfg_for_task, &url, Some(doc_chunk_limit))
                    .await
                    .map(Arc::new)
            };
            (order, url, points)
        }
    }))
    .buffer_unordered(doc_fetch_concurrency);
    while let Some((order, url, points)) = fetch_stream.next().await {
        match points {
            Ok(points) => fetched_docs.push((order, url, points)),
            Err(err) => {
                log_warn(&format!(
                    "ask: failed to retrieve full document for {url}; continuing with remaining context: {err}"
                ));
                errors.push(FullDocFetchError {
                    url,
                    error: err.to_string(),
                });
            }
        }
    }
    fetched_docs.sort_by_key(|(order, _, _)| *order);
    Ok(FetchedDocsResult {
        docs: fetched_docs,
        errors,
    })
}

fn record_empty_doc_fetch(errors: &mut Vec<FullDocFetchError>, url: String) {
    log_warn(&format!(
        "ask: no points found for full document {url}; continuing with remaining context"
    ));
    errors.push(FullDocFetchError {
        url,
        error: "no points found for full document".to_string(),
    });
}

pub fn ask_doc_cache(cfg: &Config) -> Arc<DocCache> {
    doc_cache_for_config(DocCacheConfig::from_ask_config(cfg))
}

#[cfg(test)]
#[path = "fetchers_tests.rs"]
mod tests;
