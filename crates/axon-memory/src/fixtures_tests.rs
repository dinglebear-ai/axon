//! Contract "Testing Contract" required-fixtures check: every fixture named
//! in `docs/pipeline-unification/runtime/memory-contract.md`'s fixture list
//! must exist and deserialize into its target DTO. This is a structural
//! regression guard against fixture/DTO drift, not a behavioral test.

use axon_api::source::{
    MemoryCompactRequest, MemoryContextRequest, MemoryContradictRequest, MemoryRequest,
    MemorySearchRequest,
};

const FIXTURES_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures");

fn load(relative: &str) -> serde_json::Value {
    let path = format!("{FIXTURES_ROOT}/{relative}");
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

#[test]
fn remember_decision_fixture_deserializes_as_memory_request() {
    let value = load("remember/decision.valid.json");
    let request: MemoryRequest = serde_json::from_value(value).expect("valid MemoryRequest");
    assert_eq!(request.memory_type, axon_api::source::MemoryType::Decision);
}

#[test]
fn remember_preference_fixture_deserializes_as_memory_request() {
    let value = load("remember/preference.valid.json");
    let request: MemoryRequest = serde_json::from_value(value).expect("valid MemoryRequest");
    assert_eq!(
        request.memory_type,
        axon_api::source::MemoryType::Preference
    );
}

#[test]
fn search_query_fixture_deserializes_as_memory_search_request() {
    let value = load("search/query.valid.json");
    let request: MemorySearchRequest =
        serde_json::from_value(value).expect("valid MemorySearchRequest");
    assert!(!request.query.is_empty());
    assert!(request.reinforce);
}

#[test]
fn context_budget_fixture_deserializes_as_memory_context_request() {
    let value = load("context/budget.valid.json");
    let request: MemoryContextRequest =
        serde_json::from_value(value).expect("valid MemoryContextRequest");
    assert_eq!(request.token_budget, 2000);
}

#[test]
fn review_contradiction_fixture_deserializes_as_memory_contradict_request() {
    let value = load("review/contradiction.valid.json");
    let request: MemoryContradictRequest =
        serde_json::from_value(value).expect("valid MemoryContradictRequest");
    assert_ne!(request.memory_id, request.conflicting_id);
}

#[test]
fn compact_fixture_deserializes_as_memory_compact_request() {
    let value = load("compact/compact.valid.json");
    let request: MemoryCompactRequest =
        serde_json::from_value(value).expect("valid MemoryCompactRequest");
    assert_eq!(request.memory_ids.len(), 2);
    assert_eq!(request.strategy, "concatenate");
}
