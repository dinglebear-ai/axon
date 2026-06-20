use super::super::super::timing::{AskTiming, AskTimingSlot};
use super::SearchHitsResult;
use crate::core::config::Config;
use crate::core::logging::{log_info, log_warn};
use crate::vector::ops::commands::retrieval::{
    VectorDispatchContext, dispatch_vector_search_with_diagnostics,
};
use crate::vector::ops::qdrant;
use anyhow::{Result, anyhow};

pub(super) struct DispatchOutcome<'a> {
    pub(super) primary_request: qdrant::VectorSearchRequest<'a>,
    pub(super) primary_res: SearchHitsResult,
    pub(super) secondary_res: Option<SearchHitsResult>,
    pub(super) warnings: Vec<String>,
}

/// Dispatch the NL (primary) and optional keyword (secondary) Qdrant searches.
///
/// Tries the batch path (`/points/query/batch`) first when both arms are
/// available; this saves the second TLS+TCP handshake/header round-trip on
/// every ask. On any batch failure, the dispatch falls back to the existing
/// parallel-single (`tokio::join!`) path so retrieval cannot be silently
/// disabled by a transient batch hiccup.
#[allow(clippy::too_many_arguments)]
pub(super) async fn run_qdrant_dispatch<'a>(
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
            .with_filter(qdrant::exclude_local_code_filter())
            .with_candidates_override(Some(hybrid_candidates));
    log_info(&format!(
        "ask qdrant dispatch start mode={} candidate_limit={} hybrid_candidates={} collection={}",
        if use_dual && !ask_vectors.is_empty() {
            "dual"
        } else {
            "single"
        },
        candidate_limit,
        hybrid_candidates,
        cfg.collection,
    ));

    if !use_dual || ask_vectors.is_empty() {
        return run_single_qdrant_dispatch(cfg, primary_request, query, timing).await;
    }

    let vecq_kw = ask_vectors.remove(0);
    let secondary_request =
        qdrant::VectorSearchRequest::from_query(cfg, &vecq_kw, keyword_query, candidate_limit)
            .map_err(|e| anyhow!("build ask keyword vector search request: {e}"))?
            .with_filter(qdrant::exclude_local_code_filter())
            .with_candidates_override(Some(hybrid_candidates));

    run_dual_qdrant_dispatch(
        cfg,
        primary_request,
        secondary_request,
        query,
        keyword_query,
        candidate_limit,
        hybrid_candidates,
        timing,
    )
    .await
}

async fn run_single_qdrant_dispatch<'a>(
    cfg: &'a Config,
    primary_request: qdrant::VectorSearchRequest<'a>,
    query: &'a str,
    timing: &mut AskTiming,
) -> Result<DispatchOutcome<'a>> {
    let (primary_res, primary_ms) = dispatch_ask_arm(cfg, &primary_request, query, "primary").await;
    timing.set(AskTimingSlot::QdrantPrimary, primary_ms);
    log_info(&format!(
        "ask qdrant single dispatch complete primary_ms={primary_ms}"
    ));
    Ok(DispatchOutcome {
        primary_request,
        primary_res,
        secondary_res: None,
        warnings: Vec::new(),
    })
}

#[allow(clippy::too_many_arguments)]
async fn run_dual_qdrant_dispatch<'a>(
    cfg: &'a Config,
    primary_request: qdrant::VectorSearchRequest<'a>,
    secondary_request: qdrant::VectorSearchRequest<'_>,
    query: &'a str,
    keyword_query: &'a str,
    candidate_limit: usize,
    hybrid_candidates: usize,
    timing: &mut AskTiming,
) -> Result<DispatchOutcome<'a>> {
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
    log_info("ask qdrant dual batch dispatch start");
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
            timing.set(
                AskTimingSlot::QdrantPrimary,
                batch_started.elapsed().as_millis(),
            );
            log_info(&format!(
                "ask qdrant dual batch dispatch complete primary_hits={} secondary_hits={} elapsed_ms={}",
                primary.len(),
                secondary.len(),
                batch_started.elapsed().as_millis(),
            ));
            Ok(DispatchOutcome {
                primary_request,
                primary_res: Ok(primary),
                secondary_res: Some(Ok(secondary)),
                warnings: Vec::new(),
            })
        }
        Err(e) => {
            fallback_after_batch_error(
                cfg,
                primary_request,
                secondary_request,
                query,
                keyword_query,
                timing,
                &e.to_string(),
            )
            .await
        }
    }
}

async fn fallback_after_batch_error<'a>(
    cfg: &'a Config,
    primary_request: qdrant::VectorSearchRequest<'a>,
    secondary_request: qdrant::VectorSearchRequest<'_>,
    query: &'a str,
    keyword_query: &'a str,
    timing: &mut AskTiming,
    error: &str,
) -> Result<DispatchOutcome<'a>> {
    log_warn(&format!(
        "ask: qdrant batch dual-search failed, falling back to parallel-single: {error}"
    ));
    let warning = batch_fallback_warning();
    log_info("ask qdrant fallback parallel dispatch start");
    let ((primary_res, primary_ms), (secondary_res, secondary_ms)) = fallback_parallel_dispatch(
        cfg,
        &primary_request,
        query,
        &secondary_request,
        keyword_query,
    )
    .await;
    timing.set(AskTimingSlot::QdrantPrimary, primary_ms);
    timing.set(AskTimingSlot::QdrantSecondary, secondary_ms);
    log_info(&format!(
        "ask qdrant fallback parallel dispatch complete primary_ms={primary_ms} secondary_ms={secondary_ms}"
    ));
    Ok(DispatchOutcome {
        primary_request,
        primary_res,
        secondary_res: Some(secondary_res),
        warnings: vec![warning],
    })
}

fn batch_fallback_warning() -> String {
    "ask: qdrant batch dual-search failed; falling back to parallel-single retrieval".to_string()
}

async fn fallback_parallel_dispatch(
    cfg: &Config,
    primary_request: &qdrant::VectorSearchRequest<'_>,
    query: &str,
    secondary_request: &qdrant::VectorSearchRequest<'_>,
    keyword_query: &str,
) -> (TimedSearchResult, TimedSearchResult) {
    tokio::join!(
        dispatch_ask_arm(cfg, primary_request, query, "primary"),
        dispatch_ask_arm(cfg, secondary_request, keyword_query, "secondary")
    )
}

type TimedSearchResult = (SearchHitsResult, u128);

async fn dispatch_ask_arm(
    cfg: &Config,
    request: &qdrant::VectorSearchRequest<'_>,
    query: &str,
    arm: &'static str,
) -> TimedSearchResult {
    let t = std::time::Instant::now();
    let result = dispatch_vector_search_with_diagnostics(
        cfg,
        request,
        query,
        VectorDispatchContext {
            stage: "ask_vector_search_dispatch",
            command: "ask",
            arm,
            fetch_limit: None,
        },
    )
    .await;
    (result, t.elapsed().as_millis())
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;
