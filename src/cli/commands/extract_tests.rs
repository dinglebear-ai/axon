use super::extract_provenance_message;

#[test]
fn provenance_message_reports_deterministic_only_without_fallback() {
    let summary = serde_json::json!({
        "deterministic_pages": 2,
        "llm_fallback_pages": 0,
        "parser_hits": {
            "json-ld": 1,
            "open-graph": 1
        }
    });

    let message = extract_provenance_message(&summary).expect("message");

    assert!(message.contains("2 page(s) handled by json-ld, open-graph"));
    assert!(message.contains("LLM fallback was not used"));
}

#[test]
fn provenance_message_reports_mixed_parser_and_fallback_use() {
    let summary = serde_json::json!({
        "deterministic_pages": 1,
        "llm_fallback_pages": 3,
        "parser_hits": {
            "html-table": 1
        }
    });

    let message = extract_provenance_message(&summary).expect("message");

    assert!(message.contains("1 page(s) handled by html-table"));
    assert!(message.contains("LLM fallback ran for 3 page(s)"));
}
