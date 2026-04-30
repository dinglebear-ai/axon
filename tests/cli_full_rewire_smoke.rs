/// Smoke tests for Task 3.2: CLI full rewire through services layer.
///
/// These tests verify that:
/// 1. All service module types are importable from test context (proving pub visibility).
/// 2. Pure mapping helpers work correctly in service modules.
/// 3. Service module functions have the correct signatures (compile-time proof).
///
/// No live services are required — all tests use pure functions or type assertions.
// ── services::query ──────────────────────────────────────────────────────────
use axon::crates::services::query::{
    map_ask_payload, map_evaluate_payload, map_query_results, map_retrieve_result,
    map_suggest_payload,
};
use axon::crates::services::types::{
    AskResult, EvaluateResult, IngestResult, MapResult, Pagination, QueryResult, ResearchResult,
    RetrieveOptions, RetrieveResult, ScrapeResult, ScreenshotResult, SearchOptions, SearchResult,
    SuggestResult,
};

#[test]
fn smoke_map_query_results_is_importable_and_works() {
    let items = vec![
        serde_json::json!({"rank": 1, "url": "https://docs.example.com", "snippet": "first"}),
        serde_json::json!({"rank": 2, "url": "https://api.example.com", "snippet": "second"}),
    ];
    let result: QueryResult = map_query_results(items);
    assert_eq!(result.results.len(), 2);
    assert_eq!(result.results[0]["rank"], 1);
    assert_eq!(result.results[1]["rank"], 2);
}

#[test]
fn smoke_map_query_results_empty() {
    let result: QueryResult = map_query_results(Vec::new());
    assert!(result.results.is_empty());
}

#[test]
fn smoke_map_retrieve_result_with_content() {
    let result: RetrieveResult = map_retrieve_result(5, "chunk content here".to_string());
    assert_eq!(result.chunks.len(), 1);
    assert_eq!(result.chunks[0]["chunk_count"], 5);
    assert!(
        result.chunks[0]["content"]
            .as_str()
            .unwrap()
            .contains("chunk content")
    );
}

#[test]
fn smoke_map_retrieve_result_zero_chunks_yields_empty() {
    let result: RetrieveResult = map_retrieve_result(0, String::new());
    assert!(result.chunks.is_empty());
}

#[test]
fn smoke_map_ask_payload_wraps_value() {
    let payload = serde_json::json!({
        "query": "what is RAG?",
        "answer": "Retrieval-Augmented Generation",
        "timing_ms": {"total": 500}
    });
    let result: AskResult = map_ask_payload(payload.clone());
    assert_eq!(result.payload["query"], "what is RAG?");
    assert_eq!(result.payload["answer"], "Retrieval-Augmented Generation");
}

#[test]
fn smoke_map_evaluate_payload_wraps_value() {
    let payload = serde_json::json!({
        "question": "is RAG better?",
        "note": "output emitted to stdout"
    });
    let result: EvaluateResult = map_evaluate_payload(payload.clone());
    assert_eq!(result.payload, payload);
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
    assert_eq!(result.urls.len(), 2);
    assert_eq!(result.urls[0], "https://docs.example.com/guide");
}

#[test]
fn smoke_map_suggest_payload_empty_yields_empty() {
    let payload = serde_json::json!({"suggestions": []});
    let result: SuggestResult = map_suggest_payload(&payload).expect("empty payload");
    assert!(result.urls.is_empty());
}

#[test]
fn smoke_map_suggest_payload_missing_key_returns_error() {
    let payload = serde_json::json!({"other_key": "value"});
    assert!(map_suggest_payload(&payload).is_err());
}

// ── services::scrape ─────────────────────────────────────────────────────────

use axon::crates::services::scrape::map_scrape_payload;

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
        "mapped_urls": 42,
        "urls": ["https://docs.example.com/page1"]
    });
    let result = MapResult {
        payload: payload.clone(),
    };
    assert_eq!(result.payload["mapped_urls"], 42);
}

// ── services::search ──────────────────────────────────────────────────────────

use axon::crates::services::search::{map_research_payload, map_search_results};

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
    let payload = serde_json::json!({
        "query": "rust async patterns",
        "summary": "Rust uses async/await",
        "search_results": []
    });
    let result: ResearchResult = map_research_payload(payload.clone());
    assert_eq!(result.payload["query"], "rust async patterns");
}

