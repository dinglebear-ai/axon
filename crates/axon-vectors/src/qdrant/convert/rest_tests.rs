//! Regression tests for the raw-JSON Qdrant filter builders.
//!
//! These pin the wire shape of OR filters: a bare `should` array (which Qdrant
//! treats as min_should = 1) with NO sibling `min_should` object. A sibling
//! `"min_should": {"min_count": 1}` is malformed for Qdrant's REST filter API
//! (MinShould requires the conditions nested inside it) and is rejected with
//! HTTP 400 at runtime — a defect unit tests over self-constructed JSON missed.

use super::*;

#[test]
fn canonical_uri_filter_has_bare_should_without_min_should() {
    let filter = canonical_uri_filter_json("https://example.com/docs", false);
    let should = filter
        .get("should")
        .and_then(|value| value.as_array())
        .expect("canonical-uri filter must expose a `should` array");
    assert_eq!(
        should.len(),
        3,
        "url + source_item_key + canonical_uri arms"
    );
    assert!(
        filter.get("min_should").is_none(),
        "must NOT emit a sibling min_should object (Qdrant 400s on it): {filter}"
    );
}

#[test]
fn multi_value_condition_is_bare_should_without_min_should() {
    let value = serde_json::json!(["rust", "python", "go"]);
    let condition = condition_json("code_language", &value);
    let should = condition
        .get("should")
        .and_then(|value| value.as_array())
        .expect("multi-value OR condition must expose a `should` array");
    assert_eq!(should.len(), 3);
    assert!(
        condition.get("min_should").is_none(),
        "multi-value OR condition must NOT emit a sibling min_should: {condition}"
    );
}

#[test]
fn single_value_condition_is_a_flat_match() {
    let condition = condition_json("code_language", &serde_json::json!(["rust"]));
    assert!(
        condition.get("should").is_none(),
        "a single-value filter collapses to a flat match, not a should array"
    );
    assert_eq!(
        condition.get("key").and_then(|value| value.as_str()),
        Some("code_language")
    );
}
