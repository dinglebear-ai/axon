use super::*;
use crate::services::types::{
    AskExplainCandidate, AskExplainContext, AskExplainContextRendered, AskExplainContextSourceTier,
    AskExplainFilterDecisionKind, AskExplainFullDocFetchMode, AskExplainFullDocFetchSkipReason,
    AskExplainMode, AskExplainRenderedContextFormat, AskExplainScoreComponentStatus,
    AskExplainScoreKind, AskExplainSelectionDecisionKind, AskTiming, CorpusHealthKind,
};
use serde_json::json;

// ── map_retrieve_result ───────────────────────────────────────────────────

#[test]
fn map_retrieve_zero_chunks_returns_empty() {
    let result = map_retrieve_result(0, "some content".to_string());
    assert_eq!(result.chunk_count, 0);
    assert_eq!(result.content, "");
    assert_eq!(result.requested_url, None);
    assert_eq!(result.backend, None);
}

#[test]
fn map_retrieve_nonzero_chunks() {
    let result = map_retrieve_result(5, "hello".to_string());
    assert_eq!(result.chunk_count, 5);
    assert_eq!(result.content, "hello");
    assert_eq!(result.next_cursor, None);
}

#[test]
fn map_retrieve_result_serializes_legacy_shape_when_metadata_absent() {
    let result = map_retrieve_result(5, "hello".to_string());
    let value = serde_json::to_value(result).expect("retrieve result serializes");
    assert_eq!(
        value,
        serde_json::json!({
            "chunk_count": 5,
            "content": "hello"
        })
    );
}

#[test]
fn map_direct_retrieve_preserves_metadata() {
    let result = map_direct_retrieve_result(DirectRetrieveResult {
        requested_url: "example.com/docs".to_string(),
        matched_url: Some("https://example.com/docs".to_string()),
        chunk_count: 2,
        content: "hello".to_string(),
        truncated: true,
        warnings: vec!["partial result".to_string()],
        variant_errors: vec![crate::vector::ops::qdrant::RetrieveVariantError {
            url: "https://example.com/docs/".to_string(),
            error: "timeout".to_string(),
        }],
    });
    assert_eq!(result.requested_url.as_deref(), Some("example.com/docs"));
    assert_eq!(
        result.matched_url.as_deref(),
        Some("https://example.com/docs")
    );
    assert!(result.truncated);
    assert_eq!(result.warnings, vec!["partial result"]);
    assert_eq!(result.variant_errors[0].url, "https://example.com/docs/");
    assert_eq!(result.backend, Some(DocumentBackend::Qdrant));
}

// ── map_suggest_payload ───────────────────────────────────────────────────

#[test]
fn map_suggest_valid() {
    let payload = json!({
        "suggestions": [
            { "url": "https://example.com/a", "reason": "A docs gap" },
            { "url": "https://example.com/b" }
        ]
    });
    let result = map_suggest_payload(&payload).unwrap();
    assert_eq!(result.suggestions.len(), 2);
    assert_eq!(result.suggestions[0].url, "https://example.com/a");
    assert_eq!(result.suggestions[0].reason, "A docs gap");
    assert_eq!(result.suggestions[1].url, "https://example.com/b");
    assert_eq!(result.suggestions[1].reason, "Suggested by model");
}

#[test]
fn map_suggest_missing_suggestions() {
    let payload = json!({});
    let err = map_suggest_payload(&payload).unwrap_err();
    assert!(
        err.to_string().contains("suggestions"),
        "error must mention 'suggestions', got: {err}"
    );
}

#[test]
fn map_suggest_entry_missing_url() {
    let payload = json!({
        "suggestions": [{ "reason": "no url key here" }]
    });
    let err = map_suggest_payload(&payload).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("suggestions[0]"),
        "error must reference suggestions[0], got: {msg}"
    );
}

