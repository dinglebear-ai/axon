use super::super::timing::{AskTiming, AskTimingSlot};
use crate::core::config::Config;
use crate::core::logging::{log_debug, log_info};
use crate::services::types::{AskExplainRetrieval, AskExplainScoreKind};
use crate::vector::ops::commands::retrieval::{
    CandidateBuildPolicy, CandidateRankingTrace, CandidateScorePolicy, RetrievedCandidate,
    authoritative_ratio, candidates_only, embed_retrieval_inputs, product_authority_ratio,
    query_allows_low_signal, score_and_filter_candidates, score_and_filter_candidates_with_trace,
    score_rrf_candidates_with_trace, top_domains, vector_mode_metadata,
};
use crate::vector::ops::tei::qdrant_store::VectorMode;
use crate::vector::ops::{qdrant, ranking, tei};
use anyhow::{Result, anyhow};

mod build;
mod dispatch;
use build::build_ask_candidates;
use dispatch::{DispatchOutcome, run_qdrant_dispatch};

type SearchHitsResult =
    Result<Vec<qdrant::QdrantSearchHit>, Box<dyn std::error::Error + Send + Sync>>;

const ASK_PRODUCT_AUTHORITY_BOOST: f64 = 0.35;

pub(super) struct AskRetrieval {
    pub(super) candidates: Vec<ranking::AskCandidate>,
    pub(super) reranked: Vec<ranking::AskCandidate>,
    pub(super) top_chunk_indices: Vec<usize>,
    pub(super) top_full_doc_indices: Vec<usize>,
    pub(super) retrieval_elapsed_ms: u128,
    pub(super) top_domains: Vec<String>,
    pub(super) authoritative_ratio: f64,
    pub(super) configured_authority_ratio: f64,
    pub(super) product_authority_ratio: f64,
    pub(super) min_supplemental_score: Option<f64>,
    pub(super) explain_retrieval: Option<AskExplainRetrieval>,
    pub(super) candidate_traces: Vec<CandidateRankingTrace>,
}

pub(super) struct RerankParams<'a> {
    pub(super) authoritative_domains: &'a [String],
    pub(super) authoritative_boost: f64,
    pub(super) product_authority_boost: f64,
    pub(super) min_relevance_score: f64,
}

#[cfg(test)]
pub(super) fn is_rrf_mode(
    vector_mode: VectorMode,
    hybrid_search_enabled: bool,
    sparse_was_empty: bool,
) -> bool {
    matches!(vector_mode, VectorMode::Named) && hybrid_search_enabled && !sparse_was_empty
}

pub(super) fn apply_mode_aware_rerank(
    is_rrf: bool,
    candidates: &[RetrievedCandidate],
    query_tokens: &[String],
    params: &RerankParams<'_>,
) -> Vec<RetrievedCandidate> {
    let score_policy = CandidateScorePolicy {
        authoritative_domains: params.authoritative_domains,
        authoritative_boost: params.authoritative_boost,
        product_authority_boost: params.product_authority_boost,
        min_relevance_score: if is_rrf {
            None
        } else {
            Some(params.min_relevance_score)
        },
        require_topical_overlap: true,
    };
    score_and_filter_candidates(candidates, query_tokens, &score_policy)
}

