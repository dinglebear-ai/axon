use super::*;
use crate::services::types::{
    AskExplainFilterDecisionKind, AskExplainScoreComponentStatus, AskExplainScoreKind,
};
use crate::vector::ops::qdrant::{QdrantPayload, QdrantSearchHit};
use serde_json::Value;
use std::collections::HashSet;

#[path = "product_authority_tests.rs"]
mod product_authority_tests;

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
fn candidate_has_topical_overlap_ignores_short_query_tokens_for_topical_gate() {
    let candidate = make_candidate(
        "https://docs.astral.sh/uv/concepts/projects/dependencies",
        "Python dependency management guide",
        0.4,
    )
    .candidate;
    let query_tokens = vec!["uv".to_string(), "dependencies".to_string()];

    assert!(candidate_has_topical_overlap(&candidate, &query_tokens));
}

#[test]
fn candidate_has_topical_overlap_rejects_generic_only_matches_when_product_named() {
    let candidate = make_candidate(
        "https://www.postgresql.org/docs/current/sql-createview.html",
        "CREATE VIEW examples and SQL reference content",
        0.4,
    )
    .candidate;
    let query_tokens = vec!["create".to_string(), "view".to_string(), "gpui".to_string()];

    assert!(!candidate_has_topical_overlap(&candidate, &query_tokens));
}

#[test]
fn candidate_has_topical_overlap_rejects_find_only_matches_when_crate_named() {
    let candidate = make_candidate(
        "https://www.postgresql.org/docs/current/spi-spi-cursor-find.html",
        "Find an SPI cursor in PostgreSQL reference documentation",
        0.4,
    )
    .candidate;
    let query_tokens = vec![
        "find".to_string(),
        "rust".to_string(),
        "crate".to_string(),
        "documentation".to_string(),
    ];

    assert!(!candidate_has_topical_overlap(&candidate, &query_tokens));
}

#[test]
fn candidate_has_topical_overlap_rejects_manage_only_matches_when_tool_named() {
    let candidate = make_candidate(
        "https://www.postgresql.org/docs/current/manage-ag-dropdb.html",
        "Destroying a database administration reference",
        0.4,
    )
    .candidate;
    let query_tokens = vec![
        "uv".to_string(),
        "manage".to_string(),
        "python".to_string(),
        "dependencies".to_string(),
    ];

    assert!(!candidate_has_topical_overlap(&candidate, &query_tokens));
}

#[test]
fn candidate_has_topical_overlap_rejects_topic_only_matches_when_language_named() {
    let candidate = make_candidate(
        "https://www.postgresql.org/docs/current/runtime-config-error-handling.html",
        "PostgreSQL runtime error handling reference",
        0.4,
    )
    .candidate;
    let query_tokens = vec![
        "structure".to_string(),
        "error".to_string(),
        "handling".to_string(),
        "rust".to_string(),
    ];

    assert!(!candidate_has_topical_overlap(&candidate, &query_tokens));
}

#[test]
fn score_policy_boosts_docs_like_url_with_query_product_token() {
    let candidates = vec![
        make_candidate(
            "https://docs.other.dev/cli/plugins",
            "plugins marketplace commands install list inspect unrelated docs",
            0.28,
        ),
        make_candidate(
            "https://docs.widget.dev/docs/en/plugins",
            "Widget plugins marketplace standalone configuration official docs",
            0.17,
        ),
    ];
    let query_tokens = vec![
        "widget".to_string(),
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

    assert_eq!(selected.len(), 1);
    assert_eq!(
        selected[0].candidate.url,
        "https://docs.widget.dev/docs/en/plugins"
    );
}

#[test]
fn score_trace_components_sum_to_final_rerank_score() {
    let candidates = vec![make_candidate(
        "https://docs.widget.dev/docs/en/plugins",
        "Widget plugins marketplace official docs",
        0.41,
    )];
    let query_tokens = vec![
        "widget".to_string(),
        "marketplace".to_string(),
        "plugins".to_string(),
    ];
    let policy = CandidateScorePolicy {
        authoritative_domains: &["docs.widget.dev".to_string()],
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
        "trace should make the docs-like product-token boost visible"
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
            "https://docs.other.dev/cli/plugins",
            "plugins marketplace commands install list inspect other docs",
            0.28,
        ),
        make_candidate(
            "https://docs.widget.dev/docs/en/plugins",
            "Widget plugins marketplace standalone configuration official docs",
            0.17,
        ),
    ];
    let query_tokens = vec![
        "widget".to_string(),
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
fn rrf_score_trace_applies_lexical_boosts_without_min_relevance() {
    let candidates = vec![
        make_candidate(
            "https://docs.widget.dev/docs/en/plugins",
            "Widget plugins marketplace official docs",
            0.02,
        ),
        make_candidate(
            "https://docs.other.dev/cli/plugins",
            "unrelated command reference",
            0.30,
        ),
    ];
    let query_tokens = vec!["widget".to_string(), "plugins".to_string()];
    let policy = CandidateScorePolicy {
        authoritative_domains: &[],
        authoritative_boost: 0.0,
        product_authority_boost: 0.35,
        min_relevance_score: None,
        require_topical_overlap: true,
    };

    let (selected, trace) = score_rrf_candidates_with_trace(&candidates, &query_tokens, &policy);

    assert_eq!(
        selected[0].candidate.url, "https://docs.widget.dev/docs/en/plugins",
        "RRF trace path should not apply the cosine min relevance threshold"
    );
    assert!(selected[0].candidate.rerank_score > 0.37);
    assert_eq!(trace[0].score_kind, AskExplainScoreKind::Rrf);
    assert!(
        trace[0].score_components.iter().any(|component| {
            component.name == "lexical_url_token_boost"
                && component.status == AskExplainScoreComponentStatus::Applied
                && component.value > 0.0
        }),
        "RRF trace path should apply lexical URL boosts during final ask context reranking"
    );
}
