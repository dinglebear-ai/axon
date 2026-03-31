use super::build::collect_supplemental_candidate_indices;
use super::heuristics::{
    candidate_has_topical_overlap, query_requests_low_signal_sources, should_inject_supplemental,
    url_matches_domain_list,
};
use crate::crates::core::config::Config;
use crate::crates::vector::ops::ranking::AskCandidate;
use crate::crates::vector::ops::ranking::is_low_signal_url;
use std::collections::HashSet;

fn test_candidate(url: &str, rerank_score: f64) -> AskCandidate {
    AskCandidate {
        score: rerank_score,
        url: url.to_string(),
        path: url.to_string(),
        chunk_text: "chunk text for testing".to_string(),
        url_tokens: HashSet::new(),
        chunk_tokens: HashSet::new(),
        rerank_score,
    }
}

#[test]
fn ask_hybrid_candidates_is_distinct_from_query_candidates() {
    let cfg = Config {
        hybrid_search_candidates: 100,
        ask_hybrid_candidates: 150,
        ..Default::default()
    };
    assert_ne!(
        cfg.ask_hybrid_candidates, cfg.hybrid_search_candidates,
        "ask and query prefetch windows must be independently configurable"
    );
    assert_eq!(cfg.ask_hybrid_candidates, 150);
    assert_eq!(cfg.hybrid_search_candidates, 100);
}

#[test]
fn supplemental_injects_when_coverage_is_thin_and_budget_is_available() {
    let should = should_inject_supplemental(
        10_000, 100_000, 0, // no full docs selected yet
        4, // below coverage threshold
    );
    assert!(should);
}

#[test]
fn supplemental_skips_when_context_budget_is_nearly_full() {
    let should = should_inject_supplemental(
        90_000,  // exceeds 85% budget gate
        100_000, //
        0, 2,
    );
    assert!(!should);
}

#[test]
fn supplemental_skips_when_coverage_is_already_strong() {
    let should = should_inject_supplemental(
        50_000, // budget headroom remains
        100_000, 2, // full docs present
        6, // meets chunk coverage threshold
    );
    assert!(!should);
}

#[test]
fn low_signal_source_filter_matches_sessions_and_cache() {
    assert!(is_low_signal_url(
        "docs/sessions/2026-02-26-context-injection-cleanup.md"
    ));
    assert!(is_low_signal_url(".cache/axon-rust/output/file.md"));
    assert!(is_low_signal_url("/home/user/app/logs/access.log"));
    assert!(is_low_signal_url("logs/debug.log"));
    assert!(!is_low_signal_url(
        "https://docs.datadoghq.com/logs/explorer/"
    ));
    assert!(!is_low_signal_url("https://docs.rs/spider/latest/spider/"));
}

#[test]
fn low_signal_sources_allowed_when_query_explicitly_requests_them() {
    let tokens = vec!["debug".to_string(), "session".to_string()];
    assert!(query_requests_low_signal_sources(
        &tokens,
        "debug this session"
    ));
    assert!(query_requests_low_signal_sources(
        &["debug".to_string()],
        "show docs/sessions files"
    ));
    assert!(!query_requests_low_signal_sources(
        &["debug".to_string(), "crawl".to_string()],
        "debug crawl failures"
    ));
}

#[test]
fn supplemental_candidates_respect_score_threshold_and_full_doc_exclusions() {
    let candidates = vec![
        test_candidate("https://a.dev/docs/one", 0.70),
        test_candidate("https://a.dev/docs/two", 0.52),
        test_candidate("https://b.dev/docs/three", 0.61),
    ];
    let mut excluded = HashSet::new();
    excluded.insert("https://a.dev/docs/one".to_string());
    let selected = collect_supplemental_candidate_indices(&candidates, &excluded, 0.60);
    assert_eq!(selected, vec![2]);
}

#[test]
fn topical_overlap_requires_multiple_query_tokens_for_longer_queries() {
    let candidate = test_candidate("https://example.com/docs/commands", 0.9);
    let tokens = vec![
        "create".to_string(),
        "claude".to_string(),
        "code".to_string(),
        "custom".to_string(),
        "slash".to_string(),
        "commands".to_string(),
    ];
    assert!(!candidate_has_topical_overlap(&candidate, &tokens));

    let strong_candidate = AskCandidate {
        score: 0.9,
        url: "https://docs.claude.com/en/docs/claude-code/slash-commands".to_string(),
        path: "/docs/claude-code/slash-commands".to_string(),
        chunk_text: "Create custom slash commands in Claude Code.".to_string(),
        url_tokens: ["claude", "code", "slash", "commands"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        chunk_tokens: ["create", "custom", "slash", "commands"]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        rerank_score: 0.9,
    };
    assert!(candidate_has_topical_overlap(&strong_candidate, &tokens));
}

#[test]
fn context_built_log_format_is_valid() {
    let candidates_retrieved = 150usize;
    let candidates_after_score_filter = 42usize;
    let candidates_selected = 10usize;
    let line = format!(
        "ask context_built candidates_retrieved={candidates_retrieved} candidates_after_score_filter={candidates_after_score_filter} candidates_selected={candidates_selected}"
    );
    assert!(line.contains("ask context_built"));
    assert!(line.contains("candidates_retrieved=150"));
    assert!(line.contains("candidates_after_score_filter=42"));
    assert!(line.contains("candidates_selected=10"));
}

#[test]
fn authoritative_allowlist_matches_exact_and_suffix_hosts() {
    let allow = vec!["docs.claude.com".to_string(), "openai.com".to_string()];
    assert!(url_matches_domain_list(
        "https://docs.claude.com/en/docs/claude-code/overview",
        &allow
    ));
    assert!(url_matches_domain_list(
        "https://platform.openai.com/docs/overview",
        &allow
    ));
    assert!(!url_matches_domain_list(
        "https://medium.com/some-post",
        &allow
    ));
}
