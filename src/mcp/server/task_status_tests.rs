use super::*;
use crate::jobs::backend::JobKind;
use crate::services::types::ServiceJob;
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
        result_json: Some(json!({"raw": "result"})),
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
fn task_result_payload_is_minimal_and_sanitized() {
    let payload = task_result_payload(JobKind::Ingest, &job("completed"));
    let value = serde_json::to_value(payload).unwrap();
    assert_eq!(value["kind"], "ingest");
    assert_eq!(value["completed"], true);
    assert!(value.get("result_json").is_none());
    assert!(value.get("config_json").is_none());
    assert!(value.get("error_text").is_none());
    assert!(value.get("target").is_none());
    assert!(value.get("url").is_none());
}
