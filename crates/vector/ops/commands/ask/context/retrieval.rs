use super::super::timing::{AskTiming, AskTimingSlot};
use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_debug, log_warn};
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
/// attaching collection / Qdrant URL / query-length context unconditionally so
/// operators can diagnose the failure. The legacy `cfg.ask_diagnostics` flag
/// still gates verbose success-path payloads elsewhere.
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
            Err(e) => log_warn(&format!(
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

/// Dispatch the NL (primary) and optional keyword (secondary) Qdrant searches.
///
/// Tries the batch path (`/points/query/batch`) first when both arms are
/// available; this saves the second TLS+TCP handshake/header round-trip on
/// every ask. On any batch failure (transport error, 5xx after retries,
/// VectorMode::Unnamed which the batch helper intentionally rejects) the
/// dispatch falls back to the existing parallel-single (`tokio::join!`) path
/// so retrieval cannot be silently disabled by a transient batch hiccup.
///
/// Timing semantics:
/// - **Batch path**: Qdrant's `/points/query/batch` returns only one
///   aggregate `time` field; per-arm timings are unavailable. We record the
///   batch wall-clock under [`AskTimingSlot::QdrantPrimary`] as the only
///   meaningful signal and leave [`AskTimingSlot::QdrantSecondary`] as None.
///   Operators reading diagnostics should read `qdrant_primary_ms` as the
///   aggregate dispatch ms when `qdrant_secondary_ms` is None.
/// - **Fallback path**: each arm is timed independently as before.
///
/// (bd axon_rust-j2c)
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

    // No secondary arm — single dispatch, classic timing.
    if !use_dual || ask_vectors.is_empty() {
        let t = std::time::Instant::now();
        let primary_res = qdrant::dispatch_vector_search_request(cfg, &primary_request).await;
        timing.set(AskTimingSlot::QdrantPrimary, t.elapsed().as_millis());
        return Ok(DispatchOutcome {
            primary_request,
            primary_res,
            secondary_res: None,
        });
    }

    let vecq_kw = ask_vectors.remove(0);
    let secondary_request =
        qdrant::VectorSearchRequest::from_query(cfg, &vecq_kw, keyword_query, candidate_limit)
            .map_err(|e| anyhow!("build ask keyword vector search request: {e}"))?
            .with_candidates_override(Some(hybrid_candidates));

    // Try batch path first.
    let primary_sparse_default = primary_request.sparse.clone().unwrap_or_default();
    let secondary_sparse_default = secondary_request.sparse.clone().unwrap_or_default();
    let primary_arm = qdrant::DualSearchArm {
        dense: primary_request.dense,
        sparse: &primary_sparse_default,
        filter: primary_request.filter.as_ref(),
    };
    let secondary_arm = qdrant::DualSearchArm {
        dense: secondary_request.dense,
        sparse: &secondary_sparse_default,
        filter: secondary_request.filter.as_ref(),
    };
    let batch_started = std::time::Instant::now();
    match qdrant::qdrant_dual_search(
        cfg,
        primary_arm,
        secondary_arm,
        candidate_limit,
        Some(hybrid_candidates),
    )
    .await
    {
        Ok(qdrant::DualSearchResult { primary, secondary }) => {
            // Per-arm timing is unavailable on the batch path: Qdrant only
            // returns one aggregate `time` field. Record the wall-clock under
            // QdrantPrimary and leave QdrantSecondary unset to signal the
            // batch path to operators reading diagnostics.
            timing.set(
                AskTimingSlot::QdrantPrimary,
                batch_started.elapsed().as_millis(),
            );
            Ok(DispatchOutcome {
                primary_request,
                primary_res: Ok(primary),
                secondary_res: Some(Ok(secondary)),
            })
        }
        Err(e) => {
            log_warn(&format!(
                "ask: qdrant batch dual-search failed, falling back to parallel-single: {e}"
            ));
            // Fallback: parallel-single via tokio::join!. Per-arm timing is
            // captured here so the fallback path retains the existing diag
            // shape (both QdrantPrimary and QdrantSecondary populated).
            let primary_fut = async {
                let t = std::time::Instant::now();
                let r = qdrant::dispatch_vector_search_request(cfg, &primary_request).await;
                (r, t.elapsed().as_millis())
            };
            let secondary_fut = async {
                let t = std::time::Instant::now();
                let r = qdrant::dispatch_vector_search_request(cfg, &secondary_request).await;
                (r, t.elapsed().as_millis())
            };
            let ((primary_res, primary_ms), (secondary_res, secondary_ms)) =
                tokio::join!(primary_fut, secondary_fut);
            timing.set(AskTimingSlot::QdrantPrimary, primary_ms);
            timing.set(AskTimingSlot::QdrantSecondary, secondary_ms);
            Ok(DispatchOutcome {
                primary_request,
                primary_res,
                secondary_res: Some(secondary_res),
            })
        }
    }
}
