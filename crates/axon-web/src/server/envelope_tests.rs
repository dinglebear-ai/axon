use super::*;

#[test]
fn ok_wraps_data_with_success_envelope_defaults() {
    let Json(envelope) = ok(serde_json::json!({ "hits": [] }));
    assert!(envelope.ok);
    assert_eq!(envelope.contract_version, CONTRACT_VERSION);
    assert_eq!(envelope.data, serde_json::json!({ "hits": [] }));
    assert!(envelope.warnings.is_empty());
    assert!(envelope.request_id.starts_with("req_"));
    assert!(envelope.trace.trace_id.starts_with("trace_"));
    assert!(envelope.pagination.is_none());
    assert!(envelope.job.is_none());
    assert!(envelope.artifacts.is_empty());
}

#[test]
fn ok_serializes_ok_field_true() {
    let Json(envelope) = ok(42u32);
    let value = serde_json::to_value(&envelope).expect("serialize envelope");
    assert_eq!(value["ok"], serde_json::json!(true));
    assert_eq!(value["data"], serde_json::json!(42));
}
