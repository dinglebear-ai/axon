use axon::crates::services::query::{
    map_ask_payload, map_evaluate_payload, map_query_results, map_retrieve_result,
    map_suggest_payload,
};

// ── map_query_results ─────────────────────────────────────────────────────────

#[test]
fn map_query_results_preserves_all_items() {
    let items = vec![
        serde_json::json!({"rank": 1, "score": 0.9, "rerank_score": 0.8, "url": "https://a.com", "source": "docs", "snippet": "alpha", "chunk_index": null}),
        serde_json::json!({"rank": 2, "score": 0.7, "rerank_score": 0.6, "url": "https://b.com", "source": "docs", "snippet": "beta", "chunk_index": 2}),
    ];
    let result = map_query_results(items.clone()).expect("valid query results");
    assert_eq!(result.results.len(), 2);
    assert_eq!(result.results[0].url, "https://a.com");
    assert_eq!(result.results[1].url, "https://b.com");
}

#[test]
fn map_query_results_empty_list_yields_empty_result() {
    let result = map_query_results(Vec::new()).expect("empty query results");
    assert!(result.results.is_empty());
}

// ── map_retrieve_result ───────────────────────────────────────────────────────

#[test]
fn map_retrieve_result_with_content_produces_one_chunk() {
    let result = map_retrieve_result(3, "some content here".to_string());
    assert_eq!(result.chunk_count, 3);
    assert_eq!(result.content, "some content here");
}

#[test]
fn map_retrieve_result_zero_chunks_yields_empty() {
    let result = map_retrieve_result(0, String::new());
    assert_eq!(result.chunk_count, 0);
    assert!(result.content.is_empty());
}

#[test]
fn map_retrieve_result_zero_count_empty_content_yields_empty() {
    let result = map_retrieve_result(0, "should still be empty".to_string());
    assert_eq!(result.chunk_count, 0);
    assert!(result.content.is_empty());
}

// ── map_ask_payload ───────────────────────────────────────────────────────────

#[test]
fn map_ask_payload_wraps_value() {
    let payload = serde_json::json!({
        "query": "what is a vector database?",
        "answer": "A vector database stores embeddings...",
        "timing_ms": {"retrieval": 1, "context_build": 2, "graph": 3, "llm": 4, "total": 10}
    });
    let result = map_ask_payload(payload.clone()).expect("valid ask payload");
    assert_eq!(result.query, "what is a vector database?");
    assert_eq!(result.answer, "A vector database stores embeddings...");
}

#[test]
fn map_ask_payload_rejects_null() {
    let result = map_ask_payload(serde_json::Value::Null);
    assert!(result.is_err());
}

// ── map_evaluate_payload ──────────────────────────────────────────────────────

#[test]
fn map_evaluate_payload_wraps_value() {
    let payload = serde_json::json!({
        "query": "is RAG effective?",
        "rag_answer": "Yes, RAG improves grounding.",
        "baseline_answer": "It depends.",
        "analysis_answer": "RAG wins on accuracy.",
        "source_urls": [],
        "crawl_suggestions": [],
        "crawl_enqueue_outcomes": [],
        "ref_chunk_count": 0,
        "timing_ms": {"retrieval": 1, "context_build": 2, "rag_llm": 3, "baseline_llm": 4, "research_elapsed_ms": 5, "analysis_llm_ms": 6, "total": 21}
    });
    let result = map_evaluate_payload(payload.clone()).expect("valid evaluate payload");
    assert_eq!(result.query, "is RAG effective?");
    assert_eq!(result.rag_answer, "Yes, RAG improves grounding.");
}

#[test]
fn map_evaluate_payload_rejects_invalid_object_shape() {
    let payload = serde_json::json!({"ok": true});
    let result = map_evaluate_payload(payload.clone());
    assert!(result.is_err());
}

// ── map_suggest_payload ───────────────────────────────────────────────────────

#[test]
fn map_suggest_payload_extracts_urls() {
    let payload = serde_json::json!({
        "collection": "cortex",
        "requested": 3,
        "suggestions": [
            {"url": "https://docs.example.com/guide", "reason": "Core guide"},
            {"url": "https://api.example.com/reference", "reason": "API reference"}
        ],
        "rejected_existing": []
    });
    let result = map_suggest_payload(&payload).expect("valid suggest payload");
    assert_eq!(result.suggestions.len(), 2);
    assert_eq!(result.suggestions[0].url, "https://docs.example.com/guide");
    assert_eq!(result.suggestions[0].reason, "Core guide");
    assert_eq!(
        result.suggestions[1].url,
        "https://api.example.com/reference"
    );
    assert_eq!(result.suggestions[1].reason, "API reference");
}

#[test]
fn map_suggest_payload_empty_suggestions_yields_empty_urls() {
    let payload = serde_json::json!({
        "suggestions": [],
        "rejected_existing": []
    });
    let result = map_suggest_payload(&payload).expect("valid empty payload");
    assert!(result.suggestions.is_empty());
}

#[test]
fn map_suggest_payload_missing_suggestions_returns_error() {
    let payload = serde_json::json!({"collection": "cortex"});
    let err = map_suggest_payload(&payload);
    assert!(err.is_err());
    assert!(err.unwrap_err().to_string().contains("missing suggestions"));
}

#[test]
fn map_suggest_payload_rejects_items_without_url_field() {
    let payload = serde_json::json!({
        "suggestions": [
            {"url": "https://good.com/docs", "reason": "valid"},
            {"reason": "no url field here"},
            {"url": "https://also-good.com/api", "reason": "also valid"}
        ]
    });
    // fail-fast: missing url field at index 1 returns an error
    assert!(map_suggest_payload(&payload).is_err());
}