// ── services::crawl ───────────────────────────────────────────────────────────

use axon::crates::services::crawl::{map_crawl_job_result, map_crawl_start_result};
use axon::crates::services::types::CrawlStartResult;

#[test]
fn smoke_map_crawl_start_result_wraps_job_ids() {
    use std::path::Path;
    let jobs = vec![
        ("https://example.com".to_string(), "uuid-1".to_string()),
        ("https://other.com".to_string(), "uuid-2".to_string()),
    ];
    let result: CrawlStartResult = map_crawl_start_result(Path::new("/tmp/output"), &jobs);
    assert_eq!(result.job_ids.len(), 2);
    assert_eq!(result.job_ids[0], "uuid-1");
}

#[test]
fn smoke_map_crawl_start_result_empty() {
    use std::path::Path;
    let result: CrawlStartResult = map_crawl_start_result(Path::new("/tmp/output"), &[]);
    assert!(result.job_ids.is_empty());
}

#[test]
fn smoke_map_crawl_job_result_wraps_payload() {
    let payload = serde_json::json!({"id": "uuid-1", "status": "completed"});
    let result = map_crawl_job_result(payload.clone());
    assert_eq!(result.payload["status"], "completed");
}

// ── services::embed ───────────────────────────────────────────────────────────

use axon::crates::services::embed::{map_embed_job_result, map_embed_start_result};
use axon::crates::services::types::EmbedStartResult;

#[test]
fn smoke_map_embed_start_result_wraps_job_id() {
    let result: EmbedStartResult = map_embed_start_result("embed-uuid-1".to_string());
    assert_eq!(result.job_id, "embed-uuid-1");
}

#[test]
fn smoke_map_embed_job_result_wraps_payload() {
    let payload = serde_json::json!({"id": "embed-uuid-1", "status": "pending"});
    let result = map_embed_job_result(payload);
    assert_eq!(result.payload["status"], "pending");
}

// ── services::extract ─────────────────────────────────────────────────────────

use axon::crates::services::extract::{map_extract_job_result, map_extract_start_result};
use axon::crates::services::types::ExtractStartResult;

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

// ── services::ingest ──────────────────────────────────────────────────────────

use axon::crates::services::ingest::map_ingest_result;

#[test]
fn smoke_map_ingest_result_wraps_github_payload() {
    let payload = serde_json::json!({
        "source": "github",
        "repo": "rust-lang/rust",
        "chunks": 1024
    });
    let result: IngestResult = map_ingest_result(payload.clone());
    assert_eq!(result.payload["source"], "github");
    assert_eq!(result.payload["chunks"], 1024);
}

#[test]
fn smoke_map_ingest_result_wraps_reddit_payload() {
    let payload = serde_json::json!({
        "source": "reddit",
        "target": "rust",
        "chunks": 42
    });
    let result: IngestResult = map_ingest_result(payload);
    assert_eq!(result.payload["source"], "reddit");
    assert_eq!(result.payload["target"], "rust");
}

#[test]
fn smoke_map_ingest_result_wraps_youtube_payload() {
    let payload = serde_json::json!({
        "source": "youtube",
        "url": "https://youtube.com/watch?v=test",
        "chunks": 8
    });
    let result: IngestResult = map_ingest_result(payload);
    assert_eq!(result.payload["source"], "youtube");
}

#[test]
fn smoke_map_ingest_result_wraps_sessions_payload() {
    let payload = serde_json::json!({
        "source": "sessions",
        "chunks": 100
    });
    let result: IngestResult = map_ingest_result(payload);
    assert_eq!(result.payload["source"], "sessions");
    assert_eq!(result.payload["chunks"], 100);
}

// ── services::screenshot ──────────────────────────────────────────────────────

use axon::crates::services::screenshot::map_screenshot_result;

#[test]
fn smoke_map_screenshot_result_wraps_payload() {
    let payload = serde_json::json!({
        "url": "https://example.com",
        "path": "/output/screenshots/example.png",
        "size_bytes": 204800
    });
    let result: ScreenshotResult = map_screenshot_result(payload.clone());
    assert_eq!(result.payload["size_bytes"], 204800);
    assert_eq!(result.payload["url"], "https://example.com");
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
    };
    assert_eq!(r.max_points, Some(100));

    let r2 = RetrieveOptions { max_points: None };
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
