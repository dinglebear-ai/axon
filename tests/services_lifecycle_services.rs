use axon_services::extract::{map_extract_job_result, map_extract_start_result};

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

// The screenshot mapping tests that used to live here covered
// `map_screenshot_result`, which was removed with the opaque artifact-ID
// migration — `screenshot_capture` now returns a typed `ScreenshotResult`
// directly and no raw-JSON path mapper exists to test.
