use super::sparse::STOP_WORDS;
use spider::url::Url;
use std::collections::{HashMap, HashSet};

mod snippet;
pub use snippet::{get_meaningful_snippet, select_best_preview_chunk};

// Reranking boost constants. URL tokens are weighted 3x chunk tokens because
// a query term appearing in the URL path is a stronger signal of topical relevance
// than appearing in body text. The cap prevents lexical boosts from overwhelming
// the cosine similarity score. Use `axon evaluate --ask-diagnostics` to measure
// the impact of retuning these values.
const LEXICAL_URL_TOKEN_BOOST: f64 = 0.045;
const LEXICAL_CHUNK_TOKEN_BOOST: f64 = 0.015;
const LEXICAL_BOOST_CAP: f64 = 0.30;
const DOCS_PATH_BOOST: f64 = 0.04;
const PHRASE_MATCH_BOOST: f64 = 0.06;

#[derive(Debug, Clone)]
pub struct AskCandidate {
    pub score: f64,
    pub url: String,
    pub path: String,
    pub chunk_text: String,
    pub url_tokens: HashSet<String>,
    pub chunk_tokens: HashSet<String>,
    pub rerank_score: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AskScoreBreakdown {
    pub retrieval_score: f64,
    pub lexical_url_token_boost: f64,
    pub lexical_chunk_token_boost: f64,
    pub docs_path_boost: f64,
    pub authority_boost: f64,
    pub phrase_match_boost: f64,
    pub rerank_score: f64,
}

impl AskScoreBreakdown {
    fn retrieval_only(score: f64) -> Self {
        Self {
            retrieval_score: score,
            lexical_url_token_boost: 0.0,
            lexical_chunk_token_boost: 0.0,
            docs_path_boost: 0.0,
            authority_boost: 0.0,
            phrase_match_boost: 0.0,
            rerank_score: score,
        }
    }
}

pub fn tokenize_query(text: &str) -> Vec<String> {
    super::token_policy::query_tokens(text)
}

pub fn tokenize_text_set(text: &str) -> HashSet<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() >= 3 && !STOP_WORDS.contains(*t))
        .map(str::to_string)
        .collect()
}

pub fn extract_path_from_url(path_or_url: &str) -> String {
    Url::parse(path_or_url)
        .ok()
        .map(|u| u.path().to_string())
        .unwrap_or_else(|| path_or_url.to_string())
}

pub fn tokenize_path_set(path_or_url: &str) -> HashSet<String> {
    path_or_url
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() >= 3)
        .map(str::to_string)
        .collect()
}

/// Score candidates without cloning, returning `(index, rerank_score)` pairs
/// sorted by score descending. Caller can filter by threshold before
/// materializing the selected candidates — avoids cloning ~150 candidates
/// (~5-10 KiB each) just to throw most of them away. (bd axon_rust-d71.22)
pub fn score_ask_candidates(
    candidates: &[AskCandidate],
    query_tokens: &[String],
    authoritative_domains: &[String],
    authoritative_boost: f64,
) -> Vec<(usize, f64)> {
    if query_tokens.is_empty() {
        return candidates
            .iter()
            .enumerate()
            .map(|(i, c)| (i, c.score))
            .collect();
    }
    compute_scored_indices(
        candidates.iter().enumerate(),
        query_tokens,
        authoritative_domains,
        authoritative_boost,
    )
}

/// Score borrowed candidates without cloning, returning `(index, rerank_score)`
/// pairs sorted by score descending.
pub fn score_ask_candidate_refs(
    candidates: &[&AskCandidate],
    query_tokens: &[String],
    authoritative_domains: &[String],
    authoritative_boost: f64,
) -> Vec<(usize, f64)> {
    if query_tokens.is_empty() {
        return candidates
            .iter()
            .enumerate()
            .map(|(i, c)| (i, c.score))
            .collect();
    }
    compute_scored_indices(
        candidates.iter().enumerate().map(|(i, c)| (i, *c)),
        query_tokens,
        authoritative_domains,
        authoritative_boost,
    )
}

/// Score borrowed candidates with a component breakdown from the same scoring
/// path as `score_ask_candidate_refs`.
pub fn score_ask_candidate_ref_breakdowns(
    candidates: &[&AskCandidate],
    query_tokens: &[String],
    authoritative_domains: &[String],
    authoritative_boost: f64,
) -> Vec<(usize, AskScoreBreakdown)> {
    if query_tokens.is_empty() {
        return candidates
            .iter()
            .enumerate()
            .map(|(i, c)| (i, AskScoreBreakdown::retrieval_only(c.score)))
            .collect();
    }
    compute_scored_breakdowns(
        candidates.iter().enumerate().map(|(i, c)| (i, *c)),
        query_tokens,
        authoritative_domains,
        authoritative_boost,
    )
}

