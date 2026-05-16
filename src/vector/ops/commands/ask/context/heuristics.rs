use crate::core::config::Config;
#[cfg(test)]
use crate::vector::ops::ranking;
use crate::vector::ops::ranking::AskCandidate;
#[cfg(test)]
use spider::url::Url;
#[cfg(test)]
use std::collections::HashMap;
use std::collections::HashSet;

pub(super) const SUPPLEMENTAL_CONTEXT_BUDGET_PCT: usize = 85;
pub(super) const SUPPLEMENTAL_MIN_TOP_CHUNKS_FOR_COVERAGE: usize = 6;
pub(super) const SUPPLEMENTAL_RELEVANCE_BONUS: f64 = 0.0;

/// Outcome of the adaptive full-doc fetch skip gate.
///
/// `reason` is a static string suitable for diagnostics emission. Possible
/// values:
/// - `"disabled"`           — gate disabled by config (`fulldoc-skip-enabled = false`).
/// - `"empty_top_k"`        — reranked top-K was empty.
/// - `"insufficient_urls"`  — fewer unique URLs than `fulldoc_skip_min_urls`.
/// - `"insufficient_chars"` — chunk byte sum below `fulldoc_skip_min_chars`.
/// - `"low_top_scores"`     — at least one top-K score under the mode-aware floor.
/// - `"ok_skip"`            — all conditions satisfied; full-doc fetch is elided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SkipDecision {
    pub(super) skip: bool,
    pub(super) reason: &'static str,
}

impl SkipDecision {
    const fn skip() -> Self {
        Self {
            skip: true,
            reason: "ok_skip",
        }
    }
    const fn keep(reason: &'static str) -> Self {
        Self {
            skip: false,
            reason,
        }
    }
}

/// Returns true when the reranked top-K already provides sufficient coverage
/// and the `fetch_full_docs(...)` stage can be safely elided.
///
/// Mode-aware threshold (per `crates/vector/CLAUDE.md` Ranking Pipeline contract):
/// - **Cosine path** (Unnamed legacy / dense-only Named): every score in top-K
///   must be `>= cfg.ask_min_relevance_score + cfg.ask_fulldoc_skip_score_delta`.
/// - **RRF path** (Named with `dense + bm42`, hybrid enabled, non-empty sparse):
///   rerank scores are rank-fusion output (unitless). Use a rank-based gate —
///   every score in top-K must be `>= P75` of all reranked candidate scores.
///
/// Coverage check (both modes):
/// - Unique URLs in top-K `>= cfg.ask_fulldoc_skip_min_urls`.
/// - Total `chunk_text` bytes summed across top-K `>= cfg.ask_fulldoc_skip_min_chars`.
///
/// Default-disabled: when `cfg.ask_fulldoc_skip_enabled == false`, returns
/// `SkipDecision { skip: false, reason: "disabled" }` immediately so the
/// classic full-doc fetch path runs unmodified. (bd axon_rust-30y)
pub(super) fn should_skip_full_doc_fetch(
    cfg: &Config,
    reranked: &[AskCandidate],
    is_rrf_mode: bool,
) -> SkipDecision {
    if !cfg.ask_fulldoc_skip_enabled {
        return SkipDecision::keep("disabled");
    }

    // Determine the top-K window. We mirror `ask_chunk_limit` here so the gate
    // reasons over the same slice that `select_diverse_candidates` will pick
    // for top chunks. Falls back to the full reranked list when the limit is
    // larger than the candidate count.
    let top_k = cfg.ask_chunk_limit.min(reranked.len());
    if top_k == 0 {
        return SkipDecision::keep("empty_top_k");
    }
    let top = &reranked[..top_k];

    // Coverage check: unique URLs.
    let unique_urls: HashSet<&str> = top.iter().map(|c| c.url.as_str()).collect();
    if unique_urls.len() < cfg.ask_fulldoc_skip_min_urls {
        return SkipDecision::keep("insufficient_urls");
    }

    // Coverage check: total chunk_text bytes.
    let total_chars: usize = top.iter().map(|c| c.chunk_text.len()).sum();
    if total_chars < cfg.ask_fulldoc_skip_min_chars {
        return SkipDecision::keep("insufficient_chars");
    }

    // Mode-aware quality floor.
    let floor = if is_rrf_mode {
        // Rank-based gate: P75 of ALL reranked scores. Top-quartile floor.
        rank_p75_floor(reranked)
    } else {
        cfg.ask_min_relevance_score + cfg.ask_fulldoc_skip_score_delta
    };

    let any_below = top.iter().any(|c| c.rerank_score < floor);
    if any_below {
        return SkipDecision::keep("low_top_scores");
    }

    SkipDecision::skip()
}

