use crate::contract::ArtifactHandle;

#[test]
fn artifact_handle_try_new_rejects_unsafe_relative_paths() {
    for path in [
        "../secret.txt",
        "screenshots/../secret.txt",
        "/var/tmp/secret.txt",
        "screenshots/%2e%2e/secret.txt",
        "screenshots\\secret.txt",
        "C:/secret.txt",
        "screenshots/shot.png\0",
    ] {
        assert!(
            ArtifactHandle::try_new("file", path, path, 0, None, None, None).is_err(),
            "{path} must be rejected"
        );
    }
}

#[test]
fn artifact_handle_deserialize_rejects_unsafe_relative_path() {
    let err = serde_json::from_value::<ArtifactHandle>(serde_json::json!({
        "kind": "file",
        "relative_path": "../secret.txt",
        "display_path": "../secret.txt",
        "bytes": 0,
        "line_count": null,
        "job_id": null,
        "url": null
    }))
    .expect_err("unsafe relative_path must not deserialize");

    assert!(err.to_string().contains("unsafe artifact relative_path"));
}

#[test]
fn artifact_handle_deserialize_accepts_safe_relative_path() {
    let handle = serde_json::from_value::<ArtifactHandle>(serde_json::json!({
        "kind": "file",
        "relative_path": "crawl/status.json",
        "display_path": "/srv/axon/artifacts/crawl/status.json",
        "bytes": 128,
        "line_count": 12,
        "job_id": "job-1",
        "url": "https://example.com"
    }))
    .expect("safe relative_path should deserialize");

    assert_eq!(handle.relative_path(), "crawl/status.json");
    assert_eq!(handle.bytes(), 128);
}
