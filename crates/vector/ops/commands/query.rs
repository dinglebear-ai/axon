use crate::crates::core::config::Config;
use crate::crates::vector::ops::source_display::display_source;
use crate::crates::vector::ops::tei::qdrant_store::{VectorMode, get_or_fetch_vector_mode};
use crate::crates::vector::ops::{qdrant, ranking, sparse, tei};
use std::error::Error;

/// Dispatch vector search based on collection mode and hybrid config.
///
/// Named + hybrid enabled + non-empty sparse -> hybrid search (dense + BM42 + RRF)
/// Named + hybrid disabled or empty sparse  -> named dense-only search
/// Unnamed                                   -> legacy `/points/search`
async fn dispatch_search(
    cfg: &Config,
    vector: &[f32],
    query: &str,
    limit: usize,
) -> Result<Vec<qdrant::QdrantSearchHit>, Box<dyn Error>> {
    let mode = get_or_fetch_vector_mode(cfg).await?;
    match mode {
        VectorMode::Named => {
            let sv = sparse::compute_sparse_vector(query);
            if cfg.hybrid_search_enabled && !sv.is_empty() {
                qdrant::qdrant_hybrid_search(cfg, vector, &sv, limit)
                    .await
                    .map_err(|e| -> Box<dyn Error> { e.to_string().into() })
            } else {
                qdrant::qdrant_named_dense_search(cfg, vector, limit)
                    .await
                    .map_err(|e| -> Box<dyn Error> { e.to_string().into() })
            }
        }
        VectorMode::Unnamed => qdrant::qdrant_search(cfg, vector, limit)
            .await
            .map_err(|e| -> Box<dyn Error> { e.to_string().into() }),
    }
}

pub async fn query_results(
    cfg: &Config,
    query: &str,
    limit: usize,
    offset: usize,
) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    let mut query_vectors = tei::tei_embed(cfg, std::slice::from_ref(&query.to_string())).await?;
    if query_vectors.is_empty() {
        return Err("TEI returned no vector for query".into());
    }
    let vector = query_vectors.remove(0);

    let fetch_limit = ((limit + offset).max(1) * 8).max(limit + offset).min(500);
    let hits = dispatch_search(cfg, &vector, query, fetch_limit).await?;
    let query_tokens = ranking::tokenize_query(query);
    let candidates: Vec<ranking::AskCandidate> = hits
        .into_iter()
        .filter_map(|h| {
            let url = qdrant::payload_url_typed(&h.payload).to_string();
            let chunk_text = qdrant::payload_text_typed(&h.payload).to_string();
            if url.is_empty() {
                return None;
            }
            let path = ranking::extract_path_from_url(&url);
            Some(ranking::AskCandidate {
                score: h.score,
                url,
                path: path.clone(),
                chunk_text: chunk_text.clone(),
                url_tokens: ranking::tokenize_path_set(&path),
                chunk_tokens: ranking::tokenize_text_set(&chunk_text),
                rerank_score: h.score,
            })
        })
        .collect();
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let reranked = ranking::rerank_ask_candidates(
        &candidates,
        &query_tokens,
        &cfg.ask_authoritative_domains,
        cfg.ask_authoritative_boost,
    );
    let selected_indices =
        ranking::select_diverse_candidates(&reranked, (limit + offset).max(1), 2);

    Ok(selected_indices
        .into_iter()
        .skip(offset)
        .take(limit)
        .enumerate()
        .map(|(i, hit_idx)| {
            let h = &reranked[hit_idx];
            let url = &h.url;
            let source = display_source(url);
            let preview_idx =
                ranking::select_best_preview_chunk(&reranked, url, &query_tokens, hit_idx);
            let snippet =
                ranking::get_meaningful_snippet(&reranked[preview_idx].chunk_text, &query_tokens);
            serde_json::json!({
                "rank": i + 1,
                "score": h.score,
                "rerank_score": h.rerank_score,
                "url": url,
                "source": source,
                "snippet": snippet,
                "chunk_index": serde_json::Value::Null
            })
        })
        .collect::<Vec<_>>())
}
