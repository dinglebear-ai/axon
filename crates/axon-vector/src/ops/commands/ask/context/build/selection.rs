use super::route_score::{
    dominant_retrieval_hosts, final_path_entity_match, full_doc_entity_tokens,
    full_doc_selection_score, full_doc_source_key, host_from_url, token_matches_path_token,
};
use crate::ops::ranking;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy)]
pub(in crate::ops::commands::ask::context) struct SelectionPolicy {
    pub prefer_authoritative: bool,
    pub max_docs_per_domain: usize,
}

impl Default for SelectionPolicy {
    fn default() -> Self {
        Self {
            prefer_authoritative: true,
            max_docs_per_domain: 3,
        }
    }
}

pub fn select_context_indices(
    reranked: &[ranking::AskCandidate],
    query_tokens: &[String],
    chunk_limit: usize,
    full_doc_limit: usize,
    policy: SelectionPolicy,
) -> (Vec<usize>, Vec<usize>) {
    let top_chunk_indices = ranking::select_diverse_candidates(reranked, chunk_limit, 1);
    // Full-doc indices are selected independently from the full reranked pool.
    // The old URL-exclusion caused top_full_doc_indices=[] for narrow-domain
    // queries (all top URLs already in chunk slots), silently skipping the
    // full-doc Qdrant fetch (context_build_ms ≈ 5ms).
    // append_top_chunks_to_context at line 219 already skips snippet entries
    // for URLs in planned_full_doc_urls — no duplication occurs.
    // Enable ask_fulldoc_skip_enabled to restore fast-path when top chunks
    // already provide sufficient coverage.
    let full_doc_candidate_indices = full_doc_candidate_indices(reranked, query_tokens);
    let mut top_full_doc_indices = select_full_doc_indices_with_policy(
        reranked,
        &full_doc_candidate_indices,
        full_doc_limit,
        policy,
    );
    include_preferred_top_chunk_docs(
        reranked,
        query_tokens,
        &dominant_retrieval_hosts(reranked),
        &top_chunk_indices,
        &mut top_full_doc_indices,
        full_doc_limit,
        policy,
    );
    (top_chunk_indices, top_full_doc_indices)
}

fn select_full_doc_indices_with_policy(
    reranked: &[ranking::AskCandidate],
    candidate_indices: &[usize],
    full_doc_limit: usize,
    policy: SelectionPolicy,
) -> Vec<usize> {
    if full_doc_limit == 0 {
        return Vec::new();
    }
    let max_docs_per_domain = policy.max_docs_per_domain.max(1);
    let mut selected = Vec::new();
    let mut deferred = Vec::new();
    for &idx in candidate_indices {
        if selected.len() >= full_doc_limit {
            break;
        }
        if url_already_selected(reranked, &selected, idx) {
            continue;
        }
        if domain_count_for_selected(reranked, &selected, idx) >= max_docs_per_domain {
            deferred.push(idx);
            continue;
        }
        selected.push(idx);
    }
    for idx in deferred {
        if selected.len() >= full_doc_limit {
            break;
        }
        if !url_already_selected(reranked, &selected, idx) {
            selected.push(idx);
        }
    }
    selected
}

fn url_already_selected(
    reranked: &[ranking::AskCandidate],
    selected: &[usize],
    candidate_idx: usize,
) -> bool {
    url_already_selected_except(reranked, selected, None, candidate_idx)
}

fn url_already_selected_except(
    reranked: &[ranking::AskCandidate],
    selected: &[usize],
    exclude_selected_at: Option<usize>,
    candidate_idx: usize,
) -> bool {
    selected.iter().enumerate().any(|(selected_at, &idx)| {
        Some(selected_at) != exclude_selected_at
            && full_doc_source_key(&reranked[idx].url)
                == full_doc_source_key(&reranked[candidate_idx].url)
    })
}

