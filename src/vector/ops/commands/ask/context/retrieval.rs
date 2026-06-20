use super::super::timing::{AskTiming, AskTimingSlot};
use crate::core::ask_explain::{AskExplainRetrieval, AskExplainScoreKind};
use crate::core::config::Config;
use crate::core::logging::{log_debug, log_info};
use crate::vector::ops::commands::retrieval::{
    CandidateBuildPolicy, CandidateRankingTrace, RetrievedCandidate, authoritative_ratio,
    embed_retrieval_inputs, into_candidates_only, product_authority_ratio, query_allows_low_signal,
    top_domains, vector_mode_metadata,
};
use crate::vector::ops::{qdrant, ranking, tei};
use anyhow::{Result, anyhow};

mod build;
mod dispatch;
mod rerank;
use build::build_ask_candidates;
use dispatch::{DispatchOutcome, run_qdrant_dispatch};
// `RerankParams` is re-exported `pub(super)` so `context_tests.rs` (the `context`
// module's sidecar) can reach it via `super::retrieval::RerankParams`; it is also
// used directly in this module. `apply_mode_aware_rerank` + `is_rrf_mode` are
// only consumed by that sidecar, so their re-exports are gated on `cfg(test)`.
pub(super) use rerank::RerankParams;
#[cfg(test)]
pub(super) use rerank::{apply_mode_aware_rerank, is_rrf_mode};
use rerank::{rerank_with_optional_trace, retrieval_score_kind};

type SearchHitsResult =
    Result<Vec<qdrant::QdrantSearchHit>, Box<dyn std::error::Error + Send + Sync>>;

const ASK_PRODUCT_AUTHORITY_BOOST: f64 = 0.35;

pub(super) struct AskRetrieval {
    pub(super) candidate_count: usize,
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
    pub(super) warnings: Vec<String>,
}

#[tracing::instrument(
    name = "ask.retrieve",
    skip(cfg, query, timing),
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
        mut warnings,
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
    let reranked = into_candidates_only(reranked_candidates);
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

    // Collapse near-duplicate copies (e.g. a docs page mirrored into a GitHub
    // repo) so mirror chunks don't each consume a context slot and crowd out
    // distinct sources. Keeps the canonical representative per cluster.
    let (reranked, dedup_report) =
        super::dedup::dedup_near_duplicates(reranked, &ask_tuning.ask_authoritative_domains);
    if let Some(warning) = dedup_report.warning() {
        log_info(&format!("ask dedup: {warning}"));
        warnings.push(warning);
    }

    log_debug(&format!(
        "ask context_built candidates_retrieved={} candidates_after_score_filter={} candidates_selected={}",
        retrieved_candidates.len(),
        reranked.len(),
        reranked.len().min(ask_tuning.ask_chunk_limit),
    ));
    let top_select_started = std::time::Instant::now();
    let (top_chunk_indices, top_full_doc_indices) = select_retrieval_context_indices(
        cfg,
        ask_tuning.ask_full_docs,
        ask_tuning.ask_chunk_limit,
        &query_forms,
        &reranked,
        query_tokens,
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
        warnings,
    }))
}

fn select_retrieval_context_indices(
    cfg: &Config,
    ask_full_docs: usize,
    ask_chunk_limit: usize,
    query_forms: &super::query_rewrite::AskQueryForms,
    reranked: &[ranking::AskCandidate],
    query_tokens: &[String],
) -> (Vec<usize>, Vec<usize>) {
    let (resolved_full_docs, _) = super::resolve_ask_full_docs_for_model(
        ask_full_docs,
        cfg.ask_full_docs_explicit,
        query_forms.complexity_hint,
        super::high_context_synthesis_model(cfg),
    );
    super::build::select_context_indices(
        reranked,
        query_tokens,
        ask_chunk_limit,
        resolved_full_docs,
        super::build::SelectionPolicy::default(),
    )
}

struct BuiltRetrievalCandidates {
    retrieved_candidates: Vec<RetrievedCandidate>,
    pre_rerank_traces: Vec<CandidateRankingTrace>,
    mode: crate::vector::ops::commands::retrieval::VectorModeMetadata,
    rrf_mode: bool,
    retrieval_score_kind: AskExplainScoreKind,
    warnings: Vec<String>,
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
        mut warnings,
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
        &CandidateBuildPolicy {
            allow_low_signal,
            allow_short_content: false,
        },
        cfg.ask_explain.then_some(retrieval_score_kind),
    );
    warnings.extend(built_candidates.warnings);
    Ok(BuiltRetrievalCandidates {
        retrieved_candidates: built_candidates.retrieved_candidates,
        pre_rerank_traces: built_candidates.pre_rerank_traces,
        mode,
        rrf_mode,
        retrieval_score_kind,
        warnings,
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
    warnings: Vec<String>,
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
        candidate_count: inputs.retrieved_candidates.len(),
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
        warnings: inputs.warnings,
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
