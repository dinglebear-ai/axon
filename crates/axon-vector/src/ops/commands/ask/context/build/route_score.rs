use crate::ops::ranking;
use spider::url::Url;
use std::collections::HashSet;

pub(super) fn full_doc_source_key(url: &str) -> String {
    let Ok(parsed) = Url::parse(url) else {
        return strip_route_variant_suffix(url.trim_end_matches('/')).to_ascii_lowercase();
    };
    let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
    let mut path = parsed.path().trim_end_matches('/').to_ascii_lowercase();
    if let Some(stripped) = path
        .strip_suffix("/index.html")
        .or_else(|| path.strip_suffix("/index"))
    {
        path = stripped.to_string();
    }
    path = strip_route_variant_suffix(&path).to_string();
    format!("{host}{path}")
}

fn strip_route_variant_suffix(path: &str) -> &str {
    path.strip_suffix(".md")
        .or_else(|| path.strip_suffix(".mdx"))
        .or_else(|| path.strip_suffix(".html"))
        .unwrap_or(path)
}

pub(super) fn host_from_url(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|host| host.to_ascii_lowercase()))
}

pub(super) fn full_doc_entity_tokens(query_tokens: &[String]) -> Vec<&str> {
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

fn is_route_noise_token(token: &str) -> bool {
    token.chars().all(|ch| ch.is_ascii_digit())
        || token
            .strip_prefix('v')
            .is_some_and(|rest| rest.chars().all(|ch| ch.is_ascii_digit()))
}

pub(super) fn token_matches_path_token(
    query_token: &str,
    candidate_tokens: &HashSet<String>,
) -> bool {
    if candidate_tokens.contains(query_token) {
        return true;
    }
    singular_variant(query_token).is_some_and(|singular| candidate_tokens.contains(singular))
        || candidate_tokens
            .iter()
            .any(|candidate_token| singular_variant(candidate_token) == Some(query_token))
}

fn token_matches_path_entity(query_token: &str, path_token: &str) -> bool {
    query_token == path_token
        || singular_variant(query_token) == Some(path_token)
        || singular_variant(path_token) == Some(query_token)
}

fn singular_variant(token: &str) -> Option<&str> {
    token.strip_suffix('s').filter(|variant| variant.len() >= 3)
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
        + final_path_entity_match_adjustment(&candidate.url, &query_tokens)
        + dominant_host_adjustment(&candidate.url, &candidate.path, dominant_hosts)
        + if coverage >= 0.25 { 0.18 } else { 0.0 }
}

fn final_path_entity_match_adjustment(url: &str, query_tokens: &[&str]) -> f64 {
    let Some(entity_match) = final_path_entity_match(url, query_tokens) else {
        return 0.0;
    };

    // If the final route segment is exactly the thing the user asked for
    // (`plugins` for "create a plugin"), boost it within the numeric score
    // instead of letting it bypass retrieval evidence entirely.
    (entity_match.precision * 1.15) - (entity_match.unmatched as f64 * 0.6).min(0.6)
}

#[derive(Debug, Clone, Copy)]
pub(super) struct FinalPathEntityMatch {
    pub(super) all_match: bool,
    unmatched: usize,
    precision: f64,
}

pub(super) fn final_path_entity_match(
    url: &str,
    query_tokens: &[&str],
) -> Option<FinalPathEntityMatch> {
    let mut path_tokens = final_path_entity_tokens(url)?;
    if path_tokens.is_empty() {
        return None;
    }
    path_tokens.sort();
    path_tokens.dedup();

    let matched = path_tokens
        .iter()
        .filter(|path_token| {
            query_tokens
                .iter()
                .any(|query_token| token_matches_path_entity(query_token, path_token.as_str()))
        })
        .count();
    let unmatched = path_tokens.len().saturating_sub(matched);
    let precision = matched as f64 / path_tokens.len() as f64;

    Some(FinalPathEntityMatch {
        all_match: unmatched == 0,
        unmatched,
        precision,
    })
}

fn final_path_entity_tokens(url: &str) -> Option<Vec<String>> {
    let parsed = Url::parse(url).ok()?;
    let segments = parsed
        .path_segments()?
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    let mut segment = *segments.last()?;
    let stem = strip_known_doc_extension(segment);
    if is_index_like_route_segment(stem) && segments.len() >= 2 {
        segment = segments[segments.len() - 2];
    } else {
        segment = stem;
    }
    let tokens = ranking::tokenize_text_set(segment)
        .into_iter()
        .filter(|token| !is_broad_full_doc_token(token) && !is_route_noise_token(token))
        .collect::<Vec<_>>();
    Some(tokens)
}

fn strip_known_doc_extension(segment: &str) -> &str {
    let Some((stem, extension)) = segment.rsplit_once('.') else {
        return segment;
    };
    if matches!(
        extension.to_ascii_lowercase().as_str(),
        "adoc" | "html" | "md" | "mdx" | "rst" | "txt"
    ) {
        stem
    } else {
        segment
    }
}

fn is_index_like_route_segment(segment: &str) -> bool {
    matches!(
        strip_known_doc_extension(segment)
            .to_ascii_lowercase()
            .as_str(),
        "index" | "overview" | "readme"
    )
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

fn dominant_host_adjustment(url: &str, path: &str, dominant_hosts: &HashSet<String>) -> f64 {
    // Mirror copies are demoted, not boosted: they must not win the canonical
    // full-doc slot just because their host dominates the (mirror-heavy) index.
    if super::super::dedup::is_mirror_shaped(url, path) {
        return -0.35;
    }
    if dominant_hosts.is_empty() {
        return 0.0;
    }
    if host_from_url(url).is_some_and(|host| dominant_hosts.contains(&host)) {
        1.0
    } else {
        -0.35
    }
}
