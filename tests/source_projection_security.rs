use axon_api::source::{CodeSearchRequest, SourceProjectionInput};

#[test]
fn code_search_rejects_mutation_and_idempotency_controls() {
    for forbidden in ["ensure_fresh", "cwd"] {
        let value = serde_json::json!({
            "inputs": [{"input": "needle"}],
            "options": {(forbidden): true}
        });
        assert!(
            serde_json::from_value::<CodeSearchRequest>(value).is_err(),
            "code search must reject {forbidden}"
        );
    }
    let idempotent = serde_json::json!({
        "inputs": [{"input": "needle", "idempotency_key": "not-allowed"}],
        "options": {}
    });
    assert!(serde_json::from_value::<CodeSearchRequest>(idempotent).is_err());
}

#[test]
fn source_idempotency_is_per_item_and_never_part_of_code_search() {
    let input: SourceProjectionInput = serde_json::from_value(serde_json::json!({
        "input": "https://example.com",
        "idempotency_key": "stable-key"
    }))
    .unwrap();
    assert_eq!(input.idempotency_key.as_deref(), Some("stable-key"));
}
