use axon_services::map::parse_map_result;
/// Smoke tests for Task 3.2: CLI full rewire through services layer.
///
/// These tests verify that:
/// 1. All service module types are importable from test context (proving pub visibility).
/// 2. Pure mapping helpers work correctly in service modules.
/// 3. Service module functions have the correct signatures (compile-time proof).
///
/// No live services are required — all tests use pure functions or type assertions.
// ── services::query ──────────────────────────────────────────────────────────
use axon_services::query::{
    map_ask_payload, map_evaluate_payload, map_query_results, map_retrieve_result,
    map_suggest_payload,
};
use axon_services::types::{
    AskResult, EvaluateResult, MapResult, Pagination, QueryResult, ResearchResult, RetrieveOptions,
    RetrieveResult, ScrapeResult, ScreenshotResult, SearchOptions, SearchResult, SuggestResult,
};

fn citation(uri: &str, chunk_id: &str) -> serde_json::Value {
    serde_json::json!({
        "source_id": "source-test",
        "source_item_key": uri,
        "generation": "1",
        "document_id": format!("document-{chunk_id}"),
        "chunk_id": chunk_id,
        "job_id": "00000000-0000-0000-0000-000000000001",
        "canonical_uri": uri,
        "source_range": { "line_start": 1, "line_end": 2 },
        "redaction": {
            "redaction_status": "clean",
            "redaction_version": "test-v1",
            "visibility": "public",
            "redacted_field_count": 0,
            "dropped_field_count": 0,
            "detector_count": 0,
            "detector_names": []
        }
    })
}

#[test]
fn smoke_map_query_results_is_importable_and_works() {
    let items = vec![
        serde_json::json!({"rank": 1, "score": 0.9, "rerank_score": 0.8, "url": "https://docs.example.com", "source": "docs", "snippet": "first", "citation": citation("https://docs.example.com", "chunk-a"), "chunk_index": null}),
        serde_json::json!({"rank": 2, "score": 0.7, "rerank_score": 0.6, "url": "https://api.example.com", "source": "api", "snippet": "second", "citation": citation("https://api.example.com", "chunk-b"), "chunk_index": 3}),
    ];
    let result: QueryResult = map_query_results(items).expect("valid query results");
    assert_eq!(result.results.len(), 2);
    assert_eq!(result.results[0].rank, 1);
    assert_eq!(result.results[1].rank, 2);
}

#[test]
fn smoke_map_query_results_empty() {
    let result: QueryResult = map_query_results(Vec::new()).expect("empty query results");
    assert!(result.results.is_empty());
}

#[test]
fn smoke_map_retrieve_result_with_content() {
    let result: RetrieveResult = map_retrieve_result(5, "chunk content here".to_string());
    assert_eq!(result.chunk_count, 5);
    assert!(result.content.contains("chunk content"));
}

#[test]
fn smoke_map_retrieve_result_zero_chunks_yields_empty() {
    let result: RetrieveResult = map_retrieve_result(0, String::new());
    assert_eq!(result.chunk_count, 0);
    assert!(result.content.is_empty());
}

#[test]
fn smoke_map_ask_payload_wraps_value() {
    let payload = serde_json::json!({
        "query": "what is RAG?",
        "answer": "Retrieval-Augmented Generation",
        "citations": [],
        "timing_ms": {"retrieval": 1, "context_build": 2, "llm": 4, "total": 10}
    });
    let result: AskResult = map_ask_payload(payload.clone()).expect("valid ask payload");
    assert_eq!(result.query, "what is RAG?");
    assert_eq!(result.answer, "Retrieval-Augmented Generation");
}

#[test]
fn smoke_map_evaluate_payload_wraps_value() {
    let payload = serde_json::json!({
        "query": "is RAG better?",
        "rag_answer": "yes with sources",
        "baseline_answer": "maybe",
        "analysis_answer": "rag is more grounded",
        "citations": [],
        "source_urls": [],
        "crawl_suggestions": [],
        "crawl_enqueue_outcomes": [],
        "ref_chunk_count": 0,
        "timing_ms": {"retrieval": 1, "context_build": 2, "rag_llm": 3, "baseline_llm": 4, "research_elapsed_ms": 5, "analysis_llm_ms": 6, "total": 21}
    });
    let result: EvaluateResult =
        map_evaluate_payload(payload.clone()).expect("valid evaluate payload");
    assert_eq!(result.query, "is RAG better?");
    assert_eq!(result.ref_chunk_count, 0);
}

#[test]
fn smoke_map_suggest_payload_extracts_urls() {
    let payload = serde_json::json!({
        "suggestions": [
            {"url": "https://docs.example.com/guide", "reason": "Core guide"},
            {"url": "https://api.example.com/ref", "reason": "API ref"}
        ]
    });
    let result: SuggestResult = map_suggest_payload(&payload).expect("valid suggest payload");
    assert_eq!(result.suggestions.len(), 2);
    assert_eq!(result.suggestions[0].url, "https://docs.example.com/guide");
    assert_eq!(result.suggestions[0].reason, "Core guide");
}

#[test]
fn smoke_map_suggest_payload_empty_yields_empty() {
    let payload = serde_json::json!({"suggestions": []});
    let result: SuggestResult = map_suggest_payload(&payload).expect("empty payload");
    assert!(result.suggestions.is_empty());
}

