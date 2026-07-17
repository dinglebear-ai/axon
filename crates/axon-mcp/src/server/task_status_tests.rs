use super::*;
use axon_api::source::JobKind;
use axon_services::types::ServiceJob;
use chrono::Utc;
use rmcp::model::TaskStatus;
use serde_json::json;
use uuid::Uuid;

fn job(status: &str) -> ServiceJob {
    let now = Utc::now();
    ServiceJob {
        id: Uuid::new_v4(),
        status: status.to_string(),
        created_at: now,
        updated_at: now,
        started_at: None,
        finished_at: None,
        error_text: Some("raw error must not be exposed".to_string()),
        url: Some("https://example.com/private".to_string()),
        source_type: Some("github".to_string()),
        target: Some("secret-target".to_string()),
        urls_json: Some(json!(["https://example.com/private"])),
        progress_json: None,
        result_json: Some(json!({
            "raw": "result",
            "access_token": "secret-token",
            "repo": "https://user:secret@example.com/private/repo",
        })),
        config_json: Some(json!({"token": "secret"})),
        attempt_count: 1,
        active_attempt_id: Some("attempt-1".to_string()),
        last_reclaimed_at: None,
        last_reclaimed_reason: None,
    }
}

#[test]
fn maps_axon_job_statuses_to_rmcp_task_statuses() {
    let cases = [
        ("pending", TaskStatus::Working),
        ("running", TaskStatus::Working),
        ("completed", TaskStatus::Completed),
        ("failed", TaskStatus::Failed),
        ("canceled", TaskStatus::Cancelled),
    ];
    for (axon, expected) in cases {
        assert_eq!(task_from_job(JobKind::Source, &job(axon)).status, expected);
    }
}

#[test]
fn task_objects_discourage_hot_polling() {
    let task = task_from_job(JobKind::Source, &job("running"));
    let poll_interval = task.poll_interval.expect("task should set poll interval");
    assert_eq!(poll_interval, TASK_POLL_INTERVAL_MS);
    assert!(poll_interval >= 5_000);
}

#[test]
fn task_result_payload_includes_sanitized_result_json() {
    let payload = task_result_payload(JobKind::Source, &job("completed"));
    let value = serde_json::to_value(payload).unwrap();
    assert_eq!(value["kind"], "source");
    assert_eq!(value["completed"], true);
    assert_eq!(value["result_json"]["raw"], "result");
    assert_eq!(value["result_json"]["access_token"], "[redacted]");
    assert_eq!(value["result_json"]["repo"], "[redacted-url]");
    assert!(value.get("config_json").is_none());
    assert!(value.get("error_text").is_none());
    assert!(value.get("target").is_none());
    assert!(value.get("url").is_none());
}

#[test]
fn task_result_payload_truncates_oversized_result_json() {
    let mut job = job("completed");
    job.result_json = Some(json!({
        "chunks": (0..80).map(|_| "x".repeat(1024)).collect::<Vec<_>>()
    }));

    let payload = task_result_payload(JobKind::Source, &job);
    let value = serde_json::to_value(payload).unwrap();

    assert_eq!(value["result_json"]["truncated"], true);
    assert_eq!(
        value["result_json"]["reason"],
        "result_json exceeded task payload size limit"
    );
}

#[test]
fn source_task_result_preserves_structured_progress_without_leaking_sensitive_values() {
    let mut job = job("completed");
    job.progress_json = Some(json!({
        "counts": {
            "items_total": 8,
            "items_done": 5,
            "documents_total": 5,
            "documents_done": 4,
            "chunks_total": 20,
            "chunks_done": 17,
            "bytes_done": 4096
        },
        "current": {
            "source_item_key": "/home/jmagar/private/source.md",
            "adapter": "local",
            "message": "processing /home/jmagar/private/source.md"
        },
        "warnings": [{
            "code": "source.partial",
            "severity": "warning",
            "message": "TOKEN=top-secret-value",
            "source_item_key": "/home/jmagar/private/source.md",
            "retryable": true
        }],
        "errors": [{
            "code": "source.item_failed",
            "severity": "error",
            "message": "failed /home/jmagar/private/source.md",
            "source_item_key": "/home/jmagar/private/source.md",
            "retryable": false,
            "cause": "Authorization:Bearer secret-value"
        }]
    }));

    let payload = task_result_payload(JobKind::Source, &job);
    let value = serde_json::to_value(payload).unwrap();
    let progress = &value["progress"];

    assert_eq!(progress["counts"]["items_done"], 5);
    assert_eq!(progress["counts"]["chunks_done"], 17);
    assert_eq!(progress["current"]["adapter"], "local");
    assert_eq!(progress["warnings"][0]["code"], "source.partial");
    assert_eq!(progress["warnings"][0]["retryable"], true);
    assert_eq!(progress["errors"][0]["code"], "source.item_failed");
    assert_eq!(progress["errors"][0]["retryable"], false);

    let meta = task_meta_from_job(JobKind::Source, &job).expect("source task metadata");
    let meta = serde_json::to_value(meta).unwrap();
    assert_eq!(meta["axon"]["progress"], *progress);

    let encoded = serde_json::to_string(progress).unwrap();
    assert!(!encoded.contains("/home/jmagar"));
    assert!(!encoded.contains("top-secret-value"));
    assert!(!encoded.contains("secret-value"));
}
