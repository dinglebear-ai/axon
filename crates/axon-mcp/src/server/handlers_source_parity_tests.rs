//! Shared REST/MCP input and detached persistence parity.
use super::*;
use axon_api::source::{LifecycleStatus, SourceResult};
use axon_jobs::boundary::{FakeJobWatchStore, JobStore};
use serde_json::json;

fn input() -> serde_json::Value {
    json!({
        "source": "https://github.com/example/project.git",
        "limits": {"max_items": 3, "max_pages": 4, "max_depth": 2,
            "max_bytes_per_item": 1024, "max_total_bytes": 4096,
            "max_chunks": 8, "provider_timeout_ms": 500},
        "options": {"values": {"exclude_paths": ["vendor/", "assets/"]}}
    })
}

#[test]
fn mcp_source_mapping_matches_rest_dto_and_preserves_defaults() {
    for value in [input(), json!({"source": "https://example.test"})] {
        let mcp: SourceRequest = serde_json::from_value(value.clone()).unwrap();
        let rest: ApiSourceRequest = serde_json::from_value(value).unwrap();
        let mapped = canonical_source_request(&mcp, mcp.source.clone().unwrap()).unwrap();
        assert_eq!(mapped, rest);
    }
}

#[tokio::test]
async fn detached_mcp_request_preserves_limits_options_and_result_counts() {
    let mut value = input();
    value["detached"] = json!(true);
    let mcp: SourceRequest = serde_json::from_value(value).unwrap();
    let mapped = canonical_source_request(&mcp, mcp.source.clone().unwrap()).unwrap();
    let store = FakeJobWatchStore::new();
    let result = axon_services::source::enqueue::enqueue_source_with_access_policy(
        mapped.clone(),
        &store,
        None,
        None,
        false,
    )
    .await
    .unwrap();
    assert_eq!(result.status, LifecycleStatus::Queued);
    let descriptor = result.job.as_ref().expect("detached poll descriptor");
    let queued = store
        .request_json(descriptor.job_id)
        .await
        .unwrap()
        .unwrap();
    let restored: ApiSourceRequest =
        serde_json::from_value(queued["source_request"].clone()).unwrap();
    assert_eq!(restored, mapped);
    // Both transports serialize the same SourceResult contract, including new counts.
    let mcp_payload = source_result_payload(&result);
    let rest_payload = serde_json::to_value(&result).unwrap();
    assert_eq!(mcp_payload, rest_payload);
    assert_eq!(mcp_payload["counts"]["documents_skipped"], 0);
    let roundtrip: SourceResult = serde_json::from_value(mcp_payload).unwrap();
    assert_eq!(roundtrip.counts.documents_skipped, 0);
    assert!(roundtrip.job.is_some());
}

#[test]
fn mcp_source_limits_reject_invalid_values_and_unknown_fields() {
    for limits in [
        json!({"max_items": -1}),
        json!({"max_total_bytes": "huge"}),
        json!({"unknown": 1}),
    ] {
        let mut value = input();
        value["limits"] = limits;
        assert!(serde_json::from_value::<SourceRequest>(value).is_err());
    }
}
