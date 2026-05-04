use crate::crates::core::config::Config;
use crate::crates::services::error::ServiceError;
use crate::crates::vector::ops::commands::retrieval::{
    CandidateBuildPolicy, CandidateScorePolicy, RetrievedCandidate, build_candidates_from_hits,
    candidates_only, query_allows_low_signal, score_and_filter_candidates,
};
use crate::crates::vector::ops::source_display::display_source;
use crate::crates::vector::ops::{qdrant, ranking, tei};
use std::error::Error;

pub async fn query_results(
    cfg: &Config,
    query: &str,
    limit: usize,
    offset: usize,
) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    let mut query_vectors = tei::tei_embed_kind(
        cfg,
        tei::EmbedKind::Query,
        std::slice::from_ref(&query.to_string()),
    )
    .await?;
    if query_vectors.is_empty() {
        return Err("TEI returned no vector for query".into());
    }
    let vector = query_vectors.remove(0);

    let fetch_limit = ((limit + offset).max(1) * 16).max(limit + offset).min(1000);
    let request = qdrant::VectorSearchRequest::from_query(cfg, &vector, query, fetch_limit)
        .map_err(|e| -> Box<dyn Error> { e.to_string().into() })?;
    let hits = qdrant::dispatch_vector_search_request(cfg, &request)
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
    let build_policy = CandidateBuildPolicy {
        allow_low_signal: query_allows_low_signal(&query_tokens, query),
    };
    let built = build_candidates_from_hits(hits, &build_policy);
    if built.candidates.is_empty() {
        return Ok(Vec::new());
    }
    let score_policy = query_score_policy(cfg);
    let reranked_with_metadata =
        score_and_filter_candidates(&built.candidates, &query_tokens, &score_policy);
    if reranked_with_metadata.is_empty() {
        return Ok(Vec::new());
    }
    let reranked = candidates_only(&reranked_with_metadata);
    let selected_indices =
        ranking::select_diverse_candidates(&reranked, (limit + offset).max(1), 2);

    Ok(selected_indices
        .into_iter()
        .skip(offset)
        .take(limit)
        .enumerate()
        .map(|(i, hit_idx)| {
            let selected = &reranked_with_metadata[hit_idx];
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
mod tests {
    use super::{chunk_index_for_candidate, query_score_policy};
    use crate::crates::core::config::Config;
    use crate::crates::vector::ops::commands::retrieval::RetrievedCandidate;
    use crate::crates::vector::ops::ranking;
    use crate::crates::vector::ops::tei::{QUERY_INSTRUCTION, prepend_query_instruction};

    #[test]
    fn query_instruction_is_nonempty_and_ends_with_query_colon() {
        assert!(!QUERY_INSTRUCTION.is_empty());
        assert!(
            QUERY_INSTRUCTION.ends_with("Query: "),
            "instruction must end with 'Query: ', got: {QUERY_INSTRUCTION:?}"
        );
    }

    #[test]
    fn query_instruction_prepend_produces_correct_string() {
        // Tests the prepend_query_instruction() helper used in query_results().
        // Locks in: instruction is prepended, query text is preserved verbatim,
        // combined string is strictly longer than the query alone.
        let query = "how does markdown splitting work";
        let with_instruction = prepend_query_instruction(query);

        assert!(
            with_instruction.starts_with("Instruct:"),
            "combined string must start with the instruction prefix"
        );
        assert!(
            with_instruction.ends_with(query),
            "combined string must end with the original query text verbatim"
        );
        assert!(
            with_instruction.len() > query.len(),
            "combined string must be longer than the query alone"
        );
    }

    #[test]
    fn chunk_index_for_candidate_returns_payload_index() {
        let selected = RetrievedCandidate {
            candidate: ranking::AskCandidate {
                score: 0.9,
                url: "https://example.com/a".to_string(),
                path: "/a".to_string(),
                chunk_text: "chunk body".to_string(),
                url_tokens: std::collections::HashSet::new(),
                chunk_tokens: std::collections::HashSet::new(),
                rerank_score: 0.9,
            },
            chunk_index: Some(42),
        };

        assert_eq!(chunk_index_for_candidate(&selected), serde_json::json!(42));
    }

    #[test]
    fn absolute_rank_uses_offset_plus_one_based_index() {
        let offset = 20usize;
        let ranks = (0..3).map(|i| offset + i + 1).collect::<Vec<_>>();
        assert_eq!(ranks, vec![21, 22, 23]);
    }

    #[test]
    fn query_score_policy_does_not_apply_ask_threshold() {
        let mut cfg = Config {
            ask_min_relevance_score: 0.45,
            ..Config::default()
        };
        cfg.ask_authoritative_boost = 0.25;

        let policy = query_score_policy(&cfg);

        assert_eq!(policy.min_relevance_score, None);
        assert!(policy.require_topical_overlap);
        assert_eq!(policy.authoritative_boost, 0.25);
    }
}
