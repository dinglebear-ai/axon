use super::{map_screenshot_result, screenshot_output_paths};
use axon_core::config::Config;

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
    assert_eq!(handle.relative_path(), "screenshots/shot.png");
    assert_eq!(handle.kind(), "screenshot");
    assert_eq!(handle.url(), Some("https://example.com"));
}

#[test]
fn screenshot_output_paths_preserve_explicit_output_outside_output_dir() {
    let output_root = tempfile::tempdir().expect("output root");
    let explicit_root = tempfile::tempdir().expect("explicit root");
    let explicit = explicit_root.path().join("shot.png");
    let cfg = Config {
        output_dir: output_root.path().to_path_buf(),
        output_path: Some(explicit.clone()),
        ..Config::default()
    };

    let (path, default_relative) = screenshot_output_paths(&cfg, "https://example.com");

    assert_eq!(path, explicit);
    assert!(default_relative.starts_with("screenshots"));
}

#[test]
fn screenshot_output_paths_default_to_managed_output_dir() {
    let output_root = tempfile::tempdir().expect("output root");
    let cfg = Config {
        output_dir: output_root.path().to_path_buf(),
        output_path: None,
        ..Config::default()
    };

    let (path, default_relative) = screenshot_output_paths(&cfg, "https://example.com");

    assert_eq!(path, output_root.path().join(&default_relative));
    assert!(default_relative.starts_with("screenshots"));
}