pub(super) fn apply_mode_aware_rerank_with_trace(
    is_rrf: bool,
    dense_score_kind: AskExplainScoreKind,
    candidates: &[RetrievedCandidate],
    query_tokens: &[String],
    params: &RerankParams<'_>,
) -> (Vec<RetrievedCandidate>, Vec<CandidateRankingTrace>) {
    if is_rrf {
        let score_policy = CandidateScorePolicy {
            authoritative_domains: params.authoritative_domains,
            authoritative_boost: params.authoritative_boost,
            product_authority_boost: params.product_authority_boost,
            min_relevance_score: None,
            require_topical_overlap: true,
        };
        return score_rrf_candidates_with_trace(candidates, query_tokens, &score_policy);
    }

    let score_policy = CandidateScorePolicy {
        authoritative_domains: params.authoritative_domains,
        authoritative_boost: params.authoritative_boost,
        product_authority_boost: params.product_authority_boost,
        min_relevance_score: Some(params.min_relevance_score),
        require_topical_overlap: true,
    };
    score_and_filter_candidates_with_trace(
        candidates,
        query_tokens,
        &score_policy,
        dense_score_kind,
    )
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
    let query_tokens = &query_forms.query_tokens;
    let allow_low_signal = query_allows_low_signal(query_tokens, query);
    log_info(&format!(
        "ask retrieval start query_len={} use_dual={} complexity={:?} tokens={} candidate_limit={} hybrid_candidates={}",
        query.len(),
        query_forms.use_dual,
        query_forms.complexity_hint,
        query_tokens.len(),
        ask_tuning.ask_candidate_limit,
        ask_tuning.ask_hybrid_candidates,
    ));
    let mut ask_vectors = embed_ask_query_forms(cfg, query, &query_forms, timing).await?;
    let vecq = ask_vectors.remove(0);

    let BuiltRetrievalCandidates {
        retrieved_candidates,
        pre_rerank_traces,
        mode,
        rrf_mode,
        retrieval_score_kind,
    } = retrieve_and_build_candidates(
        cfg,
        query,
        &vecq,
        &mut ask_vectors,
        &query_forms,
        allow_low_signal,
        timing,
    )
    .await?;

    if retrieved_candidates.is_empty() {
        return Err(anyhow!("No relevant documents found for ask query"));
    }

    let rerank_params = RerankParams {
        authoritative_domains: &ask_tuning.ask_authoritative_domains,
        authoritative_boost: ask_tuning.ask_authoritative_boost,
        product_authority_boost: ASK_PRODUCT_AUTHORITY_BOOST,
        min_relevance_score: ask_tuning.ask_min_relevance_score,
    };
    let rerank_started = std::time::Instant::now();
    log_info(&format!(
        "ask rerank start candidates={} rrf_mode={} score_kind={:?}",
        retrieved_candidates.len(),
        rrf_mode,
        retrieval_score_kind,
    ));
    let (reranked_candidates, candidate_traces) = rerank_with_optional_trace(
        cfg.ask_explain,
        rrf_mode,
        retrieval_score_kind,
        &retrieved_candidates,
        query_tokens,
        &rerank_params,
    );
    let mut candidate_traces = candidate_traces;
    candidate_traces.extend(pre_rerank_traces);
    let reranked = candidates_only(&reranked_candidates);
    timing.record(AskTimingSlot::Rerank, rerank_started);
    log_info(&format!(
        "ask rerank complete candidates={} selected_after_filter={} elapsed_ms={}",
        retrieved_candidates.len(),
        reranked.len(),
        rerank_started.elapsed().as_millis(),
    ));
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
        query_tokens,
        ask_tuning.ask_chunk_limit,
        ask_tuning.ask_full_docs,
        super::build::SelectionPolicy::default(),
    );
    timing.record(AskTimingSlot::TopSelect, top_select_started);

    Ok(finalize_retrieval(FinalizeRetrievalInputs {
        cfg,
        query,
        query_forms: &query_forms,
        mode: &mode,
        retrieval_score_kind,
        retrieved_candidates: &retrieved_candidates,
        reranked,
        top_chunk_indices,
        top_full_doc_indices,
        query_tokens,
        retrieval_started,
        candidate_traces,
    }))
}

struct BuiltRetrievalCandidates {
    retrieved_candidates: Vec<RetrievedCandidate>,
    pre_rerank_traces: Vec<CandidateRankingTrace>,
    mode: crate::vector::ops::commands::retrieval::VectorModeMetadata,
    rrf_mode: bool,
    retrieval_score_kind: AskExplainScoreKind,
}

