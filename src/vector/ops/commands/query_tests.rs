use super::{chunk_index_for_candidate, query_score_policy};
use crate::core::config::Config;
use crate::vector::ops::commands::retrieval::RetrievedCandidate;
use crate::vector::ops::ranking;
use crate::vector::ops::tei::{QUERY_INSTRUCTION, prepend_query_instruction};

#[test]
fn query_instruction_is_nonempty_and_ends_with_query_colon() {
    assert!(!QUERY_INSTRUCTION.is_empty());
    assert!(
        QUERY_INSTRUCTION.ends_with("Query: "),
        "instruction must end with 'Query: ', got: {QUERY_INSTRUCTION:?}"
    );
}

#[test]
fn query_instruction_prepend_produces_correct_string() {
    // Tests the prepend_query_instruction() helper used in query_results().
    // Locks in: instruction is prepended, query text is preserved verbatim,
    // combined string is strictly longer than the query alone.
    let query = "how does markdown splitting work";
    let with_instruction = prepend_query_instruction(query);

    assert!(
        with_instruction.starts_with("Instruct:"),
        "combined string must start with the instruction prefix"
    );
    assert!(
        with_instruction.ends_with(query),
        "combined string must end with the original query text verbatim"
    );
    assert!(
        with_instruction.len() > query.len(),
        "combined string must be longer than the query alone"
    );
}

#[test]
fn chunk_index_for_candidate_returns_payload_index() {
    let selected = RetrievedCandidate {
        candidate: ranking::AskCandidate {
            score: 0.9,
            url: "https://example.com/a".to_string(),
            path: "/a".to_string(),
            chunk_text: "chunk body".to_string(),
            url_tokens: std::collections::HashSet::new(),
            chunk_tokens: std::collections::HashSet::new(),
            rerank_score: 0.9,
        },
        chunk_index: Some(42),
    };

    assert_eq!(chunk_index_for_candidate(&selected), serde_json::json!(42));
}

#[test]
fn absolute_rank_uses_offset_plus_one_based_index() {
    let offset = 20usize;
    let ranks = (0..3).map(|i| offset + i + 1).collect::<Vec<_>>();
    assert_eq!(ranks, vec![21, 22, 23]);
}

#[test]
fn query_score_policy_does_not_apply_ask_threshold() {
    let mut cfg = Config {
        ask_min_relevance_score: 0.45,
        ..Config::default()
    };
    cfg.ask_authoritative_boost = 0.25;

    let policy = query_score_policy(&cfg);

    assert_eq!(policy.min_relevance_score, None);
    assert!(policy.require_topical_overlap);
    assert_eq!(policy.authoritative_boost, 0.25);
    assert_eq!(policy.product_authority_boost, 0.35);
}
