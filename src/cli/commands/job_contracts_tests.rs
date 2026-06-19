use super::*;
use crate::jobs::crawl::CrawlJob;
use crate::jobs::embed::EmbedJob;
use crate::jobs::extract::ExtractJob;
use crate::jobs::ingest::IngestJob;
use crate::services::types::ServiceJob;
use chrono::{TimeZone, Utc};
use uuid::Uuid;

fn test_ts() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 2, 24, 12, 0, 0)
        .single()
        .expect("valid timestamp")
}

fn test_crawl_job() -> CrawlJob {
    CrawlJob {
        id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").expect("valid uuid"),
        url: "https://example.com".to_string(),
        status: "running".to_string(),
        created_at: test_ts(),
        updated_at: test_ts(),
        started_at: Some(test_ts()),
        finished_at: None,
        error_text: None,
        result_json: Some(serde_json::json!({"pages_crawled": 3})),
        config_json: serde_json::json!({}),
    }
}

fn test_extract_job() -> ExtractJob {
    ExtractJob {
        id: Uuid::parse_str("22222222-2222-2222-2222-222222222222").expect("valid uuid"),
        status: "pending".to_string(),
        created_at: test_ts(),
        updated_at: test_ts(),
        started_at: None,
        finished_at: None,
        error_text: Some("boom".to_string()),
        urls_json: serde_json::json!(["https://example.com"]),
        result_json: Some(serde_json::json!({"items": 2})),
    }
}

fn test_ingest_job() -> IngestJob {
    IngestJob {
        id: Uuid::parse_str("33333333-3333-3333-3333-333333333333").expect("valid uuid"),
        status: "completed".to_string(),
        source_type: "github".to_string(),
        target: "owner/repo".to_string(),
        created_at: test_ts(),
        updated_at: test_ts(),
        started_at: Some(test_ts()),
        finished_at: Some(test_ts()),
        error_text: None,
        result_json: Some(serde_json::json!({"chunks": 99})),
        config_json: serde_json::json!({"collection": "cortex"}),
    }
}

fn test_embed_job() -> EmbedJob {
    EmbedJob {
        id: Uuid::parse_str("44444444-4444-4444-4444-444444444444").expect("valid uuid"),
        status: "running".to_string(),
        created_at: test_ts(),
        updated_at: test_ts(),
        started_at: Some(test_ts()),
        finished_at: None,
        error_text: None,
        input_text: "/tmp/embed-input".to_string(),
        result_json: Some(serde_json::json!({"chunks_embedded": 7, "source": "rust"})),
        config_json: serde_json::json!({"collection": "cortex"}),
    }
}

fn serialize_list(entries: Vec<JobSummaryEntry>) -> serde_json::Value {
    let serialized = serde_json::to_string(&entries).expect("serialize");
    serde_json::from_str(&serialized).expect("parse")
}

fn test_service_job(status: &str) -> ServiceJob {
    ServiceJob {
        id: Uuid::parse_str("55555555-5555-5555-5555-555555555555").expect("valid uuid"),
        status: status.to_string(),
        created_at: test_ts(),
        updated_at: test_ts(),
        started_at: Some(test_ts()),
        finished_at: None,
        error_text: None,
        url: Some("https://example.com".to_string()),
        source_type: None,
        target: None,
        urls_json: None,
        progress_json: None,
        result_json: None,
        config_json: None,
        attempt_count: 1,
        active_attempt_id: Some("attempt-1".to_string()),
        last_reclaimed_at: None,
        last_reclaimed_reason: None,
    }
}

#[test]
fn crawl_status_includes_shared_and_legacy_metric_fields() {
    let json =
        serde_json::to_value(JobStatusResponse::from_crawl(&test_crawl_job())).expect("serialize");
    assert_eq!(json["url"], "https://example.com");
    assert_eq!(json["status"], "running");
    assert_eq!(json["metrics"], serde_json::json!({"pages_crawled": 3}));
    assert_eq!(json["result_json"], serde_json::json!({"pages_crawled": 3}));
    assert_eq!(json["error"], serde_json::Value::Null);
    assert_eq!(json["error_text"], serde_json::Value::Null);
}