#[test]
fn map_suggest_empty_suggestions() {
    let payload = json!({ "suggestions": [] });
    let result = map_suggest_payload(&payload).unwrap();
    assert!(result.suggestions.is_empty());
}

// ── map_ask_payload ──────────────────────────────────────────────────────

#[test]
fn map_ask_payload_typed() {
    let payload = json!({
        "query": "what is axon?",
        "answer": "A crawler.",
        "diagnostics": null,
        "timing_ms": {
            "retrieval": 1,
            "context_build": 2,
            "llm": 3,
            "total": 6
        }
    });
    let result = map_ask_payload(payload).unwrap();
    assert_eq!(result.query, "what is axon?");
    assert_eq!(result.answer, "A crawler.");
    assert!(result.diagnostics.is_none());
    assert!(result.explain.is_none());
    assert_eq!(result.timing_ms.total, 6);
}

#[test]
fn ask_result_serializes_absent_explain_as_null() {
    let result = AskResult {
        query: "what is axon?".to_string(),
        answer: "A crawler.".to_string(),
        session: None,
        warnings: Vec::new(),
        diagnostics: None,
        explain: None,
        timing_ms: AskTiming {
            retrieval: 1,
            context_build: 2,
            llm: 3,
            total: 6,
            tei_embed_ms: None,
            qdrant_primary_ms: None,
            qdrant_secondary_ms: None,
            rerank_ms: None,
            top_select_ms: None,
            full_doc_fetch_ms: None,
            supplemental_ms: None,
            llm_ttft_ms: None,
            llm_total_ms: None,
            streamed: None,
            normalize_ms: None,
        },
    };

    let value = serde_json::to_value(result).expect("ask result serializes");

    assert_eq!(value["explain"], serde_json::Value::Null);
}

