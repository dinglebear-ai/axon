use super::*;
use serde_json::json;

#[test]
fn source_request_default_limits_and_options_remain_backward_compatible() {
    let request: SourceRequest = serde_json::from_value(json!({"source": "repo.git"})).unwrap();
    assert_eq!(request.limits, SourceLimits::default());
    assert_eq!(request.options, AdapterOptions::default());
    assert_eq!(request.detached, None);
}

#[test]
fn source_request_zero_limits_are_preserved_in_serialization() {
    let value = json!({"source": "repo.git", "limits": {
        "max_items": 0, "max_bytes_per_item": 0, "max_total_bytes": 0
    }, "options": {"values": {"exclude_paths": ["vendor/"]}}});
    let request: SourceRequest = serde_json::from_value(value.clone()).unwrap();
    let serialized = serde_json::to_value(request).unwrap();
    assert_eq!(serialized["limits"], value["limits"]);
    assert_eq!(serialized["options"], value["options"]);
}