/// Compute the 75th-percentile floor on `rerank_score` across `candidates`.
/// Used as the rank-based quality threshold for the RRF skip gate where
/// absolute scores are unitless (rank-fusion output, not cosine).
///
/// Uses `partial_cmp` and treats NaN as the smallest value so a stray NaN
/// can't masquerade as a top-quartile score and let the gate fire spuriously.
fn rank_p75_floor(candidates: &[AskCandidate]) -> f64 {
    if candidates.is_empty() {
        return f64::INFINITY; // gate cannot pass on empty set
    }
    let mut scores: Vec<f64> = candidates.iter().map(|c| c.rerank_score).collect();
    scores.sort_by(|a: &f64, b: &f64| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Less));
    // P75 index: ceil(0.75 * (n - 1)) lets us pick the boundary score whose
    // rank is at the start of the top quartile. With n=4 this picks index 2
    // (the third-best), which is the natural "top quartile floor".
    let n = scores.len();
    let idx = ((n as f64 - 1.0) * 0.75).ceil() as usize;
    scores[idx.min(n - 1)]
}

pub(super) fn push_context_entry(
    entries: &mut Vec<(f64, String)>,
    context_char_count: &mut usize,
    score: f64,
    entry: String,
    separator: &str,
    max_chars: usize,
) -> bool {
    let projected = if entries.is_empty() {
        entry.len()
    } else {
        *context_char_count + separator.len() + entry.len()
    };
    if projected > max_chars {
        return false;
    }
    entries.push((score, entry));
    *context_char_count = projected;
    true
}

pub(super) fn should_inject_supplemental(
    context_char_count: usize,
    max_context_chars: usize,
    full_docs_selected: usize,
    top_chunks_selected: usize,
) -> bool {
    if max_context_chars == 0 {
        return false;
    }
    let within_budget =
        context_char_count * 100 < max_context_chars * SUPPLEMENTAL_CONTEXT_BUDGET_PCT;
    let coverage_needs_backfill =
        full_docs_selected == 0 || top_chunks_selected < SUPPLEMENTAL_MIN_TOP_CHUNKS_FOR_COVERAGE;
    within_budget && coverage_needs_backfill
}

#[cfg(test)]
pub(super) fn query_requests_low_signal_sources(query_tokens: &[String], raw_query: &str) -> bool {
    ranking::query_wants_low_signal_sources(query_tokens, raw_query)
}

#[cfg(test)]
pub(super) fn url_matches_domain_list(url: &str, domains: &[String]) -> bool {
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

#[cfg(test)]
fn host_from_url(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|h| h.to_ascii_lowercase()))
}

#[cfg(test)]
pub(super) fn top_domains(candidates: &[AskCandidate], limit: usize) -> Vec<String> {
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

#[cfg(test)]
pub(super) fn authoritative_ratio(candidates: &[AskCandidate], domains: &[String]) -> f64 {
    if candidates.is_empty() || domains.is_empty() {
        return 0.0;
    }
    let authoritative = candidates
        .iter()
        .filter(|candidate| url_matches_domain_list(&candidate.url, domains))
        .count();
    authoritative as f64 / candidates.len() as f64
}

#[cfg(test)]
fn candidate_topical_overlap_count(candidate: &AskCandidate, query_tokens: &[String]) -> usize {
    query_tokens
        .iter()
        .filter(|token| token.len() >= 3)
        .filter(|token| {
            candidate.url_tokens.contains(token.as_str())
                || candidate.chunk_tokens.contains(token.as_str())
        })
        .count()
}

#[cfg(test)]
pub(super) fn candidate_has_topical_overlap(
    candidate: &AskCandidate,
    query_tokens: &[String],
) -> bool {
    if query_tokens.is_empty() {
        return true;
    }
    let topical_token_count = query_tokens.iter().filter(|token| token.len() >= 3).count();
    if topical_token_count == 0 {
        return true;
    }
    let overlap = candidate_topical_overlap_count(candidate, query_tokens);
    let coverage = overlap as f64 / topical_token_count as f64;
    match topical_token_count {
        0 => true,
        1 | 2 => overlap >= 1,
        3 | 4 => overlap >= 1 || coverage >= 0.5,
        _ => overlap >= 2 && coverage >= 0.34,
    }
}

#[cfg(test)]
mod tests;
