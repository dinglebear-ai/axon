use super::*;
use crate::services::types::{
    AskExplainFilterDecisionKind, AskExplainScoreComponentStatus, AskExplainScoreKind,
};
use crate::vector::ops::qdrant::{QdrantPayload, QdrantSearchHit};
use serde_json::Value;
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

fn make_hit(url: &str, chunk: &str, score: f64) -> QdrantSearchHit {
    QdrantSearchHit {
        id: Value::Null,
        score,
        payload: QdrantPayload {
            url: url.to_string(),
            chunk_text: chunk.to_string(),
            text: String::new(),
            chunk_index: Some(7),
        },
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
fn build_candidates_trace_records_low_signal_drops() {
    let policy = CandidateBuildPolicy {
        allow_low_signal: false,
    };
    let built = build_candidates_from_hits_with_trace(
        vec![make_hit(
            "docs/sessions/2026-05-15-rag-debug.md",
            "low signal session content long enough to pass chunk length filtering",
            0.7,
        )],
        &policy,
        AskExplainScoreKind::Cosine,
    );

    assert!(built.candidates.is_empty());
    assert_eq!(built.filter_traces.len(), 1);
    assert_eq!(
        built.filter_traces[0].filter_decisions[0].kind,
        AskExplainFilterDecisionKind::DroppedLowSignal
    );
}

#[test]
fn merge_candidates_trace_records_duplicate_drops() {
    let merged = merge_candidates_with_trace(
        vec![make_candidate(
            "https://a.test/p",
            "duplicate chunk text long enough to compare by prefix",
            0.9,
        )],
        vec![make_candidate(
            "https://a.test/p",
            "duplicate chunk text long enough to compare by prefix",
            0.8,
        )],
        AskExplainScoreKind::Cosine,
    );

    assert_eq!(merged.candidates.len(), 1);
    assert_eq!(merged.filter_traces.len(), 1);
    assert_eq!(
        merged.filter_traces[0].filter_decisions[0].kind,
        AskExplainFilterDecisionKind::DroppedDuplicate
    );
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
        product_authority_boost: 0.0,
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
        product_authority_boost: 0.0,
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

#[test]
fn score_policy_boosts_official_product_docs_for_named_product_queries() {
    let candidates = vec![
        make_candidate(
            "https://docs.openclaw.ai/cli/plugins",
            "plugins marketplace commands install list inspect openclaw docs",
            0.28,
        ),
        make_candidate(
            "https://code.claude.com/docs/en/plugins",
            "Claude Code plugins marketplace standalone configuration official docs",
            0.17,
        ),
    ];
    let query_tokens = vec![
        "claude".to_string(),
        "marketplace".to_string(),
        "plugins".to_string(),
    ];
    let policy = CandidateScorePolicy {
        authoritative_domains: &[],
        authoritative_boost: 0.0,
        product_authority_boost: 0.35,
        min_relevance_score: None,
        require_topical_overlap: true,
    };

    let selected = score_and_filter_candidates(&candidates, &query_tokens, &policy);

    assert_eq!(
        selected[0].candidate.url,
        "https://code.claude.com/docs/en/plugins"
    );
    assert!(
        selected[0].candidate.rerank_score > selected[1].candidate.rerank_score,
        "official Claude docs should outrank cross-product plugin docs for Claude queries"
    );
}

#[test]
fn score_trace_components_sum_to_final_rerank_score() {
    let candidates = vec![make_candidate(
        "https://code.claude.com/docs/en/plugins",
        "Claude Code plugins marketplace official docs",
        0.41,
    )];
    let query_tokens = vec![
        "claude".to_string(),
        "marketplace".to_string(),
        "plugins".to_string(),
    ];
    let policy = CandidateScorePolicy {
        authoritative_domains: &["code.claude.com".to_string()],
        authoritative_boost: 0.12,
        product_authority_boost: 0.35,
        min_relevance_score: None,
        require_topical_overlap: true,
    };

    let (selected, trace) = score_and_filter_candidates_with_trace(
        &candidates,
        &query_tokens,
        &policy,
        AskExplainScoreKind::Cosine,
    );

    assert_eq!(selected.len(), 1);
    assert_eq!(trace.len(), 1);
    assert_eq!(trace[0].score_kind, AskExplainScoreKind::Cosine);
    assert_eq!(
        trace[0].filter_decisions[0].kind,
        AskExplainFilterDecisionKind::Kept
    );
    let component_sum = trace[0]
        .score_components
        .iter()
        .filter(|component| component.status == AskExplainScoreComponentStatus::Applied)
        .map(|component| component.value)
        .sum::<f64>();
    assert!((component_sum - selected[0].candidate.rerank_score).abs() < 1e-12);
    assert!(
        trace[0]
            .score_components
            .iter()
            .any(|component| component.name == "product_authority_boost"
                && component.value > 0.0
                && component.status == AskExplainScoreComponentStatus::Applied),
        "trace should make the Claude official-domain product boost visible"
    );
}

#[test]
fn score_trace_uses_supplied_dense_score_kind() {
    let candidates = vec![make_candidate(
        "https://docs.example.com/rust",
        "rust async runtime details long enough to keep",
        0.41,
    )];
    let query_tokens = vec!["rust".to_string()];
    let policy = CandidateScorePolicy {
        authoritative_domains: &[],
        authoritative_boost: 0.0,
        product_authority_boost: 0.0,
        min_relevance_score: None,
        require_topical_overlap: true,
    };

    let (_, trace) = score_and_filter_candidates_with_trace(
        &candidates,
        &query_tokens,
        &policy,
        AskExplainScoreKind::NamedDense,
    );

    assert_eq!(trace[0].score_kind, AskExplainScoreKind::NamedDense);
}

#[test]
fn score_trace_preserves_normal_rerank_output() {
    let candidates = vec![
        make_candidate(
            "https://docs.openclaw.ai/cli/plugins",
            "plugins marketplace commands install list inspect openclaw docs",
            0.28,
        ),
        make_candidate(
            "https://code.claude.com/docs/en/plugins",
            "Claude Code plugins marketplace standalone configuration official docs",
            0.17,
        ),
    ];
    let query_tokens = vec![
        "claude".to_string(),
        "marketplace".to_string(),
        "plugins".to_string(),
    ];
    let policy = CandidateScorePolicy {
        authoritative_domains: &[],
        authoritative_boost: 0.0,
        product_authority_boost: 0.35,
        min_relevance_score: None,
        require_topical_overlap: true,
    };

    let normal = score_and_filter_candidates(&candidates, &query_tokens, &policy);
    let (with_trace, trace) = score_and_filter_candidates_with_trace(
        &candidates,
        &query_tokens,
        &policy,
        AskExplainScoreKind::Cosine,
    );

    assert_eq!(normal.len(), with_trace.len());
    assert_eq!(trace.len(), candidates.len());
    for (normal, with_trace) in normal.iter().zip(with_trace.iter()) {
        assert_eq!(normal.candidate.url, with_trace.candidate.url);
        assert!((normal.candidate.rerank_score - with_trace.candidate.rerank_score).abs() < 1e-12);
    }
}

#[test]
fn rrf_score_trace_skips_additive_boosts_and_min_relevance() {
    let candidates = vec![
        make_candidate(
            "https://code.claude.com/docs/en/plugins",
            "Claude Code plugins marketplace official docs",
            0.02,
        ),
        make_candidate(
            "https://docs.openclaw.ai/cli/plugins",
            "unrelated command reference",
            0.99,
        ),
    ];
    let query_tokens = vec!["claude".to_string(), "plugins".to_string()];

    let (selected, trace) = score_rrf_candidates_with_trace(&candidates, &query_tokens);

    assert_eq!(
        selected[0].candidate.url, "https://code.claude.com/docs/en/plugins",
        "RRF trace path should not apply the cosine min relevance threshold"
    );
    assert_eq!(selected[0].candidate.rerank_score, 0.02);
    assert_eq!(trace[0].score_kind, AskExplainScoreKind::Rrf);
    assert!(
        trace[0]
            .score_components
            .iter()
            .filter(|component| component.name != "retrieval_score")
            .all(|component| component.status == AskExplainScoreComponentStatus::Skipped),
        "all additive rerank components must be marked skipped in RRF mode"
    );
}

#[test]
fn score_policy_boosts_openclaw_docs_for_openclaw_queries() {
    let candidates = vec![
        make_candidate(
            "https://code.claude.com/docs/en/plugins",
            "Claude Code plugins marketplace official docs",
            0.30,
        ),
        make_candidate(
            "https://docs.openclaw.ai/cli/plugins",
            "OpenClaw plugins marketplace commands install list inspect",
            0.20,
        ),
    ];
    let query_tokens = vec![
        "openclaw".to_string(),
        "marketplace".to_string(),
        "plugins".to_string(),
    ];
    let policy = CandidateScorePolicy {
        authoritative_domains: &[],
        authoritative_boost: 0.0,
        product_authority_boost: 0.35,
        min_relevance_score: None,
        require_topical_overlap: true,
    };

    let selected = score_and_filter_candidates(&candidates, &query_tokens, &policy);

    assert_eq!(
        selected[0].candidate.url,
        "https://docs.openclaw.ai/cli/plugins"
    );
}
