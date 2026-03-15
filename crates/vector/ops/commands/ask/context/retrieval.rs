use super::heuristics::{
    authoritative_ratio, candidate_has_topical_overlap, is_low_signal_source_url,
    query_requests_low_signal_sources, top_domains, url_matches_domain_list,
};
use crate::crates::core::config::Config;
use crate::crates::vector::ops::tei::qdrant_store::{VectorMode, get_or_fetch_vector_mode};
use crate::crates::vector::ops::{qdrant, ranking, sparse, tei};
use anyhow::{Result, anyhow};

pub(super) struct AskRetrieval {
    pub(super) candidates: Vec<ranking::AskCandidate>,
    pub(super) reranked: Vec<ranking::AskCandidate>,
    pub(super) top_chunk_indices: Vec<usize>,
    pub(super) top_full_doc_indices: Vec<usize>,
    pub(super) retrieval_elapsed_ms: u128,
    pub(super) top_domains: Vec<String>,
    pub(super) authoritative_ratio: f64,
    pub(super) dropped_by_allowlist: usize,
}

/// Dispatch vector search based on collection mode and hybrid config.
async fn dispatch_ask_search(
    cfg: &Config,
    vector: &[f32],
    query: &str,
    limit: usize,
) -> Result<Vec<qdrant::QdrantSearchHit>> {
    let mode = get_or_fetch_vector_mode(cfg)
        .await
        .map_err(|e| anyhow!(e.to_string()))?;
    match mode {
        VectorMode::Named => {
            let sv = sparse::compute_sparse_vector(query);
            if cfg.hybrid_search_enabled && !sv.is_empty() {
                qdrant::qdrant_hybrid_search(cfg, vector, &sv, limit).await
            } else {
                qdrant::qdrant_named_dense_search(cfg, vector, limit).await
            }
        }
        VectorMode::Unnamed => qdrant::qdrant_search(cfg, vector, limit)
            .await
            .map_err(|e| anyhow!(e.to_string())),
    }
}

pub(super) async fn retrieve_ask_candidates(cfg: &Config, query: &str) -> Result<AskRetrieval> {
    let retrieval_started = std::time::Instant::now();
    let mut ask_vectors = tei::tei_embed(cfg, &[query.to_string()])
        .await
        .map_err(|e| anyhow!(e.to_string()))?;
    if ask_vectors.is_empty() {
        return Err(anyhow!("TEI returned no vector for ask query"));
    }
    let vecq = ask_vectors.remove(0);
    let query_tokens = ranking::tokenize_query(query);
    let allow_low_signal = query_requests_low_signal_sources(&query_tokens, query);
    let hits = dispatch_ask_search(cfg, &vecq, query, cfg.ask_candidate_limit).await?;
    let mut candidates = Vec::new();
    let mut dropped_by_allowlist = 0usize;
    for hit in hits {
        let url = qdrant::payload_url_typed(&hit.payload).to_string();
        let chunk_text = qdrant::payload_text_typed(&hit.payload).to_string();
        if url.is_empty() || chunk_text.len() < 40 {
            continue;
        }
        if !allow_low_signal && is_low_signal_source_url(&url) {
            continue;
        }
        if !cfg.ask_authoritative_allowlist.is_empty()
            && !url_matches_domain_list(&url, &cfg.ask_authoritative_allowlist)
        {
            dropped_by_allowlist += 1;
            continue;
        }
        let path = ranking::extract_path_from_url(&url);
        candidates.push(ranking::AskCandidate {
            score: hit.score,
            url: url.clone(),
            path: path.clone(),
            chunk_text: chunk_text.clone(),
            url_tokens: ranking::tokenize_path_set(&path),
            chunk_tokens: ranking::tokenize_text_set(&chunk_text),
            rerank_score: hit.score,
        });
    }
    if candidates.is_empty() {
        return Err(anyhow!("No relevant documents found for ask query"));
    }

    let reranked = ranking::rerank_ask_candidates(
        &candidates,
        &query_tokens,
        &cfg.ask_authoritative_domains,
        cfg.ask_authoritative_boost,
    )
    .into_iter()
    .filter(|candidate| {
        candidate.rerank_score >= cfg.ask_min_relevance_score
            && candidate_has_topical_overlap(candidate, &query_tokens)
    })
    .collect::<Vec<_>>();
    if reranked.is_empty() {
        return Err(anyhow!(
            "No candidates met relevance threshold {:.3}; lower AXON_ASK_MIN_RELEVANCE_SCORE",
            cfg.ask_min_relevance_score
        ));
    }

    Ok(AskRetrieval {
        top_chunk_indices: ranking::select_diverse_candidates(&reranked, cfg.ask_chunk_limit, 1),
        top_full_doc_indices: ranking::select_diverse_candidates(&reranked, cfg.ask_full_docs, 1),
        top_domains: top_domains(&reranked, 5),
        authoritative_ratio: authoritative_ratio(&reranked, &cfg.ask_authoritative_domains),
        dropped_by_allowlist,
        candidates,
        reranked,
        retrieval_elapsed_ms: retrieval_started.elapsed().as_millis(),
    })
}
