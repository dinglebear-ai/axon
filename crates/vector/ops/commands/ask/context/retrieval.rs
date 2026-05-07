use super::super::timing::{AskTiming, AskTimingSlot};
use crate::crates::core::config::Config;
use crate::crates::core::logging::log_debug;
use crate::crates::services::error::ServiceError;
use crate::crates::vector::ops::commands::retrieval::{
    CandidateBuildPolicy, CandidateScorePolicy, authoritative_ratio, build_candidates_from_hits,
    candidate_has_topical_overlap, candidates_only, merge_candidates, query_allows_low_signal,
    score_and_filter_candidates, top_domains,
};
use crate::crates::vector::ops::tei::qdrant_store::{VectorMode, get_or_fetch_vector_mode};
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
    pub(super) min_supplemental_score: Option<f64>,
}

pub(super) struct RerankParams<'a> {
    pub(super) authoritative_domains: &'a [String],
    pub(super) authoritative_boost: f64,
    pub(super) min_relevance_score: f64,
}

pub(super) fn is_rrf_mode(
    vector_mode: VectorMode,
    hybrid_search_enabled: bool,
    sparse_was_empty: bool,
) -> bool {
    matches!(vector_mode, VectorMode::Named) && hybrid_search_enabled && !sparse_was_empty
}