#[test]
fn extract_status_includes_auditable_urls_aliases() {
    let json = serde_json::to_value(JobStatusResponse::from_extract(&test_extract_job()))
        .expect("serialize");
    let expected_urls = serde_json::json!(["https://example.com"]);
    assert_eq!(json["urls"], expected_urls);
    assert_eq!(json["urls_json"], expected_urls);
    assert_eq!(json["metrics"], serde_json::json!({"items": 2}));
    assert_eq!(json["result_json"], serde_json::json!({"items": 2}));
    assert_eq!(json["error"], "boom");
    assert_eq!(json["error_text"], "boom");
}

#[test]
fn ingest_status_includes_shared_and_legacy_config_fields() {
    let json = serde_json::to_value(JobStatusResponse::from_ingest(&test_ingest_job()))
        .expect("serialize");
    assert_eq!(json["source_type"], "github");
    assert_eq!(json["target"], "owner/repo");
    assert_eq!(json["metrics"], serde_json::json!({"chunks": 99}));
    assert_eq!(json["result_json"], serde_json::json!({"chunks": 99}));
    assert_eq!(
        json["config_json"],
        serde_json::json!({"collection": "cortex"})
    );
}

#[test]
fn list_payload_serialization_keeps_crawl_metrics() {
    let payload = serialize_list(vec![JobSummaryEntry::from_crawl(&test_crawl_job())]);
    let item = &payload[0];
    assert_eq!(item["url"], "https://example.com");
    assert_eq!(item["metrics"], serde_json::json!({"pages_crawled": 3}));
    assert_eq!(item["result_json"], serde_json::json!({"pages_crawled": 3}));
}

#[test]
fn list_payload_serialization_keeps_extract_urls_and_metrics() {
    let payload = serialize_list(vec![JobSummaryEntry::from_extract(&test_extract_job())]);
    let item = &payload[0];
    let expected_urls = serde_json::json!(["https://example.com"]);
    assert_eq!(item["urls"], expected_urls);
    assert_eq!(item["urls_json"], expected_urls);
    assert_eq!(item["metrics"], serde_json::json!({"items": 2}));
    assert_eq!(item["result_json"], serde_json::json!({"items": 2}));
}

#[test]
fn list_payload_serialization_keeps_ingest_source_target_and_config() {
    let payload = serialize_list(vec![JobSummaryEntry::from_ingest(&test_ingest_job())]);
    let item = &payload[0];
    assert_eq!(item["source_type"], "github");
    assert_eq!(item["target"], "owner/repo");
    assert_eq!(item["metrics"], serde_json::json!({"chunks": 99}));
    assert_eq!(item["result_json"], serde_json::json!({"chunks": 99}));
    assert_eq!(
        item["config_json"],
        serde_json::json!({"collection": "cortex"})
    );
}

#[test]
fn embed_status_contract_includes_input_and_metrics() {
    let json =
        serde_json::to_value(JobStatusResponse::from_embed(&test_embed_job())).expect("serialize");
    assert_eq!(json["status"], "running");
    assert_eq!(json["target"], "/tmp/embed-input");
    assert_eq!(json["collection"], "cortex");
    assert_eq!(json["source"], "rust");
    assert_eq!(
        json["metrics"],
        serde_json::json!({"chunks_embedded": 7, "source": "rust"})
    );
    assert_eq!(
        json["result_json"],
        serde_json::json!({"chunks_embedded": 7, "source": "rust"})
    );
}

#[test]
fn service_running_metrics_alias_uses_progress_json() {
    let mut job = test_service_job("running");
    job.progress_json = Some(serde_json::json!({
        "phase": "crawling",
        "lifecycle_progress": 0.42,
        "pages_crawled": 42
    }));
    let json = serde_json::to_value(JobStatusResponse::from_service_job(&job)).expect("serialize");

    assert_eq!(
        json["metrics"],
        serde_json::json!({
            "phase": "crawling",
            "lifecycle_progress": 0.42,
            "pages_crawled": 42
        })
    );
    assert_eq!(json["result_json"], json["metrics"]);
    assert_eq!(json["progress_json"], json["metrics"]);
}

#[test]
fn service_wire_json_keeps_active_result_json_compat_alias() {
    let mut job = test_service_job("running");
    job.progress_json = Some(serde_json::json!({
        "phase": "crawling",
        "lifecycle_progress": 0.42,
        "pages_crawled": 42
    }));
    let json = job.wire_json_compat();

    assert_eq!(
        json["metrics"],
        serde_json::json!({
            "phase": "crawling",
            "lifecycle_progress": 0.42,
            "pages_crawled": 42
        })
    );
    assert_eq!(json["metrics"], json["result_json"]);
    assert_eq!(json["metrics"], json["progress_json"]);
}