async fn retrieve_and_build_candidates<'a>(
    cfg: &'a Config,
    query: &'a str,
    vecq: &'a [f32],
    ask_vectors: &mut Vec<Vec<f32>>,
    query_forms: &'a super::query_rewrite::AskQueryForms,
    allow_low_signal: bool,
    timing: &mut AskTiming,
) -> Result<BuiltRetrievalCandidates> {
    let ask_tuning = cfg.ask_config();
    let DispatchOutcome {
        primary_request,
        primary_res,
        secondary_res,
    } = run_qdrant_dispatch(
        cfg,
        query,
        vecq,
        ask_vectors,
        &query_forms.keyword_query,
        query_forms.use_dual,
        ask_tuning.ask_candidate_limit,
        ask_tuning.ask_hybrid_candidates,
        timing,
    )
    .await?;
    let hits = primary_res.map_err(|e| anyhow!("{e}"))?;
    log_qdrant_hits(&hits, &secondary_res);
    let mode = vector_mode_metadata(cfg, &primary_request).await?;
    let rrf_mode = mode.rrf_mode;
    let retrieval_score_kind = retrieval_score_kind(mode.vector_mode, rrf_mode);
    let built_candidates = build_ask_candidates(
        hits,
        secondary_res,
        &CandidateBuildPolicy { allow_low_signal },
        cfg.ask_explain.then_some(retrieval_score_kind),
    );
    Ok(BuiltRetrievalCandidates {
        retrieved_candidates: built_candidates.retrieved_candidates,
        pre_rerank_traces: built_candidates.pre_rerank_traces,
        mode,
        rrf_mode,
        retrieval_score_kind,
    })
}

fn log_qdrant_hits(
    hits: &[qdrant::QdrantSearchHit],
    secondary_res: &Option<
        Result<Vec<qdrant::QdrantSearchHit>, Box<dyn std::error::Error + Send + Sync>>,
    >,
) {
    log_info(&format!(
        "ask retrieval qdrant returned primary_hits={} secondary_hits={}",
        hits.len(),
        secondary_res
            .as_ref()
            .and_then(|res| res.as_ref().ok())
            .map_or(0, Vec::len),
    ));
}

struct FinalizeRetrievalInputs<'a> {
    cfg: &'a Config,
    query: &'a str,
    query_forms: &'a super::query_rewrite::AskQueryForms,
    mode: &'a crate::vector::ops::commands::retrieval::VectorModeMetadata,
    retrieval_score_kind: AskExplainScoreKind,
    retrieved_candidates: &'a [RetrievedCandidate],
    reranked: Vec<ranking::AskCandidate>,
    top_chunk_indices: Vec<usize>,
    top_full_doc_indices: Vec<usize>,
    query_tokens: &'a [String],
    retrieval_started: std::time::Instant,
    candidate_traces: Vec<CandidateRankingTrace>,
}

fn finalize_retrieval(inputs: FinalizeRetrievalInputs<'_>) -> AskRetrieval {
    let ask_tuning = inputs.cfg.ask_config();
    let configured_authority_ratio =
        authoritative_ratio(&inputs.reranked, &ask_tuning.ask_authoritative_domains);
    let product_authority_ratio = product_authority_ratio(
        &inputs.reranked,
        inputs.query_tokens,
        ASK_PRODUCT_AUTHORITY_BOOST,
    );

    AskRetrieval {
        top_chunk_indices: inputs.top_chunk_indices,
        top_full_doc_indices: inputs.top_full_doc_indices,
        top_domains: top_domains(&inputs.reranked, 5),
        authoritative_ratio: configured_authority_ratio.max(product_authority_ratio),
        configured_authority_ratio,
        product_authority_ratio,
        candidates: candidates_only(inputs.retrieved_candidates),
        reranked: inputs.reranked,
        retrieval_elapsed_ms: inputs.retrieval_started.elapsed().as_millis(),
        min_supplemental_score: min_supplemental_score(
            inputs.mode.rrf_mode,
            ask_tuning.ask_min_relevance_score,
        ),
        explain_retrieval: explain_retrieval(
            inputs.cfg,
            inputs.query,
            inputs.query_forms,
            inputs.mode,
            inputs.retrieval_score_kind,
            ask_tuning.ask_candidate_limit,
            ask_tuning.ask_hybrid_candidates,
        ),
        candidate_traces: inputs.candidate_traces,
    }
}

