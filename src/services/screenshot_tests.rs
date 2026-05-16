use super::map_screenshot_result;

#[test]
fn map_screenshot_result_preserves_artifact_handle() {
    let result = map_screenshot_result(&serde_json::json!({
        "url": "https://example.com",
        "path": "/srv/axon/artifacts/screenshots/shot.png",
        "size_bytes": 42,
        "artifact_handle": {
            "kind": "screenshot",
            "relative_path": "screenshots/shot.png",
            "display_path": "/srv/axon/artifacts/screenshots/shot.png",
            "bytes": 42,
            "line_count": null,
            "job_id": null,
            "url": "https://example.com"
        }
    }))
    .expect("screenshot payload");

    let handle = result.artifact_handle.expect("artifact handle");
    assert_eq!(handle.relative_path, "screenshots/shot.png");
    assert_eq!(handle.kind, "screenshot");
    assert_eq!(handle.url.as_deref(), Some("https://example.com"));
}