#[test]
fn service_running_alias_overwrites_stale_result_json() {
    let mut job = test_service_job("running");
    job.progress_json = Some(serde_json::json!({
        "phase": "crawling",
        "lifecycle_progress": 0.42,
        "pages_crawled": 42
    }));
    job.result_json = Some(serde_json::json!({
        "phase": "stale",
        "pages_crawled": 99
    }));

    let status_json =
        serde_json::to_value(JobStatusResponse::from_service_job(&job)).expect("serialize");
    assert_eq!(status_json["result_json"], status_json["metrics"]);
    assert_eq!(status_json["metrics"]["pages_crawled"], 42);

    let wire_json = job.wire_json_compat();
    assert_eq!(wire_json["result_json"], wire_json["metrics"]);
    assert_eq!(wire_json["metrics"]["pages_crawled"], 42);
}

#[test]
fn service_running_alias_ignores_degraded_progress_marker() {
    let mut job = test_service_job("running");
    job.progress_json = Some(serde_json::json!({
        "degraded": true,
        "field": "progress_json",
        "error": "corrupt job JSON"
    }));
    job.result_json = Some(serde_json::json!({
        "phase": "legacy",
        "pages_crawled": 5
    }));

    let status_json =
        serde_json::to_value(JobStatusResponse::from_service_job(&job)).expect("serialize");
    assert_eq!(status_json["metrics"]["pages_crawled"], 5);
    assert_eq!(status_json["result_json"], status_json["metrics"]);

    let wire_json = job.wire_json_compat();
    assert_eq!(wire_json["metrics"]["pages_crawled"], 5);
    assert_eq!(wire_json["result_json"], wire_json["metrics"]);
}

#[test]
fn service_completed_metrics_alias_uses_result_json() {
    let mut job = test_service_job("completed");
    job.finished_at = Some(test_ts());
    job.active_attempt_id = None;
    job.progress_json = Some(serde_json::json!({
        "phase": "completed",
        "lifecycle_progress": 1.0
    }));
    job.result_json = Some(serde_json::json!({
        "coverage_status": "partial",
        "pages_crawled": 42
    }));
    let json = serde_json::to_value(JobStatusResponse::from_service_job(&job)).expect("serialize");

    assert_eq!(
        json["metrics"],
        serde_json::json!({"coverage_status": "partial", "pages_crawled": 42})
    );
    assert_eq!(
        json["progress_json"],
        serde_json::json!({"phase": "completed", "lifecycle_progress": 1.0})
    );
}

#[test]
fn service_wire_json_keeps_terminal_result_json_final() {
    let mut job = test_service_job("completed");
    job.finished_at = Some(test_ts());
    job.active_attempt_id = None;
    job.progress_json = Some(serde_json::json!({
        "phase": "completed",
        "lifecycle_progress": 1.0
    }));
    job.result_json = Some(serde_json::json!({
        "coverage_status": "partial",
        "pages_crawled": 42
    }));
    let json = job.wire_json_compat();

    assert_eq!(
        json["metrics"],
        serde_json::json!({"coverage_status": "partial", "pages_crawled": 42})
    );
    assert_eq!(
        json["result_json"],
        serde_json::json!({"coverage_status": "partial", "pages_crawled": 42})
    );
    assert_eq!(
        json["progress_json"],
        serde_json::json!({"phase": "completed", "lifecycle_progress": 1.0})
    );
}

#[test]
fn cancel_and_errors_contracts_stay_stable() {
    let errors = serde_json::to_value(JobErrorsResponse::from_job(
        Uuid::nil(),
        "failed".to_string(),
        Some("boom".to_string()),
    ))
    .expect("serialize");
    assert_eq!(errors["id"], Uuid::nil().to_string());
    assert_eq!(errors["status"], "failed");
    assert_eq!(errors["error"], "boom");

    let cancel =
        serde_json::to_value(JobCancelResponse::new(Uuid::nil(), true)).expect("serialize");
    assert_eq!(cancel["id"], Uuid::nil().to_string());
    assert_eq!(cancel["canceled"], true);
    assert_eq!(cancel["source"], "rust");
}