pub(super) fn apply_mode_aware_rerank(
    is_rrf: bool,
    candidates: &[crate::crates::vector::ops::commands::retrieval::RetrievedCandidate],
    query_tokens: &[String],
    params: &RerankParams<'_>,
) -> Vec<crate::crates::vector::ops::commands::retrieval::RetrievedCandidate> {
    if is_rrf {
        return candidates
            .iter()
            .filter(|candidate| candidate_has_topical_overlap(&candidate.candidate, query_tokens))
            .cloned()
            .map(|mut candidate| {
                candidate.candidate.rerank_score = candidate.candidate.score;
                candidate
            })
            .collect();
    }

    let score_policy = CandidateScorePolicy {
        authoritative_domains: params.authoritative_domains,
        authoritative_boost: params.authoritative_boost,
        min_relevance_score: Some(params.min_relevance_score),
        require_topical_overlap: true,
    };
    score_and_filter_candidates(candidates, query_tokens, &score_policy)
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
pub(super) async fn retrieve_ask_candidates(
    cfg: &Config,
    query: &str,
    timing: &mut AskTiming,
) -> Result<AskRetrieval> {
    let retrieval_started = std::time::Instant::now();
    let ask_tuning = cfg.ask_config();
    let query_forms = super::query_rewrite::build_query_forms(query);
    let query_tokens = query_forms.query_tokens;
    let allow_low_signal = query_allows_low_signal(&query_tokens, query);

    // Per Qwen3-Embedding asymmetric spec: queries get the instruction prefix,
    // documents do not. The typed embed API enforces that distinction at the call site.
    let mut embed_inputs = vec![tei::EmbedInput::query(query)];
    if query_forms.use_dual {
        // The keyword form is essentially document-shaped text (e.g. "PreToolUse
        // hook fields"), so it is embedded WITHOUT the query instruction.
        embed_inputs.push(tei::EmbedInput::document(query_forms.keyword_query.clone()));
    }

    let tei_started = std::time::Instant::now();
    let mut ask_vectors = tei::tei_embed_typed(cfg, &embed_inputs)
        .await
        .map_err(|e| anyhow!("TEI embed for ask query: {e}"))?;
    timing.record(AskTimingSlot::TeiEmbed, tei_started);
    if ask_vectors.is_empty() {
        return Err(anyhow!("TEI returned no vector for ask query"));
    }
    let vecq = ask_vectors.remove(0);

    let DispatchOutcome {
        primary_request,
        primary_res,
        secondary_res,
    } = run_qdrant_dispatch(
        cfg,
        query,
        &vecq,
        &mut ask_vectors,
        &query_forms.keyword_query,
        query_forms.use_dual,
        ask_tuning.ask_candidate_limit,
        ask_tuning.ask_hybrid_candidates,
        timing,
    )
    .await?;

    let hits = primary_res.map_err(|e| dispatch_error(cfg, query, e.as_ref()))?;
    let vector_mode = get_or_fetch_vector_mode(cfg)
        .await
        .map_err(|e| anyhow!("vector mode probe after ask dispatch: {e}"))?;
    let sparse_was_empty = primary_request
        .sparse
        .as_ref()
        .is_none_or(|sv| sv.is_empty());
    let rrf_mode = is_rrf_mode(vector_mode, cfg.hybrid_search_enabled, sparse_was_empty);

    let build_policy = CandidateBuildPolicy { allow_low_signal };
    let primary = build_candidates_from_hits(hits, &build_policy);
    let mut retrieved_candidates = primary.candidates;

    // Secondary keyword-form search: errors are swallowed since primary already
    // succeeded.
    if let Some(secondary_res) = secondary_res {
        match secondary_res {
            Ok(kw_hits) => {
                let secondary = build_candidates_from_hits(kw_hits, &build_policy);
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

    let rerank_params = RerankParams {
        authoritative_domains: &ask_tuning.ask_authoritative_domains,
        authoritative_boost: ask_tuning.ask_authoritative_boost,
        min_relevance_score: ask_tuning.ask_min_relevance_score,
    };
    let rerank_started = std::time::Instant::now();
    let reranked_candidates = apply_mode_aware_rerank(
        rrf_mode,
        &retrieved_candidates,
        &query_tokens,
        &rerank_params,
    );
    let reranked = candidates_only(&reranked_candidates);
    timing.record(AskTimingSlot::Rerank, rerank_started);
    if reranked.is_empty() {
        if rrf_mode {
            return Err(anyhow!("No candidates passed topical overlap"));
        }
        return Err(anyhow!(
            "No candidates met relevance threshold {:.3}; lower AXON_ASK_MIN_RELEVANCE_SCORE",
            ask_tuning.ask_min_relevance_score
        ));
    }

    log_debug(&format!(
        "ask context_built candidates_retrieved={} candidates_after_score_filter={} candidates_selected={}",
        retrieved_candidates.len(),
        reranked.len(),
        reranked.len().min(ask_tuning.ask_chunk_limit),
    ));
    let top_select_started = std::time::Instant::now();
    let (top_chunk_indices, top_full_doc_indices) = super::build::select_context_indices(
        &reranked,
        ask_tuning.ask_chunk_limit,
        ask_tuning.ask_full_docs,
    );
    timing.record(AskTimingSlot::TopSelect, top_select_started);

    Ok(AskRetrieval {
        top_chunk_indices,
        top_full_doc_indices,
        top_domains: top_domains(&reranked, 5),
        authoritative_ratio: authoritative_ratio(&reranked, &ask_tuning.ask_authoritative_domains),
        candidates: candidates_only(&retrieved_candidates),
        reranked,
        retrieval_elapsed_ms: retrieval_started.elapsed().as_millis(),
        min_supplemental_score: if rrf_mode {
            None
        } else {
            Some(
                ask_tuning.ask_min_relevance_score
                    + super::heuristics::SUPPLEMENTAL_RELEVANCE_BONUS,
            )
        },
    })
}

struct DispatchOutcome<'a> {
    primary_request: qdrant::VectorSearchRequest<'a>,
    primary_res: Result<Vec<qdrant::QdrantSearchHit>, Box<dyn std::error::Error + Send + Sync>>,
    secondary_res:
        Option<Result<Vec<qdrant::QdrantSearchHit>, Box<dyn std::error::Error + Send + Sync>>>,
}

/// Build the primary (NL) and optional secondary (keyword) Qdrant search
/// requests, dispatch them in parallel via `tokio::join!` when dual-embedding
/// is active (sequential dispatch burned ~2-3s per ask — bd axon_rust-d71.3 /
/// C3), and record the combined wall-clock into the ask timing accumulator.
#[allow(clippy::too_many_arguments)]
async fn run_qdrant_dispatch<'a>(
    cfg: &'a Config,
    query: &'a str,
    vecq: &'a [f32],
    ask_vectors: &mut Vec<Vec<f32>>,
    keyword_query: &'a str,
    use_dual: bool,
    candidate_limit: usize,
    hybrid_candidates: usize,
    timing: &mut AskTiming,
) -> Result<DispatchOutcome<'a>> {
    let primary_request =
        qdrant::VectorSearchRequest::from_query(cfg, vecq, query, candidate_limit)
            .map_err(|e| anyhow!("build ask vector search request: {e}"))?
            .with_candidates_override(Some(hybrid_candidates));
    let qdrant_started = std::time::Instant::now();
    let primary_fut = qdrant::dispatch_vector_search_request(cfg, &primary_request);
    let (primary_res, secondary_res) = if use_dual && !ask_vectors.is_empty() {
        let vecq_kw = ask_vectors.remove(0);
        let secondary_request =
            qdrant::VectorSearchRequest::from_query(cfg, &vecq_kw, keyword_query, candidate_limit)
                .map_err(|e| anyhow!("build ask keyword vector search request: {e}"))?
                .with_candidates_override(Some(hybrid_candidates));
        let secondary_fut = qdrant::dispatch_vector_search_request(cfg, &secondary_request);
        let (p, s) = tokio::join!(primary_fut, secondary_fut);
        (p, Some(s))
    } else {
        (primary_fut.await, None)
    };
    // tokio::join! only exposes the wall-clock from launching the primary
    // future to having both responses; that combined window is the only
    // meaningful Qdrant-roundtrip metric available here. Record it against
    // `qdrant_primary_ms`; `qdrant_secondary_ms` reflects whether a secondary
    // dispatch ran (0 = no secondary).
    let qdrant_elapsed_ms = qdrant_started.elapsed().as_millis();
    timing.set(AskTimingSlot::QdrantPrimary, qdrant_elapsed_ms);
    timing.set(
        AskTimingSlot::QdrantSecondary,
        if use_dual { qdrant_elapsed_ms } else { 0 },
    );
    Ok(DispatchOutcome {
        primary_request,
        primary_res,
        secondary_res,
    })
}
