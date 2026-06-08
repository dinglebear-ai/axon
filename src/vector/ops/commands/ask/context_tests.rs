use super::build::{
    SelectionPolicy, collect_supplemental_candidate_indices, planned_full_doc_urls,
    select_context_indices,
};
use super::heuristics::{
    candidate_has_topical_overlap, query_requests_low_signal_sources, should_inject_supplemental,
    url_matches_domain_list,
};
use super::query_rewrite::{QueryComplexity, build_query_forms};
use super::retrieval::{RerankParams, apply_mode_aware_rerank, is_rrf_mode};
use super::{FullDocsSource, resolve_ask_full_docs};
use crate::core::config::Config;
use crate::services::types::{
    AskExplainContext, AskExplainFilterDecision, AskExplainFilterDecisionKind,
    AskExplainInsertionMode, AskExplainRetrieval, AskExplainScoreKind, AskExplainSelectionDecision,
    AskExplainSelectionDecisionKind, CorpusHealthKind,
};
use crate::vector::ops::commands::retrieval::{CandidateRankingTrace, RetrievedCandidate};
use crate::vector::ops::ranking::AskCandidate;
use crate::vector::ops::ranking::is_low_signal_url;
use crate::vector::ops::tei::qdrant_store::VectorMode;
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

fn retrieved_candidate(url: &str, chunk: &str, score: f64) -> RetrievedCandidate {
    RetrievedCandidate {
        candidate: AskCandidate {
            score,
            url: url.to_string(),
            path: url.to_string(),
            chunk_text: chunk.to_string(),
            url_tokens: crate::vector::ops::ranking::tokenize_path_set(url),
            chunk_tokens: crate::vector::ops::ranking::tokenize_text_set(chunk),
            rerank_score: 0.0,
        },
        chunk_index: Some(7),
    }
}

fn rerank_params<'a>(domains: &'a [String]) -> RerankParams<'a> {
    RerankParams {
        authoritative_domains: domains,
        authoritative_boost: 0.5,
        product_authority_boost: 0.35,
        min_relevance_score: 0.45,
    }
}

fn explain_retrieval() -> AskExplainRetrieval {
    AskExplainRetrieval {
        query: "rust docs".to_string(),
        keyword_query: "rust docs".to_string(),
        dual_search: false,
        collection: "cortex".to_string(),
        candidate_limit: 150,
        hybrid_search_enabled: true,
        hybrid_candidate_limit: 100,
        score_kind: AskExplainScoreKind::Cosine,
        vector_mode: "unnamed".to_string(),
        sparse_query_status: None,
    }
}

fn explain_context() -> AskExplainContext {
    AskExplainContext {
        planned_full_doc_urls: Vec::new(),
        full_doc_fetch_skipped: false,
        full_doc_fetch_skip_reason: "disabled".to_string(),
        full_doc_fetch_mode: "cosine".to_string(),
        final_source_order: Vec::new(),
        context_char_budget: 1000,
        context_chars_used: 100,
        rendered_context: "Sources:\n".to_string(),
        truncated_by_budget: false,
    }
}

fn trace_candidate(url: &str, chunk_index: i64, score: f64, rerank_score: f64) -> AskCandidate {
    AskCandidate {
        score,
        url: url.to_string(),
        path: url.to_string(),
        chunk_text: format!("rust docs candidate chunk {chunk_index} long enough"),
        url_tokens: HashSet::new(),
        chunk_tokens: HashSet::new(),
        rerank_score,
    }
}

fn kept_trace(url: &str, chunk_index: i64) -> CandidateRankingTrace {
    CandidateRankingTrace {
        candidate: RetrievedCandidate {
            candidate: trace_candidate(url, chunk_index, 0.7, 0.8),
            chunk_index: Some(chunk_index),
        },
        score_kind: AskExplainScoreKind::Cosine,
        score_components: Vec::new(),
        filter_decisions: vec![AskExplainFilterDecision {
            kind: AskExplainFilterDecisionKind::Kept,
            reason: None,
        }],
    }
}

fn reranked_from_traces(traces: &[CandidateRankingTrace]) -> Vec<AskCandidate> {
    traces
        .iter()
        .map(|trace| trace.candidate.candidate.clone())
        .collect()
}

fn selection(
    candidate_index: usize,
    url: &str,
    kind: AskExplainSelectionDecisionKind,
) -> super::build::ContextCandidateSelection {
    selection_for_chunk(candidate_index, url, candidate_index as i64, kind)
}