fn domain_count_for_selected(
    reranked: &[ranking::AskCandidate],
    selected: &[usize],
    candidate_idx: usize,
) -> usize {
    domain_count_for_selected_except(reranked, selected, None, candidate_idx)
}

fn domain_count_for_selected_except(
    reranked: &[ranking::AskCandidate],
    selected: &[usize],
    exclude_selected_at: Option<usize>,
    candidate_idx: usize,
) -> usize {
    let Some(candidate_host) = host_from_url(&reranked[candidate_idx].url) else {
        return 0;
    };
    selected
        .iter()
        .enumerate()
        .filter(|(selected_at, _)| Some(*selected_at) != exclude_selected_at)
        .filter(|&(_, &idx)| {
            host_from_url(&reranked[idx].url).as_deref() == Some(candidate_host.as_str())
        })
        .count()
}

fn full_doc_candidate_indices(
    reranked: &[ranking::AskCandidate],
    query_tokens: &[String],
) -> Vec<usize> {
    let dominant_hosts = dominant_retrieval_hosts(reranked);
    let entity_tokens = full_doc_entity_tokens(query_tokens);
    let mut scored = reranked
        .iter()
        .enumerate()
        .map(|(idx, candidate)| {
            let final_path_match = final_path_entity_match(&candidate.url, &entity_tokens);
            (
                idx,
                is_dominant_host(&candidate.url, &candidate.path, &dominant_hosts),
                final_path_match
                    .as_ref()
                    .is_some_and(|entity_match| entity_match.all_match),
                full_doc_selection_score(candidate, query_tokens, &dominant_hosts),
            )
        })
        .collect::<Vec<_>>();
    scored.sort_by(
        |(idx_a, dominant_a, exact_a, score_a), (idx_b, dominant_b, exact_b, score_b)| {
            dominant_b
                .cmp(dominant_a)
                .then_with(|| {
                    score_b
                        .partial_cmp(score_a)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| exact_b.cmp(exact_a))
                .then_with(|| idx_a.cmp(idx_b))
        },
    );
    scored.into_iter().map(|(idx, _, _, _)| idx).collect()
}

fn is_dominant_host(url: &str, path: &str, dominant_hosts: &HashSet<String>) -> bool {
    // A VCS-mirror copy never counts as the dominant/canonical source even when
    // its host (e.g. github.com) holds the most chunks — that "dominance" is an
    // artifact of the index mirroring docs, not of the page being authoritative.
    if super::super::dedup::is_mirror_shaped(url, path) {
        return false;
    }
    dominant_hosts.is_empty()
        || host_from_url(url).is_some_and(|host| dominant_hosts.contains(&host))
}

fn include_preferred_top_chunk_docs(
    reranked: &[ranking::AskCandidate],
    query_tokens: &[String],
    dominant_hosts: &HashSet<String>,
    top_chunk_indices: &[usize],
    top_full_doc_indices: &mut Vec<usize>,
    full_doc_limit: usize,
    policy: SelectionPolicy,
) {
    if full_doc_limit == 0 || !policy.prefer_authoritative {
        return;
    }
    let entity_tokens = full_doc_entity_tokens(query_tokens);
    if entity_tokens.is_empty() || dominant_hosts.is_empty() {
        return;
    }

    let mut preferred = top_chunk_indices
        .iter()
        .copied()
        .filter(|&idx| {
            let candidate = &reranked[idx];
            !super::super::dedup::is_mirror_shaped(&candidate.url, &candidate.path)
                && host_from_url(&candidate.url).is_some_and(|host| dominant_hosts.contains(&host))
                && entity_tokens
                    .iter()
                    .any(|token| token_matches_path_token(token, &candidate.url_tokens))
        })
        .collect::<Vec<_>>();
    preferred.sort_by(|&idx_a, &idx_b| {
        full_doc_selection_score(&reranked[idx_b], query_tokens, dominant_hosts)
            .partial_cmp(&full_doc_selection_score(
                &reranked[idx_a],
                query_tokens,
                dominant_hosts,
            ))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for idx in preferred {
        if top_full_doc_indices
            .iter()
            .any(|&selected_idx| reranked[selected_idx].url == reranked[idx].url)
        {
            continue;
        }
        if top_full_doc_indices.len() < full_doc_limit {
            if !can_add_full_doc_index(reranked, top_full_doc_indices, idx, policy) {
                continue;
            }
            top_full_doc_indices.push(idx);
            continue;
        }
        if let Some(replace_at) = replacement_slot_for_preferred_doc(
            reranked,
            query_tokens,
            dominant_hosts,
            top_full_doc_indices,
            idx,
            policy,
        ) {
            top_full_doc_indices[replace_at] = idx;
        }
    }
}

fn replacement_slot_for_preferred_doc(
    reranked: &[ranking::AskCandidate],
    query_tokens: &[String],
    dominant_hosts: &HashSet<String>,
    top_full_doc_indices: &[usize],
    candidate_idx: usize,
    policy: SelectionPolicy,
) -> Option<usize> {
    let entity_tokens = full_doc_entity_tokens(query_tokens);
    top_full_doc_indices
        .iter()
        .enumerate()
        .filter(|(replace_at, idx_ref)| {
            if !can_replace_full_doc_index(
                reranked,
                top_full_doc_indices,
                *replace_at,
                candidate_idx,
                policy,
            ) {
                return false;
            }
            let idx = **idx_ref;
            let candidate = &reranked[idx];
            !host_from_url(&candidate.url).is_some_and(|host| dominant_hosts.contains(&host))
                || !entity_tokens
                    .iter()
                    .any(|token| token_matches_path_token(token, &candidate.url_tokens))
        })
        .min_by(|(_, idx_a_ref), (_, idx_b_ref)| {
            let idx_a = **idx_a_ref;
            let idx_b = **idx_b_ref;
            full_doc_selection_score(&reranked[idx_a], query_tokens, dominant_hosts)
                .partial_cmp(&full_doc_selection_score(
                    &reranked[idx_b],
                    query_tokens,
                    dominant_hosts,
                ))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(replace_at, _)| replace_at)
}

fn can_add_full_doc_index(
    reranked: &[ranking::AskCandidate],
    selected: &[usize],
    candidate_idx: usize,
    policy: SelectionPolicy,
) -> bool {
    !url_already_selected(reranked, selected, candidate_idx)
        && domain_count_for_selected(reranked, selected, candidate_idx)
            < policy.max_docs_per_domain.max(1)
}

fn can_replace_full_doc_index(
    reranked: &[ranking::AskCandidate],
    selected: &[usize],
    replace_at: usize,
    candidate_idx: usize,
    policy: SelectionPolicy,
) -> bool {
    !url_already_selected_except(reranked, selected, Some(replace_at), candidate_idx)
        && domain_count_for_selected_except(reranked, selected, Some(replace_at), candidate_idx)
            < policy.max_docs_per_domain.max(1)
}

pub fn planned_full_doc_urls(
    reranked: &[ranking::AskCandidate],
    top_full_doc_indices: &[usize],
    skip_full_doc_fetch: bool,
) -> HashSet<String> {
    if skip_full_doc_fetch {
        return HashSet::new();
    }

    top_full_doc_indices
        .iter()
        .filter_map(|&idx| reranked.get(idx).map(|candidate| candidate.url.clone()))
        .collect()
}

pub fn collect_supplemental_candidate_indices(
    reranked: &[ranking::AskCandidate],
    inserted_full_doc_urls: &HashSet<String>,
    min_supplemental_score: Option<f64>,
) -> Vec<usize> {
    reranked
        .iter()
        .enumerate()
        .filter(|(_, candidate)| {
            !inserted_full_doc_urls.contains(&candidate.url)
                && min_supplemental_score.is_none_or(|floor| candidate.rerank_score >= floor)
        })
        .map(|(idx, _)| idx)
        .collect::<Vec<_>>()
}
