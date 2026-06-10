use super::*;

#[test]
fn async_job_commands_are_identified_correctly() {
    for cmd in &["crawl", "embed", "extract", "ingest"] {
        assert!(
            is_async_job_command(cmd),
            "{cmd} should be an async job command"
        );
    }
    for cmd in &["ask", "query", "scrape", "search", "screenshot", "status"] {
        assert!(
            !is_async_job_command(cmd),
            "{cmd} should NOT be an async job command"
        );
    }
}

#[test]
fn terminal_statuses_are_identified_correctly() {
    for status in &["completed", "failed", "canceled", "cancelled"] {
        assert!(
            is_terminal_job_status(status),
            "{status} should be terminal"
        );
    }
    for status in &["pending", "running", "unknown", ""] {
        assert!(
            !is_terminal_job_status(status),
            "{status} should NOT be terminal"
        );
    }
}

#[test]
fn accepted_job_poll_path_extracts_status_url() {
    let json = r#"{"job_id": "abc123", "status_url": "/v1/status/abc123"}"#;
    let path = accepted_job_poll_path(json);
    assert_eq!(path, Some("/v1/status/abc123".to_string()));
}

#[test]
fn accepted_job_poll_path_returns_none_when_missing() {
    let json = r#"{"job_id": "abc123"}"#;
    assert_eq!(accepted_job_poll_path(json), None);
}

#[test]
fn accepted_job_poll_path_returns_none_on_invalid_json() {
    assert_eq!(accepted_job_poll_path("not json"), None);
    assert_eq!(accepted_job_poll_path(""), None);
}

#[test]
fn screenshot_artifact_path_extracts_relative_path() {
    let json = r#"{
        "url": "https://example.com",
        "artifact_handle": { "relative_path": "screenshots/example.png" }
    }"#;
    let path = screenshot_artifact_path(json);
    assert_eq!(
        path,
        Some("/v1/artifacts/screenshots/example.png".to_string())
    );
}

#[test]
fn screenshot_artifact_path_returns_none_when_absent() {
    let json = r#"{"url": "https://example.com"}"#;
    assert_eq!(screenshot_artifact_path(json), None);
}