#[test]
fn map_ask_payload_preserves_explain_contract() {
    let payload = json!({
        "query": "widget marketplace plugins",
        "answer": "",
        "diagnostics": null,
        "explain": {
            "mode": "explain_only",
            "retrieval": {
                "query": "widget marketplace plugins",
                "keyword_query": "widget marketplace plugins",
                "dual_search": false,
                "collection": "cortex",
                "candidate_limit": 150,
                "hybrid_search_enabled": true,
                "hybrid_candidate_limit": 100,
                "score_kind": "cosine",
                "vector_mode": "unnamed",
                "sparse_query_status": null
            },
            "candidates": [{
                "id": "c0",
                "url": "https://docs.widget.dev/docs/en/discover-plugins",
                "chunk_index": 2,
                "raw_rerank_rank": 1,
                "planned_full_doc_rank": 1,
                "selected_context_rank": 1,
                "insertion_mode": "top_chunk",
                "retrieval_score": 0.7,
                "rerank_score": 1.18,
                "score_kind": "cosine",
                "score_components": [{
                    "name": "product_authority_boost",
                    "value": 0.35,
                    "status": "applied",
                    "reason": "docs-like URL contains query product token"
                }],
                "filter_decisions": [{
                    "kind": "kept",
                    "reason": "passed topical overlap"
                }],
                "selection_decisions": [{
                    "kind": "selected_top_chunk",
                    "reason": "ranked in top chunk set"
                }],
                "snippet": "official marketplace"
            }],
            "context": {
                "planned_full_doc_urls": [
                    "https://docs.widget.dev/docs/en/discover-plugins"
                ],
                "full_doc_fetch_skipped": false,
                "full_doc_fetch_skip_reason": "disabled",
                "full_doc_fetch_mode": "cosine",
                "final_source_order": [{
                    "source_id": "S1",
                    "url": "https://docs.widget.dev/docs/en/discover-plugins",
                    "tier": "top_chunk"
                }],
                "context_char_budget": 120000,
                "context_chars_used": 90,
                "context_bytes_budget": 120000,
                "context_bytes_used": 90,
                "rendered_context": {
                    "format": "axon_sources_v1",
                    "content": "Sources:\n## Top Chunk [S1]: docs.widget.dev/docs/en/discover-plugins\n\nofficial marketplace",
                    "bytes_used": 90,
                    "chars_used": 90
                },
                "truncated_by_budget": false
            },
            "candidate_trace_limit": 50,
            "candidate_trace_truncated": false,
            "llm_skipped": true
        },
        "timing_ms": {
            "retrieval": 1,
            "context_build": 2,
            "llm": 0,
            "total": 3
        }
    });

    let result = map_ask_payload(payload).unwrap();
    let explain = result.explain.expect("explain trace should deserialize");
    let candidate = &explain.candidates[0];

    assert_eq!(explain.mode, AskExplainMode::ExplainOnly);
    assert_eq!(explain.retrieval.score_kind, AskExplainScoreKind::Cosine);
    assert_eq!(
        candidate.score_components[0].status,
        AskExplainScoreComponentStatus::Applied
    );
    assert_eq!(
        candidate.filter_decisions[0].kind,
        AskExplainFilterDecisionKind::Kept
    );
    assert_eq!(
        candidate.selection_decisions[0].kind,
        AskExplainSelectionDecisionKind::SelectedTopChunk
    );
    assert_eq!(candidate.raw_rerank_rank, Some(1));
    assert_eq!(candidate.selected_context_rank, Some(1));
    assert_eq!(
        explain
            .context
            .rendered_context
            .as_ref()
            .map(|rendered| rendered.content.as_str()),
        Some(
            "Sources:\n## Top Chunk [S1]: docs.widget.dev/docs/en/discover-plugins\n\nofficial marketplace"
        )
    );
    assert_eq!(
        explain.context.final_source_order[0].tier,
        AskExplainContextSourceTier::TopChunk
    );
    assert_eq!(
        explain.context.full_doc_fetch_mode,
        AskExplainFullDocFetchMode::Cosine
    );
    assert_eq!(
        explain.context.full_doc_fetch_skip_reason,
        AskExplainFullDocFetchSkipReason::Disabled
    );
    assert_eq!(explain.context.context_bytes_budget, 120000);
    assert_eq!(explain.context.context_bytes_used, 90);
    assert!(explain.llm_skipped);
}

#[test]
fn ask_explain_context_omits_rendered_context_by_default() {
    let value = serde_json::json!({
        "planned_full_doc_urls": [],
        "full_doc_fetch_skipped": false,
        "full_doc_fetch_skip_reason": "disabled",
        "full_doc_fetch_mode": "cosine",
        "final_source_order": [],
        "context_char_budget": 120000,
        "context_chars_used": 42,
        "context_bytes_budget": 120000,
        "context_bytes_used": 42,
        "truncated_by_budget": false
    });

    let parsed: AskExplainContext = serde_json::from_value(value).unwrap();
    assert!(parsed.rendered_context.is_none());
    let serialized = serde_json::to_value(parsed).unwrap();
    assert!(serialized.get("rendered_context").is_none());
}

#[test]
fn ask_explain_rendered_context_preserves_legacy_string_payloads() {
    let value = serde_json::json!({
        "planned_full_doc_urls": [],
        "full_doc_fetch_skipped": false,
        "full_doc_fetch_skip_reason": "disabled",
        "full_doc_fetch_mode": "cosine",
        "final_source_order": [],
        "context_char_budget": 120000,
        "context_chars_used": 13,
        "rendered_context": "Sources:\nbody",
        "truncated_by_budget": false
    });

    let parsed: AskExplainContext = serde_json::from_value(value).unwrap();
    assert_eq!(
        parsed.rendered_context,
        Some(AskExplainContextRendered {
            format: AskExplainRenderedContextFormat::AxonSourcesV1,
            content: "Sources:\nbody".to_string(),
            bytes_used: "Sources:\nbody".len(),
            chars_used: "Sources:\nbody".chars().count(),
        })
    );
}

