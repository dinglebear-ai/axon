use super::*;
use chrono::{TimeZone, Utc};

fn service_job() -> ServiceJob {
    ServiceJob {
        id: uuid::Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap(),
        status: "running".to_string(),
        created_at: Utc.with_ymd_and_hms(2026, 7, 16, 12, 0, 0).unwrap(),
        updated_at: Utc.with_ymd_and_hms(2026, 7, 16, 12, 1, 0).unwrap(),
        started_at: Some(Utc.with_ymd_and_hms(2026, 7, 16, 12, 0, 10).unwrap()),
        finished_at: None,
        error_text: None,
        url: Some("https://example.com/docs".to_string()),
        source_type: Some("web".to_string()),
        target: Some("https://example.com".to_string()),
        urls_json: None,
        progress_json: Some(serde_json::json!({"phase": "acquire", "items_done": 3})),
        result_json: Some(serde_json::json!({"documents": 2})),
        config_json: Some(serde_json::json!({"secret": "not-status-data"})),
        attempt_count: 2,
        active_attempt_id: Some("attempt_2".to_string()),
        last_reclaimed_at: None,
        last_reclaimed_reason: None,
    }
}

#[test]
fn status_job_serializes_canonical_fields_without_wire_aliases() {
    let value =
        serde_json::to_value(StatusJob::from_service_job(JobKind::Source, &service_job())).unwrap();

    assert_eq!(value["job_id"], "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa");
    assert_eq!(value["kind"], "source");
    assert_eq!(value["progress"]["phase"], "acquire");
    assert_eq!(value["result"]["documents"], 2);
    for removed in [
        "id",
        "error_text",
        "progress_json",
        "result_json",
        "config_json",
        "metrics",
    ] {
        assert!(value.get(removed).is_none(), "removed field {removed}");
    }
}
