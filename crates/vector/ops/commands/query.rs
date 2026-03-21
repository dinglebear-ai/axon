use crate::crates::core::config::Config;
use crate::crates::services::error::ServiceError;
use crate::crates::vector::ops::source_display::display_source;
use crate::crates::vector::ops::{qdrant, ranking, tei};
use std::error::Error;

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
    let hits = qdrant::dispatch_vector_search(cfg, &vector, query, fetch_limit)
        .await
        .map_err(|e| -> Box<dyn Error> {
            if cfg.ask_diagnostics {
                let diagnostics = serde_json::json!({
                    "stage": "query_vector_search_dispatch",
                    "collection": cfg.collection,
                    "qdrant_url": cfg.qdrant_url,
                    "query_len": query.len(),
                    "error": e.to_string(),
                });
                Box::new(ServiceError::with_diagnostics(
                    format!("vector search dispatch: {e}"),
                    diagnostics,
                ))
            } else {
                Box::new(ServiceError::new(format!("vector search dispatch: {e}")))
            }
        })?;
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
            let url_tokens = ranking::tokenize_path_set(&path);
            let chunk_tokens = ranking::tokenize_text_set(&chunk_text);
            Some(ranking::AskCandidate {
                score: h.score,
                url,
                path,
                chunk_text,
                url_tokens,
                chunk_tokens,
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