#[test]
fn ask_explain_candidate_deserializes_without_rank_fields() {
    let value = serde_json::json!({
        "id": "candidate-1",
        "url": "https://docs.example.com/page",
        "chunk_index": null,
        "retrieval_score": 0.5,
        "rerank_score": 0.5,
        "score_kind": "cosine",
        "score_components": [],
        "filter_decisions": [],
        "selection_decisions": [],
        "snippet": "example"
    });
    let parsed: AskExplainCandidate = serde_json::from_value(value).unwrap();
    assert_eq!(parsed.raw_rerank_rank, None);
    assert_eq!(parsed.planned_full_doc_rank, None);
    assert_eq!(parsed.selected_context_rank, None);
    assert_eq!(parsed.insertion_mode, None);
}

#[test]
fn map_ask_payload_preserves_adaptive_diagnostics() {
    let payload = json!({
        "query": "what is axon?",
        "answer": "A crawler.",
        "diagnostics": {
            "candidate_pool": 12,
            "reranked_pool": 8,
            "chunks_selected": 4,
            "full_docs_selected": 2,
            "supplemental_selected": 1,
            "context_chars": 3000,
            "full_doc_fetch_skipped": true,
            "full_doc_fetch_skip_reason": "low_complexity",
            "detected_complexity": "simple",
            "resolved_full_docs": 2,
            "full_docs_source": "adaptive",
            "min_relevance_score": 0.4,
            "doc_fetch_concurrency": 8,
            "top_domains": ["docs.example.com"],
            "authority_ratio": 0.75,
            "configured_authority_ratio": 0.25,
            "product_authority_ratio": 0.75,
            "corpus_health": {
                "kind": "healthy",
                "reason": "retrieval produced selected context",
                "selected_domain_count": 2,
                "top_domain_count": 5
            }
        },
        "timing_ms": {
            "retrieval": 1,
            "context_build": 2,
            "llm": 3,
            "total": 6
        }
    });
    let result = map_ask_payload(payload).unwrap();
    let diagnostics = result.diagnostics.expect("diagnostics should deserialize");
    assert!(diagnostics.full_doc_fetch_skipped);
    assert_eq!(diagnostics.full_doc_fetch_skip_reason, "low_complexity");
    assert_eq!(diagnostics.detected_complexity, "simple");
    assert_eq!(diagnostics.resolved_full_docs, 2);
    assert_eq!(diagnostics.full_docs_source, "adaptive");
    assert_eq!(diagnostics.configured_authority_ratio, 0.25);
    assert_eq!(diagnostics.product_authority_ratio, 0.75);
    let health = diagnostics.corpus_health.expect("corpus health");
    assert_eq!(health.kind, CorpusHealthKind::Healthy);
    assert_eq!(health.selected_domain_count, 2);
}

#[test]
fn map_ask_payload_rejects_invalid_shape() {
    let err = map_ask_payload(json!({ "answer": "missing query and timing" })).unwrap_err();
    assert!(err.to_string().contains("invalid ask payload"));
}

// ── map_evaluate_payload ─────────────────────────────────────────────────

#[test]
fn map_evaluate_payload_typed() {
    let payload = json!({
        "query": "what is axon?",
        "rag_answer": "RAG",
        "baseline_answer": "Baseline",
        "analysis_answer": "Analysis",
        "source_urls": ["https://example.com/a"],
        "crawl_suggestions": [{ "url": "https://example.com/b", "reason": "gap" }],
        "crawl_enqueue_outcomes": [],
        "ref_chunk_count": 3,
        "diagnostics": null,
        "timing_ms": {
            "retrieval": 1,
            "context_build": 2,
            "rag_llm": 3,
            "baseline_llm": 4,
            "research_elapsed_ms": 5,
            "analysis_llm_ms": 6,
            "total": 21
        }
    });
    let result = map_evaluate_payload(payload).unwrap();
    assert_eq!(result.query, "what is axon?");
    assert_eq!(result.source_urls, vec!["https://example.com/a"]);
    assert_eq!(result.crawl_suggestions[0].reason, "gap");
    assert_eq!(result.timing_ms.total, 21);
}

