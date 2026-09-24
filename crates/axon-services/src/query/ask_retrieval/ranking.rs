//! Ask-specific post-retrieval ranking and adaptive context policy.

mod components;
mod signals;

use super::super::query_tokens::{
    query_tokens, query_wants_low_signal_sources, tokenize_path_set, tokenize_text_set,
};
use axon_api::{AskExplainFilterDecision, AskExplainFilterDecisionKind, AskExplainScoreComponent};
use axon_core::config::Config;
use axon_core::llm::{SynthesisModelProfile, SynthesisModelTier};
use axon_retrieval::QueryServiceHit;
use components::{ScoreBoosts, components, decision};
use reqwest::Url;
use signals::{
    ascii_lowercase_contains, authority_deltas, classify_complexity, docs_path_boost,
    host_matches_domains, is_low_signal_source, matches_named_product_identity, normalized_domains,
    product_authority_match,
};
use std::collections::{HashMap, HashSet};

const URL_TOKEN_BOOST: f64 = 0.045;
const TEXT_TOKEN_BOOST: f64 = 0.015;
const LEXICAL_BOOST_CAP: f64 = 0.30;
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
            chunk_limit: cfg
                .ask_chunk_limit
                .min(cfg.ask_candidate_limit)
                .min(chunks)
                .max(1),
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
    pub(crate) keyword_query: String,
    pub(crate) complexity: AskComplexity,
    pub(crate) effective_budget: EffectiveAskBudget,
    pub(crate) authority_ratio: f64,
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
    let signals = RankingSignals {
        tokens: &tokens,
        phrase: &keyword_query,
        allow_low_signal: query_wants_low_signal_sources(&tokens, question),
        domains: normalized_domains(&cfg.ask_authoritative_domains),
        configured_boost: cfg.ask_authoritative_boost.clamp(0.0, 0.5),
        min_relevance: cfg.ask_min_relevance_score,
        hybrid,
    };
    let mut candidates = hits
        .into_iter()
        .take(cfg.ask_candidate_limit.max(1))
        .enumerate()
        .map(|(index, hit)| score_candidate(&signals, index, hit))
        .collect::<Vec<_>>();
    let ranked_indices = order_kept_candidates(&mut candidates);
    let kept = ranked_indices
        .iter()
        .map(|i| &candidates[*i])
        .collect::<Vec<_>>();
    let authority_ratio = ratio(&kept, |c| {
        c.configured_authoritative || c.product_authoritative
    });
    let configured_authority_ratio = ratio(&kept, |c| c.configured_authoritative);
    let product_authority_ratio = ratio(&kept, |c| c.product_authoritative);
    let top_domains = top_domains(&kept, 5);
    RankingResult {
        candidates,
        ranked_indices,
        keyword_query,
        complexity,
        effective_budget,
        authority_ratio,
        configured_authority_ratio,
        product_authority_ratio,
        top_domains,
    }
}

/// Query-derived inputs shared by every candidate in one ranking pass.
struct RankingSignals<'a> {
    tokens: &'a [String],
    phrase: &'a str,
    allow_low_signal: bool,
    domains: Vec<String>,
    configured_boost: f64,
    min_relevance: f64,
    hybrid: bool,
}

fn score_candidate(
    signals: &RankingSignals<'_>,
    index: usize,
    hit: QueryServiceHit,
) -> RankedCandidate {
    let url_tokens = tokenize_path_set(&hit.canonical_uri);
    let chunk_tokens = tokenize_text_set(&hit.text);
    // Dense/cosine scores can safely absorb lexical relevance deltas.
    // RRF scores are rank-fusion values on a different scale, so preserve
    // Qdrant's fused ordering and only apply explicit trust/authority boosts.
    let (url, text) = if signals.hybrid {
        (0.0, 0.0)
    } else {
        lexical_boosts(signals.tokens, &url_tokens, &chunk_tokens)
    };
    let docs = if signals.hybrid {
        0.0
    } else {
        docs_path_boost(&hit.canonical_uri)
    };
    let configured_authoritative = host_matches_domains(&hit.canonical_uri, &signals.domains);
    let product_authoritative = product_authority_match(&hit.canonical_uri, signals.tokens);
    let (authority, product) = authority_deltas(
        hit.score,
        if configured_authoritative {
            signals.configured_boost
        } else {
            0.0
        },
        if product_authoritative {
            PRODUCT_AUTHORITY_BOOST
        } else {
            0.0
        },
        signals.hybrid,
    );
    let phrase = if !signals.hybrid
        && signals.phrase.len() >= 6
        && signals.tokens.len() >= 2
        && ascii_lowercase_contains(&hit.text, signals.phrase)
    {
        PHRASE_MATCH_BOOST
    } else {
        0.0
    };
    let boosts = ScoreBoosts {
        url,
        text,
        docs,
        authority,
        product,
        phrase,
    };
    let filter_decisions = filter_decisions(signals, &hit, &url_tokens, &chunk_tokens);
    let retrieval_score = hit.score;
    RankedCandidate {
        hit,
        retrieval_rank: index + 1,
        rerank_rank: None,
        rerank_score: boosts.applied_to(retrieval_score),
        score_components: components(retrieval_score, boosts, signals.hybrid),
        filter_decisions,
        configured_authoritative,
        product_authoritative,
        selected_context_rank: None,
        selection_reason: None,
    }
}

/// URL and chunk token boosts, scaled down together when they exceed the cap.
fn lexical_boosts(
    tokens: &[String],
    url_tokens: &HashSet<String>,
    chunk_tokens: &HashSet<String>,
) -> (f64, f64) {
    let mut url = 0.0;
    let mut text = 0.0;
    for token in tokens {
        if url_tokens.contains(token) {
            url += URL_TOKEN_BOOST;
        }
        if chunk_tokens.contains(token) {
            text += TEXT_TOKEN_BOOST;
        }
    }
    let lexical = url + text;
    if lexical > LEXICAL_BOOST_CAP {
        let scale = LEXICAL_BOOST_CAP / lexical;
        url *= scale;
        text *= scale;
    }
    (url, text)
}

fn filter_decisions(
    signals: &RankingSignals<'_>,
    hit: &QueryServiceHit,
    url_tokens: &HashSet<String>,
    chunk_tokens: &HashSet<String>,
) -> Vec<AskExplainFilterDecision> {
    let mut filters = Vec::new();
    if !signals.allow_low_signal && is_low_signal_source(hit) {
        filters.push(decision(
            AskExplainFilterDecisionKind::DroppedLowSignal,
            "session/log/cache sources are excluded unless explicitly requested",
        ));
    }
    if !signals.hybrid && hit.score < signals.min_relevance {
        filters.push(decision(
            AskExplainFilterDecisionKind::DroppedMinRelevance,
            "dense score was below ask_min_relevance_score",
        ));
    }
    if !matches_named_product_identity(url_tokens, chunk_tokens, signals.tokens) {
        filters.push(decision(
            AskExplainFilterDecisionKind::DroppedProductIdentityMismatch,
            "candidate did not match an explicit named-product identity in the query",
        ));
    }
    if filters.is_empty() {
        filters.push(AskExplainFilterDecision {
            kind: AskExplainFilterDecisionKind::Kept,
            reason: None,
        });
    }
    filters
}

/// Orders kept candidates by rerank score (retrieval rank breaks ties) and
/// stamps each with its 1-based rerank rank.
fn order_kept_candidates(candidates: &mut [RankedCandidate]) -> Vec<usize> {
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
    ranked_indices
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
