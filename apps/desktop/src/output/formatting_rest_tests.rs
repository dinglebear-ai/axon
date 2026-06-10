use super::*;

#[test]
fn screenshot_with_artifact_handle_formats_cleanly() {
    let json = r#"{
        "url": "https://example.com",
        "artifact_handle": {
            "relative_path": "screenshots/example.com-2024.png",
            "bytes": 153600
        }
    }"#;
    let result = rest_output_text("screenshot", json).unwrap();
    assert!(result.contains("https://example.com"), "should show URL");
    assert!(
        result.contains("150.0 KB"),
        "should show human-readable size"
    );
    assert!(
        result.contains("artifact: screenshots/example.com-2024.png"),
        "should show relative path"
    );
    assert!(
        !result.contains("size_bytes"),
        "must not leak raw JSON keys"
    );
    assert!(
        !result.contains("/home/"),
        "must not contain absolute server paths"
    );
}

#[test]
fn screenshot_without_artifact_handle_falls_back_to_size_bytes() {
    let json = r#"{"url": "https://example.com", "size_bytes": 2048}"#;
    let result = rest_output_text("screenshot", json).unwrap();
    assert!(result.contains("https://example.com"));
    assert!(result.contains("2.0 KB"));
}

#[test]
fn screenshot_without_artifact_handle_shows_no_raw_path_lines() {
    let json = r#"{
        "url": "https://example.com",
        "path": "/home/axon/.axon/screenshots/example.png",
        "size_bytes": 512
    }"#;
    let result = rest_output_text("screenshot", json).unwrap();
    assert!(
        !result.contains("/home/axon"),
        "absolute server path must not appear in output"
    );
    assert!(result.contains("https://example.com"));
}

#[test]
fn format_bytes_below_1kb() {
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(1023), "1023 B");
}

#[test]
fn format_bytes_kb_range() {
    assert_eq!(format_bytes(1024), "1.0 KB");
    assert_eq!(format_bytes(153_600), "150.0 KB");
}

#[test]
fn format_bytes_mb_range() {
    assert_eq!(format_bytes(1_048_576), "1.0 MB");
    assert_eq!(format_bytes(5_242_880), "5.0 MB");
}

#[test]
fn job_terminal_result_completed_crawl_shows_metrics() {
    let json = r#"{
        "job": {
            "status": "completed",
            "url": "https://docs.example.com",
            "result_json": {
                "pages_crawled": 42,
                "docs_embedded": 38,
                "chunks_embedded": 512,
                "elapsed_ms": 14500
            }
        }
    }"#;
    let result = rest_output_text("crawl", json).unwrap();
    assert!(
        result.contains("crawl completed"),
        "should show subcommand + status"
    );
    assert!(
        result.contains("pages_crawled: 42"),
        "should show pages_crawled"
    );
    assert!(
        result.contains("docs_embedded: 38"),
        "should show docs_embedded"
    );
    assert!(
        result.contains("chunks_embedded: 512"),
        "should show chunks_embedded"
    );
    assert!(
        result.contains("elapsed: 14.5s"),
        "should show elapsed in seconds"
    );
    assert!(
        result.contains("https://docs.example.com"),
        "should show target URL"
    );
}

#[test]
fn job_terminal_result_failed_shows_error() {
    let json = r#"{
        "job": {
            "status": "failed",
            "url": "https://example.com",
            "error_text": "connection refused"
        }
    }"#;
    let result = rest_output_text("crawl", json).unwrap();
    assert!(result.contains("crawl failed"), "should show failed status");
    assert!(
        result.contains("error: connection refused"),
        "should show error text"
    );
}

#[test]
fn job_terminal_result_zero_metrics_are_omitted() {
    let json = r#"{
        "job": {
            "status": "completed",
            "result_json": {
                "pages_crawled": 0,
                "docs_embedded": 0,
                "elapsed_ms": 500
            }
        }
    }"#;
    let result = rest_output_text("embed", json).unwrap();
    // Zero-value metrics should not appear
    assert!(
        !result.contains("pages_crawled"),
        "zero pages_crawled must be omitted"
    );
    assert!(
        !result.contains("docs_embedded"),
        "zero docs_embedded must be omitted"
    );
    // elapsed_ms < 1000 should also be omitted
    assert!(
        !result.contains("elapsed"),
        "sub-second elapsed must be omitted"
    );
}

#[test]
fn job_terminal_result_ingest_with_target_field() {
    let json = r#"{
        "job": {
            "status": "completed",
            "target": "owner/repo",
            "result_json": { "docs_embedded": 100, "elapsed_ms": 5000 }
        }
    }"#;
    let result = rest_output_text("ingest", json).unwrap();
    assert!(result.contains("ingest completed"));
    assert!(result.contains("docs_embedded: 100"));
    assert!(result.contains("elapsed: 5.0s"));
    assert!(result.contains("owner/repo"), "should show target field");
}

#[test]
fn job_start_result_accepted_still_shows_job_id() {
    // When there is no "job" key, accepted-job path renders job_id + "Next: status"
    let json = r#"{
        "job_id": "crawl-abc",
        "status": "accepted",
        "disposition": "queued"
    }"#;
    let result = rest_output_text("crawl", json).unwrap();
    assert!(result.contains("job: crawl-abc"), "should show job id");
    assert!(
        result.contains("Next: status"),
        "should suggest status command"
    );
}