#[test]
fn smoke_map_suggest_payload_missing_key_returns_error() {
    let payload = serde_json::json!({"other_key": "value"});
    assert!(map_suggest_payload(&payload).is_err());
}

// ── services::scrape ─────────────────────────────────────────────────────────

use axon_services::scrape::map_scrape_payload;

#[test]
fn smoke_map_scrape_payload_wraps_value() {
    let payload = serde_json::json!({
        "url": "https://example.com",
        "status_code": 200,
        "markdown": "# Hello",
        "title": "Example",
        "description": ""
    });
    let result: ScrapeResult = map_scrape_payload(payload.clone()).expect("valid scrape payload");
    assert_eq!(result.payload["status_code"], 200);
    assert_eq!(result.payload["url"], "https://example.com");
}

// ── services::map ─────────────────────────────────────────────────────────────

#[test]
fn smoke_map_map_payload_wraps_value() {
    let payload = serde_json::json!({
        "url": "https://docs.example.com",
        "mapped_urls": 42u64,
        "total": 42u64,
        "sitemap_urls": 0usize,
        "pages_seen": 0u32,
        "thin_pages": 0u32,
        "elapsed_ms": 100u64,
        "map_source": "sitemap",
        "warning": null,
        "urls": ["https://docs.example.com/page1"]
    });
    let result: MapResult = parse_map_result(payload).expect("valid map payload");
    assert_eq!(result.returned_url_count, 42);
    // wire JSON key remains `mapped_urls` for backward compat
    let v = serde_json::to_value(&result).unwrap();
    assert!(v.get("mapped_urls").is_some());
}

// ── services::search ──────────────────────────────────────────────────────────

use axon_services::search::{map_research_payload, map_search_results};

#[test]
fn smoke_map_search_results_wraps_items() {
    let items = vec![
        serde_json::json!({"position": 1, "title": "Result", "url": "https://r.com", "snippet": "s"}),
    ];
    let result: SearchResult = map_search_results(items);
    assert_eq!(result.results.len(), 1);
    assert_eq!(result.results[0]["position"], 1);
}

#[test]
fn smoke_map_research_payload_wraps_value() {
    use axon_services::types::{ResearchPayload, ResearchTiming, ResearchUsage, SummarySource};
    let payload = ResearchPayload {
        query: "rust async patterns".to_string(),
        limit: 10,
        offset: 0,
        search_results: vec![],
        extractions: vec![],
        source_index_status: "not_queued".to_string(),
        source_jobs: vec![],
        source_jobs_rejected: vec![],
        summary: Some("Rust uses async/await".to_string()),
        summary_source: SummarySource::Llm,
        usage: ResearchUsage::default(),
        timing_ms: ResearchTiming { total: 0 },
    };
    let result: ResearchResult = map_research_payload(payload.clone());
    assert_eq!(result.payload.query, "rust async patterns");
    assert_eq!(result.payload, payload);
}

// ── services::extract ─────────────────────────────────────────────────────────

use axon_services::extract::{map_extract_job_result, map_extract_start_result};
use axon_services::types::ExtractStartResult;

#[test]
fn smoke_map_extract_start_result_wraps_job_id() {
    let result: ExtractStartResult = map_extract_start_result("extract-uuid-1".to_string());
    assert_eq!(result.job_id, "extract-uuid-1");
}

#[test]
fn smoke_map_extract_job_result_wraps_payload() {
    let payload = serde_json::json!({"id": "extract-uuid-1", "status": "running"});
    let result = map_extract_job_result(payload);
    assert_eq!(result.payload["status"], "running");
}

#[test]
fn smoke_screenshot_result_uses_opaque_artifact_identity() {
    let result = ScreenshotResult {
        artifact_id: axon_api::source::ArtifactId::new("art_screenshot_smoke"),
        width: 1280,
        height: 720,
        captured_at: axon_api::source::Timestamp("2026-07-16T00:00:00Z".to_string()),
        warnings: Vec::new(),
    };
    let payload = serde_json::to_value(result).expect("serialize screenshot result");
    assert_eq!(payload["artifact_id"], "art_screenshot_smoke");
    assert_eq!(payload["width"], 1280);
    assert!(payload.get("path").is_none());
}

// ── services::types — Pagination and options types are constructible ──────────

#[test]
fn smoke_pagination_type_is_constructible() {
    let p = Pagination {
        limit: 10,
        offset: 5,
    };
    assert_eq!(p.limit, 10);
    assert_eq!(p.offset, 5);
}

#[test]
fn smoke_retrieve_options_type_is_constructible() {
    let r = RetrieveOptions {
        max_points: Some(100),
        cursor: Some("cursor".to_string()),
        token_budget: Some(4096),
    };
    assert_eq!(r.max_points, Some(100));
    assert_eq!(r.cursor.as_deref(), Some("cursor"));

    let r2 = RetrieveOptions {
        max_points: None,
        cursor: None,
        token_budget: None,
    };
    assert!(r2.max_points.is_none());
}

#[test]
fn smoke_search_options_type_is_constructible() {
    let s = SearchOptions {
        limit: 10,
        offset: 0,
        time_range: None,
    };
    assert_eq!(s.limit, 10);
    assert!(s.time_range.is_none());
}
