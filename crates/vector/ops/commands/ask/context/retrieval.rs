use crate::crates::core::config::Config;
use crate::crates::core::logging::log_debug;
use crate::crates::services::error::ServiceError;
use crate::crates::vector::ops::commands::retrieval::{
    CandidateBuildPolicy, CandidateScorePolicy, authoritative_ratio, build_candidates_from_hits,
    candidates_only, merge_candidates, query_allows_low_signal, score_and_filter_candidates,
    top_domains,
};
use crate::crates::vector::ops::{qdrant, ranking, tei};
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

/// Map a primary `dispatch_vector_search` failure to an `anyhow::Error`,
/// attaching context JSON unconditionally on the error path so operators can
/// see the collection / Qdrant URL / query-length context that produced the
/// failure. The cost is one small JSON object per failure, and every failure
/// already costs at least a Qdrant round-trip — the marginal cost is
/// negligible. The legacy `cfg.ask_diagnostics` flag still gates verbose
/// **success-path** payloads elsewhere (see `ask.rs` and
/// `evaluate/display.rs`). (bd axon_rust-d71.35)
fn dispatch_error(cfg: &Config, query: &str, err: &dyn std::error::Error) -> anyhow::Error {
    let diagnostics = serde_json::json!({
        "stage": "ask_vector_search_dispatch",
        "collection": cfg.collection,
        "qdrant_url": cfg.qdrant_url,
        "query_len": query.len(),
        "error": err.to_string(),
    });
    anyhow::Error::new(ServiceError::with_diagnostics(
        format!("vector search dispatch: {err}"),
        diagnostics,
    ))
}

#[tracing::instrument(
    name = "ask.retrieve",
    skip(cfg, query),
    fields(collection = %cfg.collection, query_len = query.len())
)]
pub(super) async fn retrieve_ask_candidates(cfg: &Config, query: &str) -> Result<AskRetrieval> {
    let retrieval_started = std::time::Instant::now();
    let query_tokens = ranking::tokenize_query(query);
    let allow_low_signal = query_allows_low_signal(&query_tokens, query);

    // Dual-embedding: embed both the NL question and a keyword form in a single TEI
    // batch call. This improves recall for NL queries whose embedding drifts from
    // the document space (e.g. "how do hooks work?" vs "hooks lifecycle events").
    let keyword_query = query_tokens.join(" ");
    let use_dual =
        query_tokens.len() >= 3 && keyword_query.to_lowercase() != query.trim().to_lowercase();

    // Per Qwen3-Embedding asymmetric spec: queries get the instruction prefix, documents
    // do not. The keyword form is essentially document-shaped text (e.g. "PreToolUse hook
    // fields"), so it is embedded WITHOUT the query instruction. Prefixing it would push
    // the keyword vector into query space and defeat the purpose of the dual-embedding
    // pass — see bd axon_rust-d71.5 (H1).
    let mut embed_inputs = vec![tei::prepend_query_instruction(query)];
    if use_dual {
        embed_inputs.push(keyword_query.clone());
    }

    let mut ask_vectors = tei::tei_embed(cfg, &embed_inputs)
        .await
        .map_err(|e| anyhow!("TEI embed for ask query: {e}"))?;
    if ask_vectors.is_empty() {
        return Err(anyhow!("TEI returned no vector for ask query"));
    }
    let vecq = ask_vectors.remove(0);

    // Ask reranks candidates before context selection, so use a wider prefetch window
    // than query (which skips reranking). cfg.ask_hybrid_candidates (default: 150)
    // overrides cfg.hybrid_search_candidates (default: 100) for this path only.
    let ask_cfg_override;
    let search_cfg = if cfg.ask_hybrid_candidates != cfg.hybrid_search_candidates {
        ask_cfg_override = {
            let mut c = cfg.clone();
            c.hybrid_search_candidates = cfg.ask_hybrid_candidates;
            c
        };
        &ask_cfg_override
    } else {
        cfg
    };

    // Run primary (NL) and secondary (keyword) dispatches in parallel when dual-embedding
    // is active. They are independent Qdrant queries; awaiting them sequentially burned
    // ~2-3s per ask (bd axon_rust-d71.3 / C3).
    let primary_fut =
        qdrant::dispatch_vector_search(search_cfg, &vecq, query, cfg.ask_candidate_limit);
    let (primary_res, secondary_res) = if use_dual && !ask_vectors.is_empty() {
        let vecq_kw = ask_vectors.remove(0);
        let secondary_fut = qdrant::dispatch_vector_search(
            search_cfg,
            &vecq_kw,
            &keyword_query,
            cfg.ask_candidate_limit,
        );
        let (p, s) = tokio::join!(primary_fut, secondary_fut);
        (p, Some(s))
    } else {
        (primary_fut.await, None)
    };

    let hits = primary_res.map_err(|e| dispatch_error(cfg, query, e.as_ref()))?;

    let build_policy = CandidateBuildPolicy {
        allow_low_signal,
        authoritative_allowlist: &cfg.ask_authoritative_allowlist,
    };
    let primary = build_candidates_from_hits(hits, &build_policy);
    let mut dropped_by_allowlist = primary.dropped_by_allowlist;
    let mut retrieved_candidates = primary.candidates;

    // Secondary keyword-form search: errors are swallowed since primary already
    // succeeded.
    if let Some(secondary_res) = secondary_res {
        match secondary_res {
            Ok(kw_hits) => {
                let secondary = build_candidates_from_hits(kw_hits, &build_policy);
                dropped_by_allowlist += secondary.dropped_by_allowlist;
                retrieved_candidates = merge_candidates(retrieved_candidates, secondary.candidates);
            }
            Err(e) => log_debug(&format!(
                "ask: keyword search failed (continuing with NL only): {e}"
            )),
        }
    }

    if retrieved_candidates.is_empty() {
        return Err(anyhow!("No relevant documents found for ask query"));
    }

    let score_policy = CandidateScorePolicy {
        authoritative_domains: &cfg.ask_authoritative_domains,
        authoritative_boost: cfg.ask_authoritative_boost,
        min_relevance_score: Some(cfg.ask_min_relevance_score),
        require_topical_overlap: true,
    };
    let reranked_candidates =
        score_and_filter_candidates(&retrieved_candidates, &query_tokens, &score_policy);
    let reranked = candidates_only(&reranked_candidates);
    if reranked.is_empty() {
        return Err(anyhow!(
            "No candidates met relevance threshold {:.3}; lower AXON_ASK_MIN_RELEVANCE_SCORE",
            cfg.ask_min_relevance_score
        ));
    }

    log_debug(&format!(
        "ask context_built candidates_retrieved={} candidates_after_score_filter={} candidates_selected={}",
        retrieved_candidates.len(),
        reranked.len(),
        reranked.len().min(cfg.ask_chunk_limit),
    ));
    Ok(AskRetrieval {
        top_chunk_indices: ranking::select_diverse_candidates(&reranked, cfg.ask_chunk_limit, 1),
        top_full_doc_indices: ranking::select_diverse_candidates(&reranked, cfg.ask_full_docs, 1),
        top_domains: top_domains(&reranked, 5),
        authoritative_ratio: authoritative_ratio(&reranked, &cfg.ask_authoritative_domains),
        dropped_by_allowlist,
        candidates: candidates_only(&retrieved_candidates),
        reranked,
        retrieval_elapsed_ms: retrieval_started.elapsed().as_millis(),
    })
}
