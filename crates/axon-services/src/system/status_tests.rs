use super::*;
use chrono::{TimeZone, Utc};

fn job(id: &str, status: &str, progress: serde_json::Value) -> ServiceJob {
    ServiceJob {
        id: uuid::Uuid::parse_str(id).unwrap(),
        status: status.to_string(),
        created_at: Utc.with_ymd_and_hms(2026, 7, 16, 12, 0, 0).unwrap(),
        updated_at: Utc.with_ymd_and_hms(2026, 7, 16, 12, 1, 0).unwrap(),
        started_at: None,
        finished_at: None,
        error_text: None,
        url: Some("https://example.com".to_string()),
        source_type: Some("web".to_string()),
        target: None,
        urls_json: None,
        progress_json: Some(progress),
        result_json: Some(serde_json::json!({"documents": 4})),
        config_json: None,
        attempt_count: 1,
        active_attempt_id: None,
        last_reclaimed_at: None,
        last_reclaimed_reason: None,
    }
}

#[test]
fn status_payload_uses_unified_typed_job_collections() {
    let source = job(
        "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa",
        "running",
        serde_json::json!({"phase": "acquire"}),
    );
    let extract = job(
        "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
        "completed",
        serde_json::json!({"phase": "complete"}),
    );
    let watch = job(
        "cccccccc-cccc-cccc-cccc-cccccccccccc",
        "queued",
        serde_json::json!({}),
    );
    let prune = job(
        "dddddddd-dddd-dddd-dddd-dddddddddddd",
        "running",
        serde_json::json!({"phase": "cleanup"}),
    );
    let value = build_status_payload_with_errors(
        &[source],
        &[extract],
        &[watch],
        &[prune],
        &StatusTotals {
            source: 1,
            extract: 1,
            watch: 1,
            prune: 1,
        },
        &["provider cooling".to_string()],
    );

    assert_eq!(value["jobs"].as_array().unwrap().len(), 2);
    assert_eq!(value["jobs"][0]["kind"], "source");
    assert_eq!(value["jobs"][1]["kind"], "extract");
    assert_eq!(value["watches"][0]["kind"], "watch");
    assert_eq!(value["cleanup"]["jobs"][0]["kind"], "prune");
    assert_eq!(value["warnings"][0], "provider cooling");
    assert_eq!(value["jobs"][0]["progress"]["phase"], "acquire");
    assert_eq!(value["jobs"][0]["result"]["documents"], 4);
}

#[test]
fn status_payload_omits_legacy_arrays_and_aliases() {
    let value = build_status_payload(&[], &[], &[], &[], &StatusTotals::default());
    for removed in [
        "source_jobs",
        "extract_jobs",
        "watch_jobs",
        "prune_jobs",
        "errors",
        "metrics",
        "result_json",
    ] {
        assert!(value.get(removed).is_none(), "removed field {removed}");
    }
}
