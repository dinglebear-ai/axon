//! Explain-surface score components and filter decisions for ask ranking.

use axon_api::{
    AskExplainFilterDecision, AskExplainFilterDecisionKind, AskExplainScoreComponent,
    AskExplainScoreComponentStatus,
};

/// Every additive delta applied on top of a candidate's retrieval score.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct ScoreBoosts {
    pub(super) url: f64,
    pub(super) text: f64,
    pub(super) docs: f64,
    pub(super) authority: f64,
    pub(super) product: f64,
    pub(super) phrase: f64,
}

impl ScoreBoosts {
    /// Adds each boost to `base` left to right, matching the historical
    /// summation order so rerank ties resolve identically.
    pub(super) fn applied_to(self, base: f64) -> f64 {
        base + self.url + self.text + self.docs + self.authority + self.product + self.phrase
    }
}

pub(super) fn components(
    retrieval: f64,
    boosts: ScoreBoosts,
    hybrid: bool,
) -> Vec<AskExplainScoreComponent> {
    let lexical_component = |name: &str, value: f64| {
        if hybrid {
            AskExplainScoreComponent {
                name: name.to_string(),
                value: 0.0,
                status: AskExplainScoreComponentStatus::Skipped,
                reason: Some(
                    "RRF preserves fused retrieval ordering; additive lexical rerank boosts are skipped"
                        .to_string(),
                ),
            }
        } else {
            component(name, value, AskExplainScoreComponentStatus::Applied)
        }
    };
    vec![
        component(
            "retrieval_score",
            retrieval,
            AskExplainScoreComponentStatus::Applied,
        ),
        lexical_component("lexical_url_token_boost", boosts.url),
        lexical_component("lexical_chunk_token_boost", boosts.text),
        lexical_component("docs_path_boost", boosts.docs),
        trust_component("authority_boost", boosts.authority),
        trust_component("product_authority_boost", boosts.product),
        lexical_component("phrase_match_boost", boosts.phrase),
    ]
}

fn trust_component(name: &str, value: f64) -> AskExplainScoreComponent {
    let status = if value > 0.0 {
        AskExplainScoreComponentStatus::Applied
    } else {
        AskExplainScoreComponentStatus::NotApplicable
    };
    component(name, value, status)
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

pub(super) fn decision(
    kind: AskExplainFilterDecisionKind,
    reason: &str,
) -> AskExplainFilterDecision {
    AskExplainFilterDecision {
        kind,
        reason: Some(reason.to_string()),
    }
}
