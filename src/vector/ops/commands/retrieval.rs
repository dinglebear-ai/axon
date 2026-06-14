use crate::core::ask_explain::{AskExplainFilterDecisionKind, AskExplainScoreKind};
use crate::core::config::Config;
use crate::core::error::ServiceError;
use crate::vector::ops::tei;
use crate::vector::ops::tei::qdrant_store::{VectorMode, get_or_fetch_vector_mode};
use crate::vector::ops::{qdrant, ranking};
use serde_json::{Value, json};
use spider::url::Url;
use std::collections::{HashMap, HashSet};
use std::error::Error;

mod tokens;
mod trace;

pub(crate) use tokens::{candidate_has_topical_overlap, product_authority_boost_for_url};
pub(crate) use trace::{
    CandidateRankingTrace, dropped_candidate_trace, score_and_filter_candidates,
    score_and_filter_candidates_with_trace, score_rrf_candidates_with_trace,
};

#[derive(Clone, Debug)]
pub(crate) struct RetrievedCandidate {
    pub(crate) candidate: ranking::AskCandidate,
    pub(crate) chunk_index: Option<i64>,
    pub(crate) code: CodeSearchMetadata,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct CodeSearchMetadata {
    pub(crate) provider: Option<String>,
    pub(crate) content_kind: Option<String>,
    pub(crate) file_path: Option<String>,
    pub(crate) language: Option<String>,
    pub(crate) file_type: Option<String>,
    pub(crate) is_test: Option<bool>,
    pub(crate) line_start: Option<u32>,
    pub(crate) line_end: Option<u32>,
    pub(crate) chunking_method: Option<String>,
    pub(crate) symbol_name: Option<String>,
    pub(crate) symbol_kind: Option<String>,
    pub(crate) symbol_extraction_status: Option<String>,
}

pub(crate) struct CandidateBuildPolicy {
    pub(crate) allow_low_signal: bool,
}

pub(crate) struct CandidateScorePolicy<'a> {
    pub(crate) authoritative_domains: &'a [String],
    pub(crate) authoritative_boost: f64,
    pub(crate) product_authority_boost: f64,
    pub(crate) apply_code_search_adjustment: bool,
    pub(crate) min_relevance_score: Option<f64>,
    pub(crate) require_topical_overlap: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct VectorDispatchContext {
    pub(crate) stage: &'static str,
    pub(crate) command: &'static str,
    pub(crate) arm: &'static str,
    pub(crate) fetch_limit: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VectorModeMetadata {
    pub(crate) vector_mode: VectorMode,
    pub(crate) sparse_was_empty: bool,
    pub(crate) rrf_mode: bool,
}

pub(crate) struct TypedRetrievalResult {
    pub(crate) retrieved_candidates: Vec<RetrievedCandidate>,
    pub(crate) reranked_candidates: Vec<RetrievedCandidate>,
}

pub(crate) struct CandidateBuildTrace {
    pub(crate) candidates: Vec<RetrievedCandidate>,
    pub(crate) filter_traces: Vec<CandidateRankingTrace>,
}

pub(crate) async fn embed_retrieval_inputs(
    cfg: &Config,
    inputs: &[tei::EmbedInput],
    context: &'static str,
) -> Result<Vec<Vec<f32>>, Box<dyn Error>> {
    tei::tei_embed_typed(cfg, inputs)
        .await
        .map_err(|e| format!("{context}: {e}").into())
}

pub(crate) async fn vector_mode_metadata(
    cfg: &Config,
    request: &qdrant::VectorSearchRequest<'_>,
) -> anyhow::Result<VectorModeMetadata> {
    let vector_mode = get_or_fetch_vector_mode(cfg)
        .await
        .map_err(|e| anyhow::anyhow!("vector mode probe after retrieval dispatch: {e}"))?;
    let sparse_was_empty = request.sparse.as_ref().is_none_or(|sv| sv.is_empty());
    Ok(VectorModeMetadata {
        vector_mode,
        sparse_was_empty,
        rrf_mode: matches!(vector_mode, VectorMode::Named)
            && cfg.hybrid_search_enabled
            && !sparse_was_empty,
    })
}

pub(crate) async fn dispatch_vector_search_with_diagnostics(
    cfg: &Config,
    request: &qdrant::VectorSearchRequest<'_>,
    query: &str,
    context: VectorDispatchContext,
) -> Result<Vec<qdrant::QdrantSearchHit>, Box<dyn Error + Send + Sync>> {
    qdrant::dispatch_vector_search_request(cfg, request)
        .await
        .map_err(|err| {
            Box::new(ServiceError::vector_dispatch_failure(
                context.stage,
                cfg,
                query.len(),
                vector_search_context(request, context),
                err.as_ref(),
            )) as Box<dyn Error + Send + Sync>
        })
}

pub(crate) fn vector_search_context(
    request: &qdrant::VectorSearchRequest<'_>,
    context: VectorDispatchContext,
) -> Value {
    json!({
        "command": context.command,
        "arm": context.arm,
        "request_limit": request.limit,
        "fetch_limit": context.fetch_limit,
        "candidates_override": request.candidates_override,
        "sparse_query_empty": request.sparse.as_ref().is_none_or(|sv| sv.is_empty()),
        "has_filter": request.filter.is_some(),
    })
}

pub(crate) fn build_typed_retrieval_result(
    hits: Vec<qdrant::QdrantSearchHit>,
    query_tokens: &[String],
    build_policy: &CandidateBuildPolicy,
    score_policy: &CandidateScorePolicy<'_>,
) -> TypedRetrievalResult {
    let retrieved_candidates = build_candidates_from_hits(hits, build_policy);
    let reranked_candidates =
        score_and_filter_candidates(&retrieved_candidates, query_tokens, score_policy);
    TypedRetrievalResult {
        retrieved_candidates,
        reranked_candidates,
    }
}

pub(crate) fn build_candidates_from_hits(
    hits: Vec<qdrant::QdrantSearchHit>,
    policy: &CandidateBuildPolicy,
) -> Vec<RetrievedCandidate> {
    build_candidates_from_hits_with_trace(hits, policy, AskExplainScoreKind::Cosine).candidates
}

pub(crate) fn build_candidates_from_hits_with_trace(
    hits: Vec<qdrant::QdrantSearchHit>,
    policy: &CandidateBuildPolicy,
    score_kind: AskExplainScoreKind,
) -> CandidateBuildTrace {
    let mut candidates = Vec::new();
    let mut filter_traces = Vec::new();
    for hit in hits {
        let url = qdrant::payload_url_typed(&hit.payload).to_string();
        let chunk_text = qdrant::payload_text_typed(&hit.payload).to_string();
        if url.is_empty() || chunk_text.len() < 40 {
            continue;
        }
        if !policy.allow_low_signal && ranking::is_low_signal_url(&url) {
            let candidate = retrieved_candidate_from_hit(hit, url, chunk_text);
            filter_traces.push(dropped_candidate_trace(
                candidate,
                score_kind,
                AskExplainFilterDecisionKind::DroppedLowSignal,
                "candidate URL matched low-signal source filtering",
            ));
            continue;
        }
        let candidate = retrieved_candidate_from_hit(hit, url, chunk_text);
        candidates.push(candidate);
    }
    CandidateBuildTrace {
        candidates,
        filter_traces,
    }
}

fn retrieved_candidate_from_hit(
    hit: qdrant::QdrantSearchHit,
    url: String,
    chunk_text: String,
) -> RetrievedCandidate {
    let code = code_metadata_from_payload(&hit.payload);
    let path = ranking::extract_path_from_url(&url);
    let url_tokens = ranking::tokenize_path_set(&path);
    let chunk_tokens = ranking::tokenize_text_set(&chunk_text);
    RetrievedCandidate {
        candidate: ranking::AskCandidate {
            score: hit.score,
            url,
            path,
            chunk_text,
            url_tokens,
            chunk_tokens,
            rerank_score: hit.score,
        },
        chunk_index: hit.payload.chunk_index,
        code,
    }
}

fn code_metadata_from_payload(payload: &qdrant::QdrantPayload) -> CodeSearchMetadata {
    CodeSearchMetadata {
        provider: payload.provider.clone(),
        content_kind: payload.git_content_kind.clone(),
        file_path: payload
            .code_file_path
            .clone()
            .or_else(|| payload.git_file_path.clone()),
        language: payload.code_language.clone(),
        file_type: payload.code_file_type.clone(),
        is_test: payload.code_is_test,
        line_start: payload.code_line_start,
        line_end: payload.code_line_end,
        chunking_method: payload.code_chunking_method.clone(),
        symbol_name: payload.symbol_name.clone(),
        symbol_kind: payload.symbol_kind.clone(),
        symbol_extraction_status: payload.symbol_extraction_status.clone(),
    }
}

/// Merge secondary candidates into primary, deduplicating by (url, chunk prefix).
/// Primary candidates win; secondary only adds chunks not already present.
pub(crate) fn merge_candidates(
    primary: Vec<RetrievedCandidate>,
    secondary: Vec<RetrievedCandidate>,
) -> Vec<RetrievedCandidate> {
    merge_candidates_with_trace(primary, secondary, AskExplainScoreKind::Cosine).candidates
}

pub(crate) fn merge_candidates_with_trace(
    primary: Vec<RetrievedCandidate>,
    secondary: Vec<RetrievedCandidate>,
    score_kind: AskExplainScoreKind,
) -> CandidateBuildTrace {
    fn prefix_key(url: &str, chunk_text: &str) -> String {
        // Truncate at a UTF-8 char boundary so multibyte text cannot panic.
        let mut end = chunk_text.len().min(80);
        while end > 0 && !chunk_text.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}|{}", url, &chunk_text[..end])
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut deduped: Vec<RetrievedCandidate> = Vec::with_capacity(primary.len());
    let mut filter_traces = Vec::new();
    for c in primary {
        let key = prefix_key(&c.candidate.url, &c.candidate.chunk_text);
        if seen.insert(key) {
            deduped.push(c);
        } else {
            filter_traces.push(dropped_candidate_trace(
                c,
                score_kind,
                AskExplainFilterDecisionKind::DroppedDuplicate,
                "candidate duplicated an earlier URL and chunk-text prefix",
            ));
        }
    }
    for c in secondary {
        let key = prefix_key(&c.candidate.url, &c.candidate.chunk_text);
        if seen.insert(key) {
            deduped.push(c);
        } else {
            filter_traces.push(dropped_candidate_trace(
                c,
                score_kind,
                AskExplainFilterDecisionKind::DroppedDuplicate,
                "candidate duplicated an earlier URL and chunk-text prefix",
            ));
        }
    }
    CandidateBuildTrace {
        candidates: deduped,
        filter_traces,
    }
}

pub(crate) fn query_allows_low_signal(query_tokens: &[String], raw_query: &str) -> bool {
    ranking::query_wants_low_signal_sources(query_tokens, raw_query)
}

/// Project the borrowed reranked set into `AskCandidate`s.
///
/// This clones each inner candidate (chunk_text + two token sets). The clone is
/// forced by the borrowed `&[RetrievedCandidate]` signature: `query.rs`
/// re-indexes `reranked_candidates[hit_idx]` for previews after this call, so it
/// cannot relinquish ownership. Capacity is pre-reserved to avoid intermediate
/// reallocations. The ask path uses the move-based [`into_candidates_only`]
/// instead, which avoids the clone entirely (PERF-M1).
pub(crate) fn candidates_only(candidates: &[RetrievedCandidate]) -> Vec<ranking::AskCandidate> {
    let mut out = Vec::with_capacity(candidates.len());
    out.extend(
        candidates
            .iter()
            .map(|candidate| candidate.candidate.clone()),
    );
    out
}

/// Move-based variant of [`candidates_only`]: consumes the owned reranked set and
/// MOVES each inner `AskCandidate` out instead of cloning it. Preserves candidate
/// ordering. Used by the ask path (`ask/context/retrieval.rs`), which owns the
/// `Vec<RetrievedCandidate>` and does not reuse it after the projection — this
/// eliminates the ~1 MB/ask transient clone (PERF-M1). `query.rs` keeps using
/// the borrowing [`candidates_only`] because it re-indexes the set afterward.
pub(crate) fn into_candidates_only(
    candidates: Vec<RetrievedCandidate>,
) -> Vec<ranking::AskCandidate> {
    candidates
        .into_iter()
        .map(|candidate| candidate.candidate)
        .collect()
}

pub(crate) fn url_matches_domain_list(url: &str, domains: &[String]) -> bool {
    if domains.is_empty() {
        return true;
    }
    let host = Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|h| h.to_ascii_lowercase()));
    let Some(host) = host else {
        return false;
    };
    domains.iter().any(|domain| {
        let normalized = domain.trim().to_ascii_lowercase();
        !normalized.is_empty() && (host == normalized || host.ends_with(&format!(".{normalized}")))
    })
}

