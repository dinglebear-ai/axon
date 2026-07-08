use axon_api::source::{SourceId, Visibility};
use serde_json::json;

use crate::engine::search_filters;
use crate::plan::RetrievalPlan;

#[test]
fn retrieval_filters_exclude_unpublished_vectors_by_default() {
    let plan = RetrievalPlan {
        collection: "axon-test".to_string(),
        limit: 10,
        source_id: Some(SourceId::new("src_local_repo")),
        generation: None,
        allowed_visibility: vec![Visibility::Public],
        namespace_filters: Vec::new(),
        excluded_namespaces: Vec::new(),
        byte_budget: 4096,
        token_budget: 512,
    };

    let filters = search_filters(&plan).expect("generation-safe retrieval filters");

    assert_eq!(filters["source_id"], json!("src_local_repo"));
    assert!(filters.get("committed_generation").is_none());
    assert_eq!(filters["document_status"], json!("published"));
    assert_eq!(filters["visibility"], json!(["public"]));
    assert_eq!(filters["redaction_status"], json!("clean"));
}