fn min_supplemental_score(rrf_mode: bool, min_relevance_score: f64) -> Option<f64> {
    (!rrf_mode).then_some(min_relevance_score + super::heuristics::SUPPLEMENTAL_RELEVANCE_BONUS)
}

fn explain_retrieval(
    cfg: &Config,
    query: &str,
    query_forms: &super::query_rewrite::AskQueryForms,
    mode: &crate::vector::ops::commands::retrieval::VectorModeMetadata,
    score_kind: AskExplainScoreKind,
    candidate_limit: usize,
    hybrid_candidate_limit: usize,
) -> Option<AskExplainRetrieval> {
    cfg.ask_explain.then(|| AskExplainRetrieval {
        query: query.to_string(),
        keyword_query: query_forms.keyword_query.clone(),
        dual_search: query_forms.use_dual,
        collection: cfg.collection.clone(),
        candidate_limit,
        hybrid_search_enabled: cfg.hybrid_search_enabled,
        hybrid_candidate_limit,
        score_kind,
        vector_mode: format!("{:?}", mode.vector_mode).to_ascii_lowercase(),
        sparse_query_status: mode
            .sparse_was_empty
            .then(|| "empty_sparse_fallback".to_string()),
    })
}

fn rerank_with_optional_trace(
    ask_explain: bool,
    rrf_mode: bool,
    dense_score_kind: AskExplainScoreKind,
    retrieved_candidates: &[RetrievedCandidate],
    query_tokens: &[String],
    rerank_params: &RerankParams<'_>,
) -> (Vec<RetrievedCandidate>, Vec<CandidateRankingTrace>) {
    if ask_explain {
        apply_mode_aware_rerank_with_trace(
            rrf_mode,
            dense_score_kind,
            retrieved_candidates,
            query_tokens,
            rerank_params,
        )
    } else {
        (
            apply_mode_aware_rerank(rrf_mode, retrieved_candidates, query_tokens, rerank_params),
            Vec::new(),
        )
    }
}

fn retrieval_score_kind(vector_mode: VectorMode, rrf_mode: bool) -> AskExplainScoreKind {
    if rrf_mode {
        AskExplainScoreKind::Rrf
    } else if matches!(vector_mode, VectorMode::Named) {
        AskExplainScoreKind::NamedDense
    } else {
        AskExplainScoreKind::Cosine
    }
}

async fn embed_ask_query_forms(
    cfg: &Config,
    query: &str,
    query_forms: &super::query_rewrite::AskQueryForms,
    timing: &mut AskTiming,
) -> Result<Vec<Vec<f32>>> {
    // Per Qwen3-Embedding asymmetric spec: queries get the instruction prefix,
    // documents do not. The typed embed API enforces that distinction at the call site.
    let mut embed_inputs = vec![tei::EmbedInput::query(query)];
    if query_forms.use_dual {
        // The keyword form is essentially document-shaped text (e.g. "PreToolUse
        // hook fields"), so it is embedded WITHOUT the query instruction.
        embed_inputs.push(tei::EmbedInput::document(query_forms.keyword_query.clone()));
    }

    let tei_started = std::time::Instant::now();
    let ask_vectors = embed_retrieval_inputs(cfg, &embed_inputs, "TEI embed for ask query")
        .await
        .map_err(|e| anyhow!("{e}"))?;
    timing.record(AskTimingSlot::TeiEmbed, tei_started);
    log_info(&format!(
        "ask embed complete forms={} vectors={} elapsed_ms={}",
        embed_inputs.len(),
        ask_vectors.len(),
        tei_started.elapsed().as_millis(),
    ));
    if ask_vectors.is_empty() {
        return Err(anyhow!("TEI returned no vector for ask query"));
    }
    Ok(ask_vectors)
}