pub(crate) fn top_domains(candidates: &[ranking::AskCandidate], limit: usize) -> Vec<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for candidate in candidates {
        if let Some(host) = host_from_url(&candidate.url) {
            *counts.entry(host).or_insert(0) += 1;
        }
    }
    let mut entries = counts.into_iter().collect::<Vec<_>>();
    entries.sort_by(|(domain_a, count_a), (domain_b, count_b)| {
        count_b.cmp(count_a).then_with(|| domain_a.cmp(domain_b))
    });
    entries
        .into_iter()
        .take(limit)
        .map(|(domain, count)| format!("{domain}:{count}"))
        .collect()
}

pub(crate) fn authoritative_ratio(candidates: &[ranking::AskCandidate], domains: &[String]) -> f64 {
    if candidates.is_empty() || domains.is_empty() {
        return 0.0;
    }
    let authoritative = candidates
        .iter()
        .filter(|candidate| url_matches_domain_list(&candidate.url, domains))
        .count();
    authoritative as f64 / candidates.len() as f64
}

pub(crate) fn product_authority_ratio(
    candidates: &[ranking::AskCandidate],
    query_tokens: &[String],
    product_authority_boost: f64,
) -> f64 {
    if candidates.is_empty() || query_tokens.is_empty() || product_authority_boost <= 0.0 {
        return 0.0;
    }
    let authoritative = candidates
        .iter()
        .filter(|candidate| {
            product_authority_boost_for_url(&candidate.url, query_tokens, product_authority_boost)
                > 0.0
        })
        .count();
    authoritative as f64 / candidates.len() as f64
}

fn host_from_url(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|h| h.to_ascii_lowercase()))
}

#[cfg(test)]
#[path = "retrieval_tests.rs"]
mod tests;
