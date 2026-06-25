use crate::ops::commands::retrieval::{
    CandidateBuildPolicy, CandidateScorePolicy, RetrievedCandidate, VectorDispatchContext,
    build_typed_retrieval_result, candidates_only, dispatch_vector_search_with_diagnostics,
    embed_retrieval_inputs, query_allows_low_signal, vector_mode_metadata,
};
use crate::ops::qdrant::exclude_local_code_filter;
use crate::ops::source_display::display_source;
use crate::ops::{qdrant, ranking, tei};
use axon_api::QueryHit;
use axon_core::config::Config;
use std::error::Error;

type QueryError = Box<dyn Error + Send + Sync>;

/// Thin JSON wrapper preserved for compatibility / tests. Serializes the typed
/// `query_hits()` result once; callers in the services layer should prefer
/// `query_hits()` directly to avoid the serialize→deserialize round-trip.
pub async fn query_results(
    cfg: &Config,
    query: &str,
    limit: usize,
    offset: usize,
) -> Result<Vec<serde_json::Value>, QueryError> {
    let hits = query_hits(cfg, query, limit, offset).await?;
    hits.into_iter()
        .map(|hit| serde_json::to_value(hit).map_err(|e| -> QueryError { e.to_string().into() }))
        .collect()
}

/// Run a semantic vector query and return typed `QueryHit`s directly.
pub async fn query_hits(
    cfg: &Config,
    query: &str,
    limit: usize,
    offset: usize,
) -> Result<Vec<QueryHit>, QueryError> {
    query_hits_with_options(
        cfg,
        query,
        limit,
        offset,
        QueryHitOptions {
            command: "query",
            filter: Some(exclude_local_code_filter()),
            allow_short_content: false,
            score_policy: query_score_policy(cfg),
        },
    )
    .await
}

pub(crate) struct QueryHitOptions<'a> {
    pub command: &'static str,
    pub filter: Option<serde_json::Value>,
    pub allow_short_content: bool,
    pub score_policy: CandidateScorePolicy<'a>,
}

pub(crate) async fn query_hits_with_options(
    cfg: &Config,
    query: &str,
    limit: usize,
    offset: usize,
    options: QueryHitOptions<'_>,
) -> Result<Vec<QueryHit>, QueryError> {
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
    let mut request = qdrant::VectorSearchRequest::from_query(cfg, &vector, query, fetch_limit)
        .map_err(|e| -> QueryError { e.to_string().into() })?;
    if let Some(filter) = options.filter {
        request = request.with_filter(filter);
    }
    let hits = dispatch_vector_search_with_diagnostics(
        cfg,
        &request,
        query,
        VectorDispatchContext {
            stage: "query_vector_search_dispatch",
            command: options.command,
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
        allow_short_content: options.allow_short_content,
    };
    let retrieval =
        build_typed_retrieval_result(hits, &query_tokens, &build_policy, &options.score_policy);
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
            QueryHit {
                rank: (offset + i + 1) as u64,
                score: h.score,
                rerank_score: h.rerank_score,
                url: url.clone(),
                source,
                snippet,
                chunk_index: chunk_index_for_candidate(selected),
                file_path: selected.code.file_path.clone(),
                symbol: selected.code.symbol_name.clone(),
                kind: selected.code.symbol_kind.clone(),
                start_line: selected.code.line_start,
                end_line: selected.code.line_end,
                file_type: selected.code.file_type.clone(),
                language: selected.code.language.clone(),
                provider: selected.code.provider.clone(),
                content_kind: selected.code.content_kind.clone(),
                chunking_method: selected.code.chunking_method.clone(),
                symbol_extraction_status: selected.code.symbol_extraction_status.clone(),
            }
        })
        .collect::<Vec<_>>())
}

fn query_score_policy(cfg: &Config) -> CandidateScorePolicy<'_> {
    CandidateScorePolicy {
        authoritative_domains: &cfg.ask_authoritative_domains,
        authoritative_boost: cfg.ask_authoritative_boost,
        product_authority_boost: 0.35,
        apply_code_search_adjustment: true,
        force_code_intent: false,
        min_relevance_score: None,
        require_topical_overlap: true,
    }
}

fn chunk_index_for_candidate(selected: &RetrievedCandidate) -> Option<i64> {
    selected.chunk_index
}

#[cfg(test)]
#[path = "query_tests.rs"]
mod tests;
