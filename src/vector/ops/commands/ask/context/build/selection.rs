use crate::vector::ops::ranking;
use spider::url::Url;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy)]
pub(in crate::vector::ops::commands::ask::context) struct SelectionPolicy {
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
        Some(selected_at) != exclude_selected_at && reranked[idx].url == reranked[candidate_idx].url
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
    let mut scored = reranked
        .iter()
        .enumerate()
        .map(|(idx, candidate)| {
            (
                idx,
                is_dominant_host(&candidate.url, &dominant_hosts),
                full_doc_selection_score(candidate, query_tokens, &dominant_hosts),
            )
        })
        .collect::<Vec<_>>();
    scored.sort_by(
        |(idx_a, dominant_a, score_a), (idx_b, dominant_b, score_b)| {
            dominant_b
                .cmp(dominant_a)
                .then_with(|| {
                    score_b
                        .partial_cmp(score_a)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| idx_a.cmp(idx_b))
        },
    );
    scored.into_iter().map(|(idx, _, _)| idx).collect()
}

fn is_dominant_host(url: &str, dominant_hosts: &HashSet<String>) -> bool {
    dominant_hosts.is_empty()
        || host_from_url(url).is_some_and(|host| dominant_hosts.contains(&host))
}

pub(super) fn full_doc_selection_score(
    candidate: &ranking::AskCandidate,
    query_tokens: &[String],
    dominant_hosts: &HashSet<String>,
) -> f64 {
    let query_tokens = full_doc_entity_tokens(query_tokens);
    if query_tokens.is_empty() {
        return candidate.rerank_score;
    }

    let url_matches = query_tokens
        .iter()
        .filter(|token| token_matches_path_token(token, &candidate.url_tokens))
        .count();
    let chunk_matches = query_tokens
        .iter()
        .filter(|token| token_matches_path_token(token, &candidate.chunk_tokens))
        .count();
    let coverage = url_matches as f64 / query_tokens.len() as f64;

    // Full-doc fetch is a canonical-source selection step, not just another
    // chunk-ranking step. Favor pages whose path/title-like URL tokens match
    // the query entities, then use body-token coverage as a smaller tiebreaker.
    candidate.rerank_score
        + (url_matches as f64 * 0.55).min(1.65)
        + (chunk_matches as f64 * 0.03).min(0.18)
        + dominant_host_adjustment(&candidate.url, dominant_hosts)
        + if coverage >= 0.25 { 0.18 } else { 0.0 }
}

pub(super) fn dominant_retrieval_hosts(reranked: &[ranking::AskCandidate]) -> HashSet<String> {
    let mut counts = std::collections::HashMap::<String, usize>::new();
    for candidate in reranked.iter().take(50) {
        if let Some(host) = host_from_url(&candidate.url) {
            *counts.entry(host).or_insert(0) += 1;
        }
    }

    counts
        .into_iter()
        .filter_map(|(host, count)| (count >= 5).then_some(host))
        .collect()
}

fn dominant_host_adjustment(url: &str, dominant_hosts: &HashSet<String>) -> f64 {
    if dominant_hosts.is_empty() {
        return 0.0;
    }
    if host_from_url(url).is_some_and(|host| dominant_hosts.contains(&host)) {
        1.0
    } else {
        -0.35
    }
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
            host_from_url(&candidate.url).is_some_and(|host| dominant_hosts.contains(&host))
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

fn host_from_url(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|host| host.to_ascii_lowercase()))
}

fn full_doc_entity_tokens(query_tokens: &[String]) -> Vec<&str> {
    query_tokens
        .iter()
        .map(String::as_str)
        .filter(|token| !is_broad_full_doc_token(token))
        .collect()
}

fn is_broad_full_doc_token(token: &str) -> bool {
    matches!(
        token,
        // Product/navigation words tend to appear in many URLs and should not
        // decide which document deserves full-doc expansion.
        "claude"
            | "code"
            | "docs"
            | "doc"
            | "documentation"
            | "guide"
            | "guides"
            | "setup"
            | "using"
            | "use"
    )
}

fn token_matches_path_token(query_token: &str, candidate_tokens: &HashSet<String>) -> bool {
    if candidate_tokens.contains(query_token) {
        return true;
    }
    singular_variant(query_token).is_some_and(|singular| candidate_tokens.contains(singular))
        || candidate_tokens
            .iter()
            .any(|candidate_token| singular_variant(candidate_token) == Some(query_token))
}

fn singular_variant(token: &str) -> Option<&str> {
    token.strip_suffix('s').filter(|variant| variant.len() >= 3)
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
