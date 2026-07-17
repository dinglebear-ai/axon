use crate::types::ScreenshotResult;
use axon_api::source::{ArtifactId, Timestamp};

#[test]
fn screenshot_result_serializes_only_the_opaque_contract() {
    let value = serde_json::to_value(ScreenshotResult {
        artifact_id: ArtifactId::new("art_screenshot_123"),
        width: 1280,
        height: 720,
        captured_at: Timestamp("2026-07-16T00:00:00Z".to_string()),
        warnings: Vec::new(),
    })
    .expect("serialize screenshot result");

    let object = value.as_object().expect("object");
    assert_eq!(object.len(), 5);
    assert_eq!(value["artifact_id"], "art_screenshot_123");
    assert!(object.get("path").is_none());
    assert!(object.get("relative_path").is_none());
    assert!(object.get("display_path").is_none());
}