/// Rerank candidates in place and return sorted. Takes ownership to avoid cloning
/// all candidate structs (each contains ~2KB chunk_text + HashSets).
///
/// Prefer `score_ask_candidates` + selective materialization when the caller
/// will threshold-filter most candidates out.
pub fn rerank_ask_candidates(
    candidates: &[AskCandidate],
    query_tokens: &[String],
    authoritative_domains: &[String],
    authoritative_boost: f64,
) -> Vec<AskCandidate> {
    if query_tokens.is_empty() {
        return candidates.to_vec();
    }

    let scored = compute_scored_indices(
        candidates.iter().enumerate(),
        query_tokens,
        authoritative_domains,
        authoritative_boost,
    );

    scored
        .into_iter()
        .map(|(idx, score)| {
            let mut c = candidates[idx].clone();
            c.rerank_score = score;
            c
        })
        .collect()
}

/// Inner scoring loop shared between `score_ask_candidates` and `rerank_ask_candidates`.
///
/// Returns `(index, rerank_score)` pairs sorted by score descending. Pure
/// computation over `&[AskCandidate]` — no allocations beyond the result Vec
/// and the pre-normalized domain list.
fn compute_scored_indices<'a>(
    candidates: impl IntoIterator<Item = (usize, &'a AskCandidate)>,
    query_tokens: &[String],
    authoritative_domains: &[String],
    authoritative_boost: f64,
) -> Vec<(usize, f64)> {
    compute_scored_breakdowns(
        candidates,
        query_tokens,
        authoritative_domains,
        authoritative_boost,
    )
    .into_iter()
    .map(|(idx, breakdown)| (idx, breakdown.rerank_score))
    .collect()
}