#[test]
fn map_evaluate_payload_rejects_invalid_shape() {
    let err = map_evaluate_payload(json!({ "query": "missing fields" })).unwrap_err();
    assert!(err.to_string().contains("invalid evaluate payload"));
}

// ── map_query_results ─────────────────────────────────────────────────────

#[test]
fn map_query_results_passthrough_empty() {
    let result = map_query_results(vec![]).unwrap();
    assert!(result.results.is_empty());
}

#[test]
fn map_query_results_typed_nonempty() {
    let items = vec![
        json!({
            "rank": 1,
            "score": 0.9,
            "rerank_score": 1.1,
            "url": "https://a.com",
            "source": "a.com",
            "snippet": "alpha",
            "chunk_index": 2
        }),
        json!({
            "rank": 2,
            "score": 0.8,
            "rerank_score": 0.95,
            "url": "https://b.com",
            "source": "b.com",
            "snippet": "bravo",
            "chunk_index": null
        }),
    ];
    let result = map_query_results(items).unwrap();
    assert_eq!(result.results.len(), 2);
    assert_eq!(result.results[0].url, "https://a.com");
    assert_eq!(result.results[0].chunk_index, Some(2));
    assert_eq!(result.results[1].source, "b.com");
    assert_eq!(result.results[1].chunk_index, None);
}

#[test]
fn map_query_results_rejects_missing_required_fields() {
    let err = map_query_results(vec![json!({ "url": "https://a.com" })]).unwrap_err();
    assert!(
        err.to_string().contains("query result[0]"),
        "error should identify the bad result index, got: {err}"
    );
}

#[tokio::test]
async fn query_reports_typed_diagnostics_payload_without_ask_diagnostics() {
    use httpmock::Method::POST;
    use httpmock::MockServer;

    // TEI succeeds so query proceeds to vector mode probe.
    let tei = MockServer::start_async().await;
    tei.mock_async(|when, then| {
        when.method(POST).path("/embed");
        then.status(200)
            .json_body(json!([[0.1_f32, 0.2_f32, 0.3_f32, 0.4_f32]]));
    })
    .await;

    // Qdrant probe fails with 404, which should surface as structured diagnostics.
    let qdrant = MockServer::start_async().await;
    qdrant
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET)
                .path_matches(regex::Regex::new("/collections/").unwrap());
            then.status(404);
        })
        .await;

    let mut cfg = Config::test_default();
    cfg.tei_url = tei.base_url();
    cfg.qdrant_url = qdrant.base_url();
    cfg.collection = "diag_test_collection".to_string();
    cfg.ask_diagnostics = false;

    let err = query(
        &cfg,
        "diagnostics regression test query",
        Pagination {
            limit: 5,
            offset: 0,
        },
    )
    .await
    .expect_err("query should fail when collection is missing");

    let diag = diagnostics_from_error(err.as_ref())
        .expect("diagnostics payload should be attached without ask_diagnostics");
    assert_eq!(diag["stage"], "query_vector_search_dispatch");
    assert_eq!(diag["collection"], "diag_test_collection");
    assert_eq!(
        diag["qdrant_url"],
        reqwest::Url::parse(&qdrant.base_url()).unwrap().to_string()
    );
    assert_eq!(diag["query_len"], "diagnostics regression test query".len());
    assert_eq!(diag["mode"]["hybrid_search_enabled"], true);
    assert_eq!(diag["search_context"]["command"], "query");
    assert_eq!(diag["search_context"]["request_limit"], 80);
    assert_eq!(diag["search_context"]["sparse_query_empty"], false);
    assert!(diag["error"].as_str().unwrap_or("").contains("404"));
}
