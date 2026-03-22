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

pub fn tokenize_query(text: &str) -> Vec<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| t.len() >= 3 && !STOP_WORDS.contains(*t))
        .map(str::to_string)
        .collect()
}

pub fn tokenize_text_set(text: &str) -> HashSet<String> {
    // Build HashSet directly without intermediate Vec allocation (LOW-2).
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

/// Rerank candidates in place and return sorted. Takes ownership to avoid cloning
/// all candidate structs (each contains ~2KB chunk_text + HashSets).
pub fn rerank_ask_candidates(
    candidates: &[AskCandidate],
    query_tokens: &[String],
    authoritative_domains: &[String],
    authoritative_boost: f64,
) -> Vec<AskCandidate> {
    if query_tokens.is_empty() {
        return candidates.to_vec();
    }

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

    // Compute rerank scores on a parallel Vec<(usize, f64)> to avoid cloning all
    // candidates (HIGH-1). Only the final sorted output clones the selected ones.
    let mut scored: Vec<(usize, f64)> = candidates
        .iter()
        .enumerate()
        .map(|(i, candidate)| {
            let mut lexical_boost = 0.0f64;
            for token in query_tokens {
                if candidate.url_tokens.contains(token) {
                    lexical_boost += LEXICAL_URL_TOKEN_BOOST;
                }
                if candidate.chunk_tokens.contains(token) {
                    lexical_boost += LEXICAL_CHUNK_TOKEN_BOOST;
                }
            }
            lexical_boost = lexical_boost.min(LEXICAL_BOOST_CAP);

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
            let authority_boost = if host_matches_domains(&candidate.url, &normalized_domains) {
                authoritative_boost.max(0.0)
            } else {
                0.0
            };

            // Phrase matching: use case-insensitive byte search instead of
            // allocating a full lowercased copy of chunk_text (HIGH-2).
            let phrase_boost =
                if phrase_threshold && ascii_lowercase_contains(&candidate.chunk_text, &phrase) {
                    PHRASE_MATCH_BOOST
                } else {
                    0.0
                };

            let rerank_score =
                candidate.score + lexical_boost + docs_boost + phrase_boost + authority_boost;
            (i, rerank_score)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    scored
        .into_iter()
        .map(|(idx, score)| {
            let mut c = candidates[idx].clone();
            c.rerank_score = score;
            c
        })
        .collect()
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
    if candidate_indices.len() <= target_count {
        return candidate_indices.to_vec();
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

#[cfg(test)]
#[path = "ranking_test.rs"]
mod tests; // tests live in ranking_test.rs (excluded from monolith line-count)