fn compute_scored_breakdowns<'a>(
    candidates: impl IntoIterator<Item = (usize, &'a AskCandidate)>,
    query_tokens: &[String],
    authoritative_domains: &[String],
    authoritative_boost: f64,
) -> Vec<(usize, AskScoreBreakdown)> {
    // Reconstruct joined phrase for verbatim phrase-match boost.
    // Tokens are already lowercased so this matches case-insensitively.
    let phrase = query_tokens.join(" ");
    let phrase_threshold = phrase.len() >= 6 && query_tokens.len() >= 2;

    // Pre-normalize authoritative domains once instead of per-candidate (MED-4).
    let normalized_domains: Vec<String> = authoritative_domains
        .iter()
        .map(|d| d.trim().to_ascii_lowercase())
        .filter(|d| !d.is_empty())
        .collect();

    let mut scored: Vec<(usize, AskScoreBreakdown)> = candidates
        .into_iter()
        .map(|(i, candidate)| {
            let mut lexical_url_token_boost = 0.0f64;
            let mut lexical_chunk_token_boost = 0.0f64;
            for token in query_tokens {
                if candidate.url_tokens.contains(token) {
                    lexical_url_token_boost += LEXICAL_URL_TOKEN_BOOST;
                }
                if candidate.chunk_tokens.contains(token) {
                    lexical_chunk_token_boost += LEXICAL_CHUNK_TOKEN_BOOST;
                }
            }
            let lexical_boost = lexical_url_token_boost + lexical_chunk_token_boost;
            if lexical_boost > LEXICAL_BOOST_CAP {
                let scale = LEXICAL_BOOST_CAP / lexical_boost;
                lexical_url_token_boost *= scale;
                lexical_chunk_token_boost *= scale;
            }

            let docs_boost = if candidate.path.contains("/docs/")
                || candidate.path.contains("/guides/")
                || candidate.path.contains("/api/")
                || candidate.path.contains("/reference/")
            {
                DOCS_PATH_BOOST
            } else {
                0.0
            };

            // Use pre-extracted host from URL and pre-normalized domains (MED-4).
            let authority_boost = authority_boost_for_normalized_domains(
                &candidate.url,
                &normalized_domains,
                authoritative_boost,
            );

            // Phrase matching: use case-insensitive byte search instead of
            // allocating a full lowercased copy of chunk_text (HIGH-2).
            let phrase_boost =
                if phrase_threshold && ascii_lowercase_contains(&candidate.chunk_text, &phrase) {
                    PHRASE_MATCH_BOOST
                } else {
                    0.0
                };

            let rerank_score = candidate.score
                + lexical_url_token_boost
                + lexical_chunk_token_boost
                + docs_boost
                + phrase_boost
                + authority_boost;
            (
                i,
                AskScoreBreakdown {
                    retrieval_score: candidate.score,
                    lexical_url_token_boost,
                    lexical_chunk_token_boost,
                    docs_path_boost: docs_boost,
                    authority_boost,
                    phrase_match_boost: phrase_boost,
                    rerank_score,
                },
            )
        })
        .collect();

    scored.sort_by(|a, b| {
        b.1.rerank_score
            .partial_cmp(&a.1.rerank_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored
}

/// Case-insensitive ASCII substring check without allocating a lowercased copy.
/// Since query tokens are already ASCII-lowered, we only need byte-level comparison.
fn ascii_lowercase_contains(haystack: &str, needle: &str) -> bool {
    if needle.len() > haystack.len() {
        return false;
    }
    let needle_bytes = needle.as_bytes();
    haystack
        .as_bytes()
        .windows(needle_bytes.len())
        .any(|window| {
            window
                .iter()
                .zip(needle_bytes)
                .all(|(h, n)| h.to_ascii_lowercase() == *n)
        })
}

/// Match a URL's host against pre-normalized authoritative domains.
fn host_matches_domains(url: &str, normalized_domains: &[String]) -> bool {
    if normalized_domains.is_empty() {
        return false;
    }
    let host = Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|h| h.to_ascii_lowercase()));
    let Some(host) = host else {
        return false;
    };
    normalized_domains
        .iter()
        .any(|normalized| host == *normalized || host.ends_with(&format!(".{normalized}")))
}

fn authority_boost_for_normalized_domains(
    url: &str,
    normalized_domains: &[String],
    authoritative_boost: f64,
) -> f64 {
    if host_matches_domains(url, normalized_domains) {
        authoritative_boost.max(0.0)
    } else {
        0.0
    }
}

pub fn select_diverse_candidates(
    candidates: &[AskCandidate],
    target_count: usize,
    max_per_url: usize,
) -> Vec<usize> {
    let all_indices = (0..candidates.len()).collect::<Vec<_>>();
    select_diverse_candidates_from_indices(candidates, &all_indices, target_count, max_per_url)
}

pub fn select_diverse_candidates_from_indices(
    candidates: &[AskCandidate],
    candidate_indices: &[usize],
    target_count: usize,
    max_per_url: usize,
) -> Vec<usize> {
    if target_count == 0 || candidate_indices.is_empty() {
        return Vec::new();
    }

    let mut selected: Vec<usize> = Vec::new();
    let mut selected_set: HashSet<usize> = HashSet::new();
    // Use &str references into the candidates slice to avoid cloning URLs (HIGH-3).
    let mut per_url_count: HashMap<&str, usize> = HashMap::new();

    // Pass 1: pick one candidate per unique URL.
    for &candidate_idx in candidate_indices {
        if selected.len() >= target_count {
            break;
        }
        let url = candidates[candidate_idx].url.as_str();
        if per_url_count.contains_key(url) {
            continue;
        }
        selected.push(candidate_idx);
        selected_set.insert(candidate_idx);
        per_url_count.insert(url, 1);
    }

    // Pass 2: fill remaining slots up to max_per_url per URL.
    for &candidate_idx in candidate_indices {
        if selected.len() >= target_count {
            break;
        }
        if selected_set.contains(&candidate_idx) {
            continue;
        }
        let url = candidates[candidate_idx].url.as_str();
        let count = per_url_count.entry(url).or_insert(0);
        if *count >= max_per_url {
            continue;
        }
        *count += 1;
        selected.push(candidate_idx);
        selected_set.insert(candidate_idx);
    }

    selected
}

/// Returns `true` for URLs that are low-signal noise sources and should be
/// excluded from general query results unless the user explicitly requests them.
///
/// Catches:
/// - Session export JSONL files (Claude/Codex/Gemini session logs indexed by `sessions` ingest)
/// - Local `file://` URLs in general (not useful as doc references)
/// - Cached/temp paths
/// - Log files
///
/// The caller is responsible for skipping this filter when the query
/// explicitly requests sessions/logs (e.g. contains "session", "log", "history").
pub fn is_low_signal_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let is_web_url = lower.starts_with("http://") || lower.starts_with("https://");
    lower.starts_with("file://")
        || lower.ends_with(".jsonl")
        || lower.contains("/docs/sessions/")
        || lower.contains("docs/sessions/")
        || lower.contains("/.cache/")
        || lower.contains(".cache/")
        || (!is_web_url && lower.contains("/logs/"))
        || (!is_web_url && lower.ends_with(".log"))
}

/// Returns `true` when the query explicitly asks for session logs, history, or
/// similar low-signal sources that are normally filtered from results.
///
/// Used by both `query` and `ask` paths to decide whether to bypass
/// `is_low_signal_url` filtering for a given request.
pub fn query_wants_low_signal_sources(query_tokens: &[String], raw_query: &str) -> bool {
    if raw_query.to_ascii_lowercase().contains("docs/sessions") {
        return true;
    }
    query_tokens.iter().any(|token| {
        matches!(
            token.as_str(),
            "session" | "sessions" | "log" | "logs" | "history" | "histories"
        )
    })
}

#[cfg(test)]
#[path = "ranking_test.rs"]
mod tests; // tests live in ranking_test.rs (excluded from monolith line-count)
