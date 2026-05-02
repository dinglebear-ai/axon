use crate::crates::vector::ops::{qdrant, ranking};
use spider::url::Url;
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
pub(crate) struct RetrievedCandidate {
    pub(crate) candidate: ranking::AskCandidate,
    pub(crate) chunk_index: Option<i64>,
}

pub(crate) struct CandidateBuildResult {
    pub(crate) candidates: Vec<RetrievedCandidate>,
    pub(crate) dropped_by_allowlist: usize,
}

pub(crate) struct CandidateBuildPolicy<'a> {
    pub(crate) allow_low_signal: bool,
    pub(crate) authoritative_allowlist: &'a [String],
}

pub(crate) struct CandidateScorePolicy<'a> {
    pub(crate) authoritative_domains: &'a [String],
    pub(crate) authoritative_boost: f64,
    pub(crate) min_relevance_score: Option<f64>,
    pub(crate) require_topical_overlap: bool,
}

pub(crate) fn build_candidates_from_hits(
    hits: Vec<qdrant::QdrantSearchHit>,
    policy: &CandidateBuildPolicy<'_>,
) -> CandidateBuildResult {
    let mut candidates = Vec::new();
    let mut dropped_by_allowlist = 0usize;
    for hit in hits {
        let url = qdrant::payload_url_typed(&hit.payload).to_string();
        let chunk_text = qdrant::payload_text_typed(&hit.payload).to_string();
        if url.is_empty() || chunk_text.len() < 40 {
            continue;
        }
        if !policy.allow_low_signal && ranking::is_low_signal_url(&url) {
            continue;
        }
        if !url_matches_domain_list(&url, policy.authoritative_allowlist) {
            dropped_by_allowlist += 1;
            continue;
        }
        let path = ranking::extract_path_from_url(&url);
        let url_tokens = ranking::tokenize_path_set(&path);
        let chunk_tokens = ranking::tokenize_text_set(&chunk_text);
        candidates.push(RetrievedCandidate {
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
        });
    }
    CandidateBuildResult {
        candidates,
        dropped_by_allowlist,
    }
}

/// Merge secondary candidates into primary, deduplicating by (url, chunk prefix).
/// Primary candidates win; secondary only adds chunks not already present.
pub(crate) fn merge_candidates(
    primary: Vec<RetrievedCandidate>,
    secondary: Vec<RetrievedCandidate>,
) -> Vec<RetrievedCandidate> {
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
    for c in primary {
        let key = prefix_key(&c.candidate.url, &c.candidate.chunk_text);
        if seen.insert(key) {
            deduped.push(c);
        }
    }
    for c in secondary {
        let key = prefix_key(&c.candidate.url, &c.candidate.chunk_text);
        if seen.insert(key) {
            deduped.push(c);
        }
    }
    deduped
}

pub(crate) fn score_and_filter_candidates(
    candidates: &[RetrievedCandidate],
    query_tokens: &[String],
    policy: &CandidateScorePolicy<'_>,
) -> Vec<RetrievedCandidate> {
    let raw_candidates = candidates
        .iter()
        .map(|candidate| &candidate.candidate)
        .collect::<Vec<_>>();
    let scored = ranking::score_ask_candidate_refs(
        &raw_candidates,
        query_tokens,
        policy.authoritative_domains,
        policy.authoritative_boost,
    );

    scored
        .into_iter()
        .filter(|(idx, score)| {
            policy
                .min_relevance_score
                .is_none_or(|min_score| *score >= min_score)
                && (!policy.require_topical_overlap
                    || candidate_has_topical_overlap(raw_candidates[*idx], query_tokens))
        })
        .map(|(idx, score)| {
            let mut candidate = candidates[idx].clone();
            candidate.candidate.rerank_score = score;
            candidate
        })
        .collect()
}

pub(crate) fn query_allows_low_signal(query_tokens: &[String], raw_query: &str) -> bool {
    ranking::query_wants_low_signal_sources(query_tokens, raw_query)
}

pub(crate) fn candidates_only(candidates: &[RetrievedCandidate]) -> Vec<ranking::AskCandidate> {
    candidates
        .iter()
        .map(|candidate| candidate.candidate.clone())
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

fn host_from_url(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|h| h.to_ascii_lowercase()))
}

