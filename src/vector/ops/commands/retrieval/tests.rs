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
