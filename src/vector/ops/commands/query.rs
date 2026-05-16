use crate::core::config::Config;
use crate::vector::ops::commands::retrieval::{
    CandidateBuildPolicy, CandidateScorePolicy, RetrievedCandidate, VectorDispatchContext,
    build_typed_retrieval_result, candidates_only, dispatch_vector_search_with_diagnostics,
    embed_retrieval_inputs, query_allows_low_signal, vector_mode_metadata,
};
use crate::vector::ops::source_display::display_source;
use crate::vector::ops::{qdrant, ranking, tei};
use std::error::Error;

type QueryError = Box<dyn Error + Send + Sync>;

pub async fn query_results(
    cfg: &Config,
    query: &str,
    limit: usize,
    offset: usize,
) -> Result<Vec<serde_json::Value>, QueryError> {
    let mut query_vectors =
        embed_retrieval_inputs(cfg, &[tei::EmbedInput::query(query)], "TEI embed for query")
            .await
            .map_err(|e| -> QueryError { e.to_string().into() })?;
    if query_vectors.is_empty() {
        return Err("TEI returned no vector for query".into());
    }
    let vector = query_vectors.remove(0);

    let total = limit + offset;
    let fetch_limit = (total.max(1) * 16).min(1000);
    let request = qdrant::VectorSearchRequest::from_query(cfg, &vector, query, fetch_limit)
        .map_err(|e| -> QueryError { e.to_string().into() })?;
    let hits = dispatch_vector_search_with_diagnostics(
        cfg,
        &request,
        query,
        VectorDispatchContext {
            stage: "query_vector_search_dispatch",
            command: "query",
            arm: "primary",
            fetch_limit: Some(fetch_limit),
        },
    )
    .await?;
    let _mode = vector_mode_metadata(cfg, &request)
        .await
        .map_err(|e| -> QueryError { e.to_string().into() })?;
    let query_tokens = ranking::tokenize_query(query);
    let build_policy = CandidateBuildPolicy {
        allow_low_signal: query_allows_low_signal(&query_tokens, query),
    };
    let score_policy = query_score_policy(cfg);
    let retrieval = build_typed_retrieval_result(hits, &query_tokens, &build_policy, &score_policy);
    if retrieval.retrieved_candidates.is_empty() || retrieval.reranked_candidates.is_empty() {
        return Ok(Vec::new());
    }
    let reranked = candidates_only(&retrieval.reranked_candidates);
    let selected_indices = ranking::select_diverse_candidates(&reranked, total.max(1), 2);

    Ok(selected_indices
        .into_iter()
        .skip(offset)
        .take(limit)
        .enumerate()
        .map(|(i, hit_idx)| {
            let selected = &retrieval.reranked_candidates[hit_idx];
            let h = &selected.candidate;
            let url = &h.url;
            let source = display_source(url);
            let preview_idx =
                ranking::select_best_preview_chunk(&reranked, url, &query_tokens, hit_idx);
            let snippet =
                ranking::get_meaningful_snippet(&reranked[preview_idx].chunk_text, &query_tokens);
            serde_json::json!({
                "rank": offset + i + 1,
                "score": h.score,
                "rerank_score": h.rerank_score,
                "url": url,
                "source": source,
                "snippet": snippet,
                "chunk_index": chunk_index_for_candidate(selected)
            })
        })
        .collect::<Vec<_>>())
}

fn query_score_policy(cfg: &Config) -> CandidateScorePolicy<'_> {
    CandidateScorePolicy {
        authoritative_domains: &cfg.ask_authoritative_domains,
        authoritative_boost: cfg.ask_authoritative_boost,
        product_authority_boost: 0.35,
        min_relevance_score: None,
        require_topical_overlap: true,
    }
}

fn chunk_index_for_candidate(selected: &RetrievedCandidate) -> serde_json::Value {
    selected
        .chunk_index
        .map_or(serde_json::Value::Null, serde_json::Value::from)
}

#[cfg(test)]
#[path = "query_tests.rs"]
mod tests;
