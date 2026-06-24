use super::*;
use axon_jobs::backend::JobKind;
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
        assert_eq!(task_from_job(JobKind::Crawl, &job(axon)).status, expected);
    }
}

#[test]
fn task_objects_discourage_hot_polling() {
    let task = task_from_job(JobKind::Crawl, &job("running"));
    let poll_interval = task.poll_interval.expect("task should set poll interval");
    assert_eq!(poll_interval, TASK_POLL_INTERVAL_MS);
    assert!(poll_interval >= 5_000);
}

#[test]
fn task_result_payload_includes_sanitized_result_json() {
    let payload = task_result_payload(JobKind::Ingest, &job("completed"));
    let value = serde_json::to_value(payload).unwrap();
    assert_eq!(value["kind"], "ingest");
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

    let payload = task_result_payload(JobKind::Crawl, &job);
    let value = serde_json::to_value(payload).unwrap();

    assert_eq!(value["result_json"]["truncated"], true);
    assert_eq!(
        value["result_json"]["reason"],
        "result_json exceeded task payload size limit"
    );
}
