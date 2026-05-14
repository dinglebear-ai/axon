use crate::core::config::Config;
use crate::core::logging::log_warn;
use crate::vector::cache::{
    DocCache, DocCacheConfig, DocCacheKey, current_generation, doc_cache_for_config,
};
use crate::vector::ops::qdrant;
use anyhow::Result;
use futures_util::stream::{self, StreamExt};
use std::sync::Arc;

pub async fn fetch_full_docs(
    cfg: &Config,
    reranked: &[crate::vector::ops::ranking::AskCandidate],
    top_full_doc_indices: &[usize],
    context_char_count: usize,
    max_context_chars: usize,
    doc_chunk_limit: usize,
    doc_fetch_concurrency: usize,
) -> Result<Vec<(usize, String, Vec<qdrant::QdrantPoint>)>> {
    let mut fetched_docs = Vec::new();
    if context_char_count >= max_context_chars {
        return Ok(fetched_docs);
    }
    let cfg_arc = Arc::new(cfg.clone());
    // Cache enable gate. The cache itself is process-global; we only consult
    // it when `cfg.ask_cache_enabled`. Snapshot the per-collection generation
    // once for this fetch batch so all concurrent lookups in this stream see
    // a consistent key. (axon_rust-pmc)
    let cache_enabled = cfg.ask_cache_enabled;
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
        let cfg_for_task = Arc::clone(&cfg_arc);
        let url = reranked[doc_idx].url.clone();
        let collection = collection.clone();
        let doc_cache = doc_cache.clone();
        async move {
            let points = if cache_enabled {
                let key = DocCacheKey {
                    collection,
                    url: url.clone(),
                    generation,
                };
                let cfg_for_fetch = Arc::clone(&cfg_for_task);
                let url_for_fetch = url.clone();
                doc_cache
                    .expect("doc cache must exist when cache is enabled")
                    .get_or_fetch(key, move || async move {
                        qdrant::qdrant_retrieve_by_url(
                            &cfg_for_fetch,
                            &url_for_fetch,
                            Some(doc_chunk_limit),
                        )
                        .await
                    })
                    .await
                    .map(|arc| (*arc).clone())
            } else {
                qdrant::qdrant_retrieve_by_url(&cfg_for_task, &url, Some(doc_chunk_limit)).await
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
            }
        }
    }
    fetched_docs.sort_by_key(|(order, _, _)| *order);
    Ok(fetched_docs)
}

pub fn ask_doc_cache(cfg: &Config) -> Arc<DocCache> {
    doc_cache_for_config(DocCacheConfig::from_ask_config(cfg))
}