fn selection_for_chunk(
    candidate_index: usize,
    url: &str,
    chunk_index: i64,
    kind: AskExplainSelectionDecisionKind,
) -> super::build::ContextCandidateSelection {
    super::build::ContextCandidateSelection {
        candidate_index,
        key: super::build::candidate_selection_key(&trace_candidate(url, chunk_index, 0.0, 0.0)),
        url: url.to_string(),
        decisions: vec![AskExplainSelectionDecision { kind, reason: None }],
        metadata: super::build::CandidateSelectionMetadata {
            planned_full_doc_rank: None,
            selected_context_rank: None,
            insertion_mode: None::<AskExplainInsertionMode>,
        },
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
fn ask_explain_trace_caps_candidate_output() {
    let traces = (0..51)
        .map(|idx| kept_trace(&format!("https://docs.example.com/{idx}"), idx))
        .collect::<Vec<_>>();
    let reranked = reranked_from_traces(&traces);
    let selections = (0..51)
        .map(|idx| {
            selection(
                idx as usize,
                &format!("https://docs.example.com/{idx}"),
                AskExplainSelectionDecisionKind::SelectedTopChunk,
            )
        })
        .collect::<Vec<_>>();

    let trace = super::build_explain_trace(
        "rust docs",
        &reranked,
        Some(explain_retrieval()),
        traces,
        explain_context(),
        selections,
    )
    .expect("trace");

    assert_eq!(trace.candidate_trace_limit, 50);
    assert_eq!(trace.candidates.len(), 50);
    assert!(trace.candidate_trace_truncated);
}

#[test]
fn ask_explain_trace_maps_duplicate_url_selections_by_kept_order() {
    let url = "https://docs.example.com/rust";
    let traces = vec![kept_trace(url, 1), kept_trace(url, 2)];
    let reranked = reranked_from_traces(&traces);
    let trace = super::build_explain_trace(
        "rust docs",
        &reranked,
        Some(explain_retrieval()),
        traces,
        explain_context(),
        vec![
            selection_for_chunk(0, url, 1, AskExplainSelectionDecisionKind::SelectedTopChunk),
            selection_for_chunk(
                1,
                url,
                2,
                AskExplainSelectionDecisionKind::SelectedSupplemental,
            ),
        ],
    )
    .expect("trace");

    assert_eq!(
        trace.candidates[0].selection_decisions[0].kind,
        AskExplainSelectionDecisionKind::SelectedTopChunk
    );
    assert_eq!(
        trace.candidates[1].selection_decisions[0].kind,
        AskExplainSelectionDecisionKind::SelectedSupplemental
    );
}

#[test]
fn ask_explain_trace_maps_reordered_kept_traces_by_candidate_key() {
    let first_url = "https://docs.example.com/first";
    let boosted_url = "https://docs.example.com/boosted";
    let traces = vec![kept_trace(first_url, 1), kept_trace(boosted_url, 2)];
    let reranked = vec![
        traces[1].candidate.candidate.clone(),
        traces[0].candidate.candidate.clone(),
    ];
    let trace = super::build_explain_trace(
        "rust docs",
        &reranked,
        Some(explain_retrieval()),
        traces,
        explain_context(),
        vec![
            selection_for_chunk(
                0,
                boosted_url,
                2,
                AskExplainSelectionDecisionKind::SelectedTopChunk,
            ),
            selection_for_chunk(
                1,
                first_url,
                1,
                AskExplainSelectionDecisionKind::SelectedSupplemental,
            ),
        ],
    )
    .expect("trace");

    assert_eq!(
        trace.candidates[0].selection_decisions[0].kind,
        AskExplainSelectionDecisionKind::SelectedSupplemental
    );
    assert_eq!(trace.candidates[0].raw_rerank_rank, Some(2));
    assert_eq!(
        trace.candidates[1].selection_decisions[0].kind,
        AskExplainSelectionDecisionKind::SelectedTopChunk
    );
    assert_eq!(trace.candidates[1].raw_rerank_rank, Some(1));
}

#[test]
fn corpus_health_classifies_no_candidates() {
    let health = super::classify_corpus_health(&[], &[], 0, 0);
    assert_eq!(health.kind, CorpusHealthKind::NoRetrievalCandidates);
    assert_eq!(health.selected_domain_count, 0);
    assert_eq!(health.top_domain_count, 0);
}

#[test]
fn corpus_health_classifies_retrieved_not_selected() {
    let health = super::classify_corpus_health(&["docs.example.com:3".to_string()], &[], 3, 0);
    assert_eq!(health.kind, CorpusHealthKind::RetrievedNotSelected);
}

#[test]
fn corpus_health_counts_unique_selected_domains_and_ignores_bad_urls() {
    let selected = vec![
        "https://docs.example.com/a".to_string(),
        "https://docs.example.com/b".to_string(),
        "not a url".to_string(),
        "https://api.example.com/c".to_string(),
    ];
    let health = super::classify_corpus_health(
        &[
            "docs.example.com:2".to_string(),
            "api.example.com:1".to_string(),
        ],
        &selected,
        4,
        3_000,
    );
    assert_eq!(health.kind, CorpusHealthKind::Healthy);
    assert_eq!(health.selected_domain_count, 2);
    assert_eq!(health.top_domain_count, 2);
}

#[test]
fn corpus_health_classifies_thin_and_unknown_context() {
    let thin = super::classify_corpus_health(
        &["docs.example.com:1".to_string()],
        &["https://docs.example.com/a".to_string()],
        1,
        100,
    );
    assert_eq!(thin.kind, CorpusHealthKind::ThinDomain);

    let unknown =
        super::classify_corpus_health(&[], &["https://docs.example.com/a".to_string()], 1, 3_000);
    assert_eq!(unknown.kind, CorpusHealthKind::Unknown);
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
    let selected = collect_supplemental_candidate_indices(&candidates, &excluded, Some(0.60));
    assert_eq!(selected, vec![2]);
}

#[test]
fn supplemental_candidates_can_skip_score_threshold_on_rrf_path() {
    let candidates = vec![
        test_candidate("https://a.dev/docs/one", 0.03),
        test_candidate("https://b.dev/docs/two", 0.02),
    ];
    let excluded = HashSet::new();
    let selected = collect_supplemental_candidate_indices(&candidates, &excluded, None);
    assert_eq!(selected, vec![0, 1]);
}

#[test]
fn context_full_doc_selection_is_independent_of_chunk_urls() {
    // When all top-ranked URLs fill chunk slots, full_doc_indices must still
    // return the top N sources — not an empty list.
    // The old URL-exclusion produced top_full_doc_indices=[] for narrow-domain
    // queries (observable as context_build_ms ≈ 5ms).
    let candidates = vec![
        test_candidate("https://a.dev/docs/one", 0.90),
        test_candidate("https://a.dev/docs/two", 0.80),
        test_candidate("https://b.dev/docs/three", 0.70),
        test_candidate("https://c.dev/docs/four", 0.60),
    ];

    let query_tokens = Vec::new();
    let (chunk_indices, full_doc_indices) =
        select_context_indices(&candidates, &query_tokens, 2, 2, SelectionPolicy::default());
    assert_eq!(chunk_indices.len(), 2, "should select 2 top chunks");
    assert_eq!(full_doc_indices.len(), 2, "should select 2 full docs");

    // Both sets pick the two highest-scoring unique URLs (intentional overlap).
    // append_top_chunks_to_context will skip snippets for planned_full_doc_urls.
    let chunk_urls: HashSet<&str> = chunk_indices
        .iter()
        .map(|&i| candidates[i].url.as_str())
        .collect();
    let full_doc_urls: HashSet<&str> = full_doc_indices
        .iter()
        .map(|&i| candidates[i].url.as_str())
        .collect();
    assert_eq!(
        chunk_urls, full_doc_urls,
        "both sets should pick the two highest-scoring URLs"
    );
}

#[test]
fn full_doc_selection_prefers_url_entity_matches() {
    let candidates = vec![
        retrieved_candidate(
            "https://code.claude.com/docs/en/debug-your-config",
            "subagents mcp servers settings skills",
            1.00,
        )
        .candidate,
        retrieved_candidate(
            "https://code.claude.com/docs/en/desktop",
            "mcp servers skills claude code desktop",
            0.96,
        )
        .candidate,
        retrieved_candidate(
            "https://code.claude.com/docs/en/sub-agents",
            "tools disallowedTools skills mcpServers",
            0.72,
        )
        .candidate,
    ];
    let query_tokens =
        crate::vector::ops::ranking::tokenize_query("setup agents not inherit mcp servers skills");

    let (_, full_doc_indices) =
        select_context_indices(&candidates, &query_tokens, 2, 2, SelectionPolicy::default());
    let full_doc_urls = full_doc_indices
        .iter()
        .map(|&idx| candidates[idx].url.as_str())
        .collect::<Vec<_>>();

    assert!(
        full_doc_urls.contains(&"https://code.claude.com/docs/en/sub-agents"),
        "canonical URL entity matches should get full-doc treatment: {full_doc_urls:?}"
    );
}

#[test]
fn full_doc_selection_prefers_dominant_authoritative_host_over_external_summary() {
    let candidates = vec![
        retrieved_candidate(
            "https://example.com/qdrant-hybrid-search-overview",
            "qdrant hybrid queries overview and vector search summary",
            1.20,
        )
        .candidate,
        retrieved_candidate(
            "https://qdrant.tech/documentation/search/hybrid-queries/",
            "hybrid queries sparse dense vectors fusion",
            0.90,
        )
        .candidate,
        retrieved_candidate(
            "https://qdrant.tech/documentation/search/filtering/",
            "filtering hybrid search payload conditions",
            0.88,
        )
        .candidate,
        retrieved_candidate(
            "https://qdrant.tech/documentation/concepts/vectors/",
            "dense sparse vector concepts hybrid",
            0.86,
        )
        .candidate,
        retrieved_candidate(
            "https://qdrant.tech/documentation/search/",
            "search query api hybrid qdrant",
            0.84,
        )
        .candidate,
        retrieved_candidate(
            "https://qdrant.tech/documentation/concepts/indexing/",
            "indexing qdrant vector search",
            0.82,
        )
        .candidate,
    ];
    let query_tokens = crate::vector::ops::ranking::tokenize_query("qdrant hybrid queries");

    let (_, full_doc_indices) =
        select_context_indices(&candidates, &query_tokens, 3, 3, SelectionPolicy::default());
    let full_doc_urls = full_doc_indices
        .iter()
        .map(|&idx| candidates[idx].url.as_str())
        .collect::<Vec<_>>();

    assert!(
        full_doc_urls
            .iter()
            .all(|url| url.starts_with("https://qdrant.tech/")),
        "dominant official host should fill scarce full-doc slots before external summaries: {full_doc_urls:?}"
    );
}

#[test]
fn selection_limits_full_docs_per_domain_when_alternatives_exist() {
    let candidates = vec![
        test_candidate("https://a.dev/docs/one", 0.99),
        test_candidate("https://a.dev/docs/two", 0.98),
        test_candidate("https://a.dev/docs/three", 0.97),
        test_candidate("https://b.dev/docs/one", 0.90),
        test_candidate("https://b.dev/docs/two", 0.89),
    ];
    let policy = SelectionPolicy {
        max_docs_per_domain: 2,
        ..SelectionPolicy::default()
    };

    let (_, full_doc_indices) = select_context_indices(&candidates, &[], 2, 4, policy);
    let full_doc_urls = full_doc_indices
        .iter()
        .map(|&idx| candidates[idx].url.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        full_doc_urls
            .iter()
            .filter(|url| url.contains("https://a.dev/"))
            .count(),
        2
    );
    assert!(
        full_doc_urls
            .iter()
            .any(|url| url.contains("https://b.dev/")),
        "selection should admit alternate domains after the per-domain cap: {full_doc_urls:?}"
    );
}

#[test]
fn selection_deduplicates_full_doc_urls() {
    let candidates = vec![
        test_candidate("https://a.dev/docs/one", 0.99),
        test_candidate("https://a.dev/docs/one", 0.98),
        test_candidate("https://a.dev/docs/one", 0.97),
        test_candidate("https://b.dev/docs/two", 0.90),
    ];

    let (_, full_doc_indices) =
        select_context_indices(&candidates, &[], 2, 3, SelectionPolicy::default());
    let full_doc_urls = full_doc_indices
        .iter()
        .map(|&idx| candidates[idx].url.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        full_doc_urls,
        vec!["https://a.dev/docs/one", "https://b.dev/docs/two"]
    );
}

#[test]
fn selection_fills_from_single_domain_when_no_alternatives_exist() {
    let candidates = vec![
        test_candidate("https://a.dev/docs/one", 0.99),
        test_candidate("https://a.dev/docs/two", 0.98),
        test_candidate("https://a.dev/docs/three", 0.97),
        test_candidate("https://a.dev/docs/four", 0.96),
        test_candidate("https://a.dev/docs/five", 0.95),
    ];

    let (_, full_doc_indices) =
        select_context_indices(&candidates, &[], 2, 5, SelectionPolicy::default());

    assert_eq!(full_doc_indices.len(), 5);
}

#[test]
fn selection_preserves_non_authoritative_high_signal_example() {
    let candidates = vec![
        test_candidate("https://blog.example.com/deep-topic", 0.99),
        test_candidate("https://docs.example.com/reference", 0.70),
        test_candidate("https://docs.example.com/reference-two", 0.69),
    ];
    let policy = SelectionPolicy {
        prefer_authoritative: false,
        ..SelectionPolicy::default()
    };

    let (chunk_indices, full_doc_indices) = select_context_indices(&candidates, &[], 1, 1, policy);

    assert_eq!(
        candidates[chunk_indices[0]].url,
        "https://blog.example.com/deep-topic"
    );
    assert_eq!(
        candidates[full_doc_indices[0]].url,
        "https://blog.example.com/deep-topic"
    );
}

#[test]
fn selection_without_authority_signal_preserves_existing_diversity() {
    let candidates = vec![
        test_candidate("https://a.dev/docs/one", 0.99),
        test_candidate("https://b.dev/docs/two", 0.98),
        test_candidate("https://c.dev/docs/three", 0.97),
    ];

    let (chunk_indices, full_doc_indices) =
        select_context_indices(&candidates, &[], 2, 2, SelectionPolicy::default());

    assert_eq!(chunk_indices, vec![0, 1]);
    assert_eq!(full_doc_indices, vec![0, 1]);
}

#[test]
fn planned_full_doc_urls_are_empty_when_fetch_is_skipped() {
    let candidates = vec![
        test_candidate("https://a.dev/docs/one", 0.90),
        test_candidate("https://b.dev/docs/two", 0.80),
    ];
    let query_tokens = Vec::new();
    let (_, full_doc_indices) =
        select_context_indices(&candidates, &query_tokens, 2, 2, SelectionPolicy::default());

    let skipped = planned_full_doc_urls(&candidates, &full_doc_indices, true);
    let fetched = planned_full_doc_urls(&candidates, &full_doc_indices, false);

    assert!(
        skipped.is_empty(),
        "top chunks must not be suppressed for planned full-doc URLs when full-doc fetch is skipped"
    );
    assert_eq!(fetched.len(), 2);
}

#[test]
fn apply_mode_aware_rerank_cosine_drops_below_threshold() {
    let candidates = vec![retrieved_candidate(
        "https://example.com/reference",
        "rust async runtime details long enough to keep",
        0.20,
    )];
    let selected = apply_mode_aware_rerank(
        false,
        &candidates,
        &["rust".to_string()],
        &rerank_params(&[]),
    );
    assert!(selected.is_empty());
}

#[test]
fn is_rrf_mode_requires_named_hybrid_and_non_empty_sparse() {
    assert!(is_rrf_mode(VectorMode::Named, true, false));
    assert!(!is_rrf_mode(VectorMode::Named, true, true));
    assert!(!is_rrf_mode(VectorMode::Named, false, false));
    assert!(!is_rrf_mode(VectorMode::Unnamed, true, false));
}

#[test]
fn apply_mode_aware_rerank_rrf_keeps_low_scores() {
    let candidates = vec![retrieved_candidate(
        "https://docs.example.com/rust",
        "rust async runtime details long enough to keep",
        0.03,
    )];
    let selected = apply_mode_aware_rerank(
        true,
        &candidates,
        &["rust".to_string()],
        &rerank_params(&[]),
    );
    assert_eq!(selected.len(), 1);
    assert!(selected[0].candidate.rerank_score > 0.03);
}

#[test]
fn apply_mode_aware_rerank_rrf_boosts_docs_like_product_token_url() {
    let candidates = vec![
        retrieved_candidate(
            "https://docs.other.dev/cli/plugins",
            "Widget marketplace plugins command reference long enough to keep",
            0.70,
        ),
        retrieved_candidate(
            "https://docs.widget.dev/docs/en/discover-plugins",
            "Widget marketplace plugins official documentation long enough to keep",
            0.50,
        ),
    ];
    let selected = apply_mode_aware_rerank(
        true,
        &candidates,
        &[
            "widget".to_string(),
            "marketplace".to_string(),
            "plugins".to_string(),
        ],
        &rerank_params(&[]),
    );

    assert_eq!(
        selected[0].candidate.url,
        "https://docs.widget.dev/docs/en/discover-plugins"
    );
    assert!(selected[0].candidate.rerank_score > selected[1].candidate.rerank_score);
}

#[test]
fn apply_mode_aware_rerank_rrf_filters_off_topic() {
    let candidates = vec![retrieved_candidate(
        "https://docs.example.com/python",
        "python decorators reference long enough to keep",
        0.03,
    )];
    let selected = apply_mode_aware_rerank(
        true,
        &candidates,
        &["rust".to_string()],
        &rerank_params(&[]),
    );
    assert!(selected.is_empty());
}

#[test]
fn apply_mode_aware_rerank_cosine_applies_authority_boost() {
    let domains = vec!["docs.example.com".to_string()];
    let candidates = vec![retrieved_candidate(
        "https://docs.example.com/rust",
        "rust async runtime details long enough to keep",
        0.10,
    )];
    let selected = apply_mode_aware_rerank(
        false,
        &candidates,
        &["rust".to_string()],
        &rerank_params(&domains),
    );
    assert_eq!(selected.len(), 1);
    assert!(selected[0].candidate.rerank_score >= 0.45);
}

#[test]
fn apply_mode_aware_rerank_empty_input_both_modes() {
    let params = rerank_params(&[]);
    assert!(apply_mode_aware_rerank(false, &[], &["rust".to_string()], &params).is_empty());
    assert!(apply_mode_aware_rerank(true, &[], &["rust".to_string()], &params).is_empty());
}

#[test]
fn apply_mode_aware_rerank_named_dense_uses_cosine_path() {
    let candidates = vec![retrieved_candidate(
        "https://docs.example.com/rust",
        "rust async runtime details long enough to keep",
        0.03,
    )];
    let selected = apply_mode_aware_rerank(
        false,
        &candidates,
        &["rust".to_string()],
        &rerank_params(&[]),
    );
    assert!(selected.is_empty());
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
fn authoritative_domains_match_exact_and_suffix_hosts() {
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

// ---- Adaptive ask_full_docs resolver (bd axon_rust-721) ----

#[test]
fn simple_query_uses_4_full_docs_when_user_default() {
    // cfg.ask_full_docs is left at its hardcoded default (6) and not pinned
    // (ask_full_docs_explicit=false). A simple query should flip to the
    // adaptive value of 4.
    let (resolved, source) = resolve_ask_full_docs(6, false, QueryComplexity::Simple);
    assert_eq!(resolved, 4);
    assert_eq!(source.as_str(), "adaptive_simple");
}

#[test]
fn complex_query_uses_6_full_docs_when_user_default() {
    let (resolved, source) = resolve_ask_full_docs(6, false, QueryComplexity::Complex);
    assert_eq!(resolved, 6);
    assert_eq!(source.as_str(), "adaptive_complex");
}

#[test]
fn user_explicit_override_wins_over_adaptive() {
    // The user pinned AXON_ASK_FULL_DOCS=10. Even on a simple query the
    // resolver must defer to the explicit value.
    let (resolved, source) = resolve_ask_full_docs(10, true, QueryComplexity::Simple);
    assert_eq!(resolved, 10);
    assert_eq!(source.as_str(), "user_override");

    // And on a complex query.
    let (resolved, source) = resolve_ask_full_docs(7, true, QueryComplexity::Complex);
    assert_eq!(resolved, 7);
    assert_eq!(source.as_str(), "user_override");
}

#[test]
fn complexity_hint_matches_use_dual_signal() {
    // Single-token / keyword-shaped queries: use_dual=false → Simple.
    let simple = build_query_forms("rust");
    assert!(!simple.use_dual);
    assert_eq!(simple.complexity_hint, QueryComplexity::Simple);

    // Multi-keyword NL question that survives stop-word stripping:
    // use_dual=true → Complex.
    let complex = build_query_forms("how do PreToolUse hook fields work in Claude Code");
    assert!(complex.use_dual);
    assert_eq!(complex.complexity_hint, QueryComplexity::Complex);
}

#[test]
fn full_docs_source_strings_are_stable() {
    // The diagnostic surface is a public contract — guard against typos.
    assert_eq!(FullDocsSource::UserOverride.as_str(), "user_override");
    assert_eq!(FullDocsSource::AdaptiveSimple.as_str(), "adaptive_simple");
    assert_eq!(FullDocsSource::AdaptiveComplex.as_str(), "adaptive_complex");
}