fn candidate_topical_overlap_count(
    candidate: &ranking::AskCandidate,
    query_tokens: &[String],
) -> usize {
    query_tokens
        .iter()
        .filter(|token| {
            candidate.url_tokens.contains(token.as_str())
                || candidate.chunk_tokens.contains(token.as_str())
        })
        .count()
}

pub(crate) fn candidate_has_topical_overlap(
    candidate: &ranking::AskCandidate,
    query_tokens: &[String],
) -> bool {
    if query_tokens.is_empty() {
        return true;
    }
    let overlap = candidate_topical_overlap_count(candidate, query_tokens);
    let coverage = overlap as f64 / query_tokens.len() as f64;
    match query_tokens.len() {
        0 => true,
        1 | 2 => overlap >= 1,
        3 | 4 => overlap >= 1 || coverage >= 0.5,
        _ => overlap >= 2 && coverage >= 0.34,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn make_candidate(url: &str, chunk: &str, score: f64) -> RetrievedCandidate {
        RetrievedCandidate {
            candidate: ranking::AskCandidate {
                score,
                url: url.to_string(),
                path: ranking::extract_path_from_url(url),
                chunk_text: chunk.to_string(),
                url_tokens: ranking::tokenize_path_set(url),
                chunk_tokens: ranking::tokenize_text_set(chunk),
                rerank_score: score,
            },
            chunk_index: Some(42),
        }
    }

    #[test]
    fn merge_candidates_dedupes_within_primary() {
        let primary = vec![
            make_candidate("https://a.test/p", "alpha bravo charlie", 0.9),
            make_candidate("https://a.test/p", "alpha bravo charlie", 0.8),
            make_candidate("https://b.test/p", "delta echo foxtrot", 0.7),
        ];
        let merged = merge_candidates(primary, vec![]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn merge_candidates_handles_multibyte_chunk_prefix() {
        let prefix = "あ".repeat(40);
        let primary = vec![make_candidate("https://a.test/p", &prefix, 0.9)];
        let secondary = vec![make_candidate("https://a.test/p", &prefix, 0.8)];
        let merged = merge_candidates(primary, secondary);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn score_policy_can_apply_threshold_and_topical_overlap() {
        let candidates = vec![
            make_candidate(
                "https://docs.example.com/rust",
                "rust async runtime details long enough to keep",
                0.8,
            ),
            make_candidate(
                "https://docs.example.com/python",
                "python decorators reference long enough to keep",
                0.9,
            ),
        ];
        let query_tokens = vec!["rust".to_string(), "async".to_string()];
        let policy = CandidateScorePolicy {
            authoritative_domains: &[],
            authoritative_boost: 0.0,
            min_relevance_score: Some(0.0),
            require_topical_overlap: true,
        };
        let selected = score_and_filter_candidates(&candidates, &query_tokens, &policy);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].candidate.url, "https://docs.example.com/rust");
    }

    #[test]
    fn score_policy_can_disable_threshold_for_query_modes() {
        let candidates = vec![make_candidate(
            "https://docs.example.com/rust",
            "rust async runtime details long enough to keep",
            0.1,
        )];
        let query_tokens = vec!["rust".to_string()];
        let policy = CandidateScorePolicy {
            authoritative_domains: &[],
            authoritative_boost: 0.0,
            min_relevance_score: None,
            require_topical_overlap: true,
        };
        assert_eq!(
            score_and_filter_candidates(&candidates, &query_tokens, &policy).len(),
            1
        );
    }

    #[test]
    fn candidate_has_topical_overlap_chunk_tokens_count_toward_overlap() {
        let candidate = ranking::AskCandidate {
            score: 0.5,
            url: "https://example.com".to_string(),
            path: String::new(),
            chunk_text: String::new(),
            url_tokens: HashSet::new(),
            chunk_tokens: HashSet::from(["rust".to_string()]),
            rerank_score: 0.0,
        };
        assert!(candidate_has_topical_overlap(
            &candidate,
            &["rust".to_string()]
        ));
    }
}
