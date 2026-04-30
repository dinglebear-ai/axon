use axon::crates::services::query::{
    map_ask_payload, map_evaluate_payload, map_query_results, map_retrieve_result,
    map_suggest_payload,
};

// ── map_query_results ─────────────────────────────────────────────────────────

#[test]
fn map_query_results_preserves_all_items() {
    let items = vec![
        serde_json::json!({"rank": 1, "url": "https://a.com", "snippet": "alpha"}),
        serde_json::json!({"rank": 2, "url": "https://b.com", "snippet": "beta"}),
    ];
    let result = map_query_results(items.clone());
    assert_eq!(result.results.len(), 2);
    assert_eq!(result.results[0]["url"], "https://a.com");
    assert_eq!(result.results[1]["url"], "https://b.com");
}

#[test]
fn map_query_results_empty_list_yields_empty_result() {
    let result = map_query_results(Vec::new());
    assert!(result.results.is_empty());
}

// ── map_retrieve_result ───────────────────────────────────────────────────────

#[test]
fn map_retrieve_result_with_content_produces_one_chunk() {
    let result = map_retrieve_result(3, "some content here".to_string());
    assert_eq!(result.chunks.len(), 1);
    assert_eq!(result.chunks[0]["chunk_count"], 3);
    assert_eq!(result.chunks[0]["content"], "some content here");
}

#[test]
fn map_retrieve_result_zero_chunks_yields_empty() {
    let result = map_retrieve_result(0, String::new());
    assert!(result.chunks.is_empty());
}

#[test]
fn map_retrieve_result_zero_count_empty_content_yields_empty() {
    let result = map_retrieve_result(0, "should still be empty".to_string());
    // chunk_count == 0 means nothing was found
    assert!(result.chunks.is_empty());
}

// ── map_ask_payload ───────────────────────────────────────────────────────────

#[test]
fn map_ask_payload_wraps_value() {
    let payload = serde_json::json!({
        "query": "what is a vector database?",
        "answer": "A vector database stores embeddings...",
        "timing_ms": {"total": 1200}
    });
    let result = map_ask_payload(payload.clone());
    assert_eq!(result.payload, payload);
}

#[test]
fn map_ask_payload_preserves_null() {
    let result = map_ask_payload(serde_json::Value::Null);
    assert_eq!(result.payload, serde_json::Value::Null);
}

// ── map_evaluate_payload ──────────────────────────────────────────────────────

#[test]
fn map_evaluate_payload_wraps_value() {
    let payload = serde_json::json!({
        "query": "is RAG effective?",
        "rag_answer": "Yes, RAG improves grounding.",
        "baseline_answer": "It depends.",
        "analysis_answer": "RAG wins on accuracy."
    });
    let result = map_evaluate_payload(payload.clone());
    assert_eq!(result.payload, payload);
}

#[test]
fn map_evaluate_payload_preserves_object_shape() {
    let payload = serde_json::json!({"ok": true});
    let result = map_evaluate_payload(payload.clone());
    assert_eq!(result.payload["ok"], true);
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
