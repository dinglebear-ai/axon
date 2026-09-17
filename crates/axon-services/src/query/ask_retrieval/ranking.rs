//! Ask-specific post-retrieval ranking and adaptive context policy.

use super::super::query_tokens::{
    identity_tokens, is_generic_authority_token, is_generic_topical_token, query_tokens,
    query_wants_low_signal_sources, tokenize_path_set, tokenize_text_set,
};
use axon_api::{
    AskExplainFilterDecision, AskExplainFilterDecisionKind, AskExplainScoreComponent,
    AskExplainScoreComponentStatus,
};
use axon_core::config::Config;
use axon_core::llm::{SynthesisModelProfile, SynthesisModelTier};
use axon_retrieval::QueryServiceHit;
use reqwest::Url;
use std::collections::{HashMap, HashSet};

const URL_TOKEN_BOOST: f64 = 0.045;
const TEXT_TOKEN_BOOST: f64 = 0.015;
const LEXICAL_BOOST_CAP: f64 = 0.30;
const DOCS_PATH_BOOST: f64 = 0.04;
const PHRASE_MATCH_BOOST: f64 = 0.06;
const PRODUCT_AUTHORITY_BOOST: f64 = 0.35;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AskComplexity {
    Simple,
    Complex,
    Exhaustive,
}
impl AskComplexity {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Simple => "simple",
            Self::Complex => "complex",
            Self::Exhaustive => "exhaustive",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EffectiveAskBudget {
    pub(crate) chunk_limit: usize,
    pub(crate) max_context_chars: usize,
    pub(crate) max_chunk_chars: usize,
}
impl EffectiveAskBudget {
    pub(crate) fn resolve(cfg: &Config, complexity: AskComplexity) -> Self {
        let tier = SynthesisModelProfile::from_config(cfg).tier();
        let (chunks, context_chars, chunk_chars) = match (tier, complexity) {
            (SynthesisModelTier::LocalGemma, AskComplexity::Simple) => (4, 20_000, 5_000),
            (SynthesisModelTier::LocalGemma, AskComplexity::Complex) => (6, 32_000, 6_000),
            (SynthesisModelTier::LocalGemma, AskComplexity::Exhaustive) => (10, 48_000, 7_000),
            (SynthesisModelTier::Small, AskComplexity::Simple) => (5, 24_000, 6_000),
            (SynthesisModelTier::Small, AskComplexity::Complex) => (8, 40_000, 7_000),
            (SynthesisModelTier::Small, AskComplexity::Exhaustive) => (12, 64_000, 8_000),
            (_, AskComplexity::Simple) => (6, 36_000, 7_500),
            (_, AskComplexity::Complex) => (10, 64_000, 9_000),
            (_, AskComplexity::Exhaustive) => (18, 128_000, 12_000),
        };
        Self {
            chunk_limit: cfg.ask_chunk_limit.min(chunks).max(1),
            max_context_chars: cfg.ask_max_context_chars.min(context_chars).max(1),
            max_chunk_chars: chunk_chars.min(cfg.ask_max_context_chars).max(1),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RankedCandidate {
    pub(crate) hit: QueryServiceHit,
    pub(crate) retrieval_rank: usize,
    pub(crate) rerank_rank: Option<usize>,
    pub(crate) rerank_score: f64,
    pub(crate) score_components: Vec<AskExplainScoreComponent>,
    pub(crate) filter_decisions: Vec<AskExplainFilterDecision>,
    pub(crate) configured_authoritative: bool,
    pub(crate) product_authoritative: bool,
    pub(crate) selected_context_rank: Option<usize>,
    pub(crate) selection_reason: Option<String>,
}
impl RankedCandidate {
    pub(crate) fn kept(&self) -> bool {
        self.filter_decisions
            .iter()
            .any(|d| d.kind == AskExplainFilterDecisionKind::Kept)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RankingResult {
    pub(crate) candidates: Vec<RankedCandidate>,
    pub(crate) ranked_indices: Vec<usize>,
    pub(crate) query_tokens: Vec<String>,
    pub(crate) keyword_query: String,
    pub(crate) complexity: AskComplexity,
    pub(crate) effective_budget: EffectiveAskBudget,
    pub(crate) configured_authority_ratio: f64,
    pub(crate) product_authority_ratio: f64,
    pub(crate) top_domains: Vec<String>,
}

pub(crate) fn rank_candidates(
    cfg: &Config,
    question: &str,
    hits: Vec<QueryServiceHit>,
    hybrid: bool,
) -> RankingResult {
    let tokens = query_tokens(question);
    let keyword_query = tokens.join(" ");
    let complexity = classify_complexity(question, &tokens);
    let effective_budget = EffectiveAskBudget::resolve(cfg, complexity);
    let allow_low_signal = query_wants_low_signal_sources(&tokens, question);
    let phrase = tokens.join(" ");
    let normalized_domains = normalized_domains(&cfg.ask_authoritative_domains);
    let configured_boost = cfg.ask_authoritative_boost.clamp(0.0, 0.5);
    let mut candidates = Vec::with_capacity(hits.len());

    for (index, hit) in hits
        .into_iter()
        .take(cfg.ask_candidate_limit.max(1))
        .enumerate()
    {
        let url_tokens = tokenize_path_set(&hit.canonical_uri);
        let chunk_tokens = tokenize_text_set(&hit.text);
        let mut url_boost = 0.0;
        let mut text_boost = 0.0;
        for token in &tokens {
            if url_tokens.contains(token) {
                url_boost += URL_TOKEN_BOOST;
            }
            if chunk_tokens.contains(token) {
                text_boost += TEXT_TOKEN_BOOST;
            }
        }
        let lexical = url_boost + text_boost;
        if lexical > LEXICAL_BOOST_CAP {
            let scale = LEXICAL_BOOST_CAP / lexical;
            url_boost *= scale;
            text_boost *= scale;
        }
        let docs_boost = docs_path_boost(&hit.canonical_uri);
        let configured_authoritative =
            host_matches_domains(&hit.canonical_uri, &normalized_domains);
        let authority_boost = if configured_authoritative {
            configured_boost
        } else {
            0.0
        };
        let product_authoritative = product_authority_match(&hit.canonical_uri, &tokens);
        let product_boost = if product_authoritative {
            PRODUCT_AUTHORITY_BOOST
        } else {
            0.0
        };
        let phrase_boost = if phrase.len() >= 6
            && tokens.len() >= 2
            && ascii_lowercase_contains(&hit.text, &phrase)
        {
            PHRASE_MATCH_BOOST
        } else {
            0.0
        };
        let rerank_score = hit.score
            + url_boost
            + text_boost
            + docs_boost
            + authority_boost
            + product_boost
            + phrase_boost;
        let mut filters = Vec::new();
        if !allow_low_signal && is_low_signal_source(&hit) {
            filters.push(decision(
                AskExplainFilterDecisionKind::DroppedLowSignal,
                "session/log/cache sources are excluded unless explicitly requested",
            ));
        }
        if !hybrid && hit.score < cfg.ask_min_relevance_score {
            filters.push(decision(
                AskExplainFilterDecisionKind::DroppedMinRelevance,
                "dense score was below ask_min_relevance_score",
            ));
        }
        if !topical_overlap(&url_tokens, &chunk_tokens, &tokens) {
            filters.push(decision(
                AskExplainFilterDecisionKind::DroppedTopicalOverlap,
                "candidate did not sufficiently overlap salient query tokens",
            ));
        }
        if filters.is_empty() {
            filters.push(AskExplainFilterDecision {
                kind: AskExplainFilterDecisionKind::Kept,
                reason: None,
            });
        }
        let retrieval_score = hit.score;
        candidates.push(RankedCandidate {
            hit,
            retrieval_rank: index + 1,
            rerank_rank: None,
            rerank_score,
            score_components: components(
                retrieval_score,
                url_boost,
                text_boost,
                docs_boost,
                authority_boost,
                product_boost,
                phrase_boost,
            ),
            filter_decisions: filters,
            configured_authoritative,
            product_authoritative,
            selected_context_rank: None,
            selection_reason: None,
        });
    }

    let mut ranked_indices = candidates
        .iter()
        .enumerate()
        .filter_map(|(i, c)| c.kept().then_some(i))
        .collect::<Vec<_>>();
    ranked_indices.sort_by(|a, b| {
        candidates[*b]
            .rerank_score
            .partial_cmp(&candidates[*a].rerank_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                candidates[*a]
                    .retrieval_rank
                    .cmp(&candidates[*b].retrieval_rank)
            })
    });
    for (rank, index) in ranked_indices.iter().copied().enumerate() {
        candidates[index].rerank_rank = Some(rank + 1);
    }
    let kept = ranked_indices
        .iter()
        .map(|i| &candidates[*i])
        .collect::<Vec<_>>();
    let configured_authority_ratio = ratio(&kept, |c| c.configured_authoritative);
    let product_authority_ratio = ratio(&kept, |c| c.product_authoritative);
    let top_domains = top_domains(&kept, 5);
    RankingResult {
        candidates,
        ranked_indices,
        query_tokens: tokens,
        keyword_query,
        complexity,
        effective_budget,
        configured_authority_ratio,
        product_authority_ratio,
        top_domains,
    }
}

fn components(
    retrieval: f64,
    url: f64,
    text: f64,
    docs: f64,
    authority: f64,
    product: f64,
    phrase: f64,
) -> Vec<AskExplainScoreComponent> {
    vec![
        component(
            "retrieval_score",
            retrieval,
            AskExplainScoreComponentStatus::Applied,
        ),
        component(
            "lexical_url_token_boost",
            url,
            AskExplainScoreComponentStatus::Applied,
        ),
        component(
            "lexical_chunk_token_boost",
            text,
            AskExplainScoreComponentStatus::Applied,
        ),
        component(
            "docs_path_boost",
            docs,
            AskExplainScoreComponentStatus::Applied,
        ),
        component(
            "authority_boost",
            authority,
            if authority > 0.0 {
                AskExplainScoreComponentStatus::Applied
            } else {
                AskExplainScoreComponentStatus::NotApplicable
            },
        ),
        component(
            "product_authority_boost",
            product,
            if product > 0.0 {
                AskExplainScoreComponentStatus::Applied
            } else {
                AskExplainScoreComponentStatus::NotApplicable
            },
        ),
        component(
            "phrase_match_boost",
            phrase,
            AskExplainScoreComponentStatus::Applied,
        ),
    ]
}
fn component(
    name: &str,
    value: f64,
    status: AskExplainScoreComponentStatus,
) -> AskExplainScoreComponent {
    AskExplainScoreComponent {
        name: name.to_string(),
        value,
        status,
        reason: None,
    }
}
fn decision(kind: AskExplainFilterDecisionKind, reason: &str) -> AskExplainFilterDecision {
    AskExplainFilterDecision {
        kind,
        reason: Some(reason.to_string()),
    }
}

fn classify_complexity(question: &str, tokens: &[String]) -> AskComplexity {
    let lower = question.to_ascii_lowercase();
    if [
        "list all",
        "show all",
        "every ",
        "everything",
        "enumerate",
        "comprehensive",
        "thorough",
        "in detail",
        "deep dive",
    ]
    .iter()
    .any(|n| lower.contains(n))
    {
        AskComplexity::Exhaustive
    } else if tokens.len() >= 5
        || lower.contains("how do i")
        || lower.contains("how should")
        || lower.contains("step by step")
        || lower.matches('?').count() > 1
    {
        AskComplexity::Complex
    } else {
        AskComplexity::Simple
    }
}

fn is_low_signal_source(hit: &QueryServiceHit) -> bool {
    let lower = hit.canonical_uri.to_ascii_lowercase();
    let source_key = hit.citation.source_item_key.to_ascii_lowercase();
    let web = lower.starts_with("http://") || lower.starts_with("https://");
    lower.starts_with("session://")
        || lower.starts_with("file://")
        || source_key.ends_with(".jsonl")
        || lower.contains("/docs/sessions/")
        || lower.contains("docs/sessions/")
        || lower.contains("/.cache/")
        || lower.contains(".cache/")
        || (!web && lower.contains("/logs/"))
        || (!web && lower.ends_with(".log"))
}

fn topical_overlap(url: &HashSet<String>, text: &HashSet<String>, tokens: &[String]) -> bool {
    let topical = tokens.iter().filter(|t| t.len() >= 3).collect::<Vec<_>>();
    if topical.is_empty() {
        return true;
    }
    let salient = topical
        .iter()
        .copied()
        .filter(|t| !is_generic_topical_token(t))
        .collect::<Vec<_>>();
    if !salient.is_empty()
        && !salient
            .iter()
            .any(|t| url.contains(t.as_str()) || text.contains(t.as_str()))
    {
        return false;
    }
    let overlap = topical
        .iter()
        .filter(|t| url.contains(t.as_str()) || text.contains(t.as_str()))
        .count();
    match topical.len() {
        1 | 2 => overlap >= 1,
        3 | 4 => overlap >= 1,
        _ => overlap >= 2,
    }
}

fn docs_path_boost(url: &str) -> f64 {
    let path = Url::parse(url)
        .ok()
        .map(|u| u.path().to_ascii_lowercase())
        .unwrap_or_else(|| url.to_ascii_lowercase());
    if path.contains("/docs/")
        || path.contains("/guides/")
        || path.contains("/api/")
        || path.contains("/reference/")
    {
        DOCS_PATH_BOOST
    } else {
        0.0
    }
}
fn normalized_domains(domains: &[String]) -> Vec<String> {
    domains
        .iter()
        .map(|d| d.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|d| !d.is_empty())
        .collect()
}
fn host_matches_domains(url: &str, domains: &[String]) -> bool {
    let Some(host) = Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
    else {
        return false;
    };
    domains.iter().any(|d| {
        host == *d
            || (host.len() > d.len()
                && host.ends_with(d)
                && host.as_bytes()[host.len() - d.len() - 1] == b'.')
    })
}
fn product_authority_match(url: &str, tokens: &[String]) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str().map(str::to_ascii_lowercase) else {
        return false;
    };
    if !is_docs_like_url(&host, url) {
        return false;
    }
    let mut identity = identity_tokens(&host);
    for segment in parsed
        .path_segments()
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .take(2)
    {
        identity.extend(identity_tokens(segment));
    }
    tokens
        .iter()
        .any(|t| !is_generic_authority_token(t) && identity.contains(t.as_str()))
}
fn is_docs_like_url(host: &str, url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    host == "docs.rs"
        || host.starts_with("docs.")
        || host.contains(".readthedocs.")
        || host.contains("developer")
        || [
            "/documentation/",
            "/docs/",
            "/guides/",
            "/guide/",
            "/api/",
            "/reference/",
            "/book/",
            "/learn/",
        ]
        .iter()
        .any(|p| lower.contains(p))
}
fn ascii_lowercase_contains(haystack: &str, needle: &str) -> bool {
    if needle.len() > haystack.len() {
        return false;
    }
    haystack.as_bytes().windows(needle.len()).any(|w| {
        w.iter()
            .zip(needle.as_bytes())
            .all(|(a, b)| a.to_ascii_lowercase() == *b)
    })
}
fn ratio<F>(candidates: &[&RankedCandidate], predicate: F) -> f64
where
    F: Fn(&RankedCandidate) -> bool,
{
    if candidates.is_empty() {
        0.0
    } else {
        candidates.iter().filter(|c| predicate(c)).count() as f64 / candidates.len() as f64
    }
}
fn top_domains(candidates: &[&RankedCandidate], limit: usize) -> Vec<String> {
    let mut counts = HashMap::<String, usize>::new();
    for candidate in candidates {
        if let Some(host) = Url::parse(&candidate.hit.canonical_uri)
            .ok()
            .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
        {
            *counts.entry(host).or_default() += 1;
        }
    }
    let mut domains = counts.into_iter().collect::<Vec<_>>();
    domains.sort_by(|(ah, ac), (bh, bc)| bc.cmp(ac).then_with(|| ah.cmp(bh)));
    domains
        .into_iter()
        .take(limit)
        .map(|(host, _)| host)
        .collect()
}
