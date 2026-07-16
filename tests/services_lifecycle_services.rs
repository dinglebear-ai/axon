use axon_services::extract::{map_extract_job_result, map_extract_start_result};
use axon_services::screenshot::map_screenshot_result;

// --- extract mapping helpers ---

#[test]
fn maps_extract_start_result_with_job_id() {
    let job_id = "550e8400-e29b-41d4-a716-446655440002".to_string();
    let result = map_extract_start_result(job_id.clone());
    assert_eq!(result.job_id, job_id);
}

#[test]
fn maps_extract_job_result_preserves_payload() {
    let payload = serde_json::json!({
        "job_id": "xyz",
        "status": "pending",
        "urls": ["https://example.com"],
        "total_items": 0
    });
    let result = map_extract_job_result(payload.clone());
    assert_eq!(result.payload, payload);
}

// --- screenshot mapping helper ---

#[test]
fn maps_screenshot_result_to_typed_contract() {
    let payload = serde_json::json!({
        "url": "https://example.com",
        "path": "/output/screenshots/0001-example-com.png",
        "size_bytes": 98765
    });
    let result = map_screenshot_result(&payload).unwrap();
    assert_eq!(result.url, "https://example.com");
    assert_eq!(result.path, "/output/screenshots/0001-example-com.png");
    assert_eq!(result.size_bytes, 98765);
}

#[test]
fn maps_screenshot_result_rejects_raw_null_payload() {
    assert!(map_screenshot_result(&serde_json::Value::Null).is_err());
}

#[test]
fn maps_screenshot_result_rejects_missing_required_fields() {
    let payload = serde_json::json!({
        "url": "https://docs.rust-lang.org",
        "path": "/screenshots/0001-docs-rust-lang-org.png"
    });
    assert!(map_screenshot_result(&payload).is_err());
}

#[test]
fn maps_screenshot_result_fields() {
    let payload = serde_json::json!({
        "url": "https://docs.rust-lang.org",
        "path": "/screenshots/0001-docs-rust-lang-org.png",
        "size_bytes": 42000
    });
    let result = map_screenshot_result(&payload).unwrap();
    assert_eq!(result.url, "https://docs.rust-lang.org");
    assert_eq!(result.size_bytes, 42000);
}
