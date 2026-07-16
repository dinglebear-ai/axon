use axon_cli::commands::monitor::{JobMonitorState, detect_job_events};
use axon_services::types::ServiceJob;
use chrono::{TimeZone, Utc};
use serde_json::json;
use uuid::Uuid;

fn job(id: Uuid, status: &str, target: &str, result_json: serde_json::Value) -> ServiceJob {
    ServiceJob {
        id,
        status: status.to_string(),
        created_at: Utc.with_ymd_and_hms(2026, 5, 24, 12, 0, 0).unwrap(),
        updated_at: Utc.with_ymd_and_hms(2026, 5, 24, 12, 1, 0).unwrap(),
        started_at: Some(Utc.with_ymd_and_hms(2026, 5, 24, 12, 0, 10).unwrap()),
        finished_at: None,
        error_text: None,
        url: Some(target.to_string()),
        source_type: None,
        target: Some(target.to_string()),
        urls_json: None,
        progress_json: None,
        result_json: Some(result_json),
        config_json: None,
        attempt_count: 1,
        active_attempt_id: None,
        last_reclaimed_at: None,
        last_reclaimed_reason: None,
    }
}

#[test]
fn detects_started_completed_and_failed_job_events() {
    let running_source = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap();
    let completed_extract = Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").unwrap();
    let failed_source = Uuid::parse_str("cccccccc-cccc-cccc-cccc-cccccccccccc").unwrap();

    let mut state = JobMonitorState::default();
    state.remember("extract", completed_extract, "running");
    state.remember("source", failed_source, "running");

    let mut failed = job(failed_source, "failed", "github:owner/repo", json!({}));
    failed.url = None;
    failed.source_type = Some("github".to_string());
    failed.error_text = Some("rate limited".to_string());

    let events = detect_job_events(
        &mut state,
        &[
            (
                "source",
                job(
                    running_source,
                    "running",
                    "https://docs.example.com",
                    json!({}),
                ),
            ),
            ("source", failed),
        ],
        &[(
            "extract",
            job(
                completed_extract,
                "completed",
                "https://docs.example.com",
                json!({"documents": 141, "chunks_embedded": 3834}),
            ),
        )],
    );

    assert_eq!(events.len(), 3);
    assert_eq!(events[0].event, "started");
    assert_eq!(events[0].kind, "source");
    assert_eq!(
        events[0].target.as_deref(),
        Some("https://docs.example.com")
    );

    assert_eq!(events[1].event, "failed");
    assert_eq!(events[1].kind, "source");
    assert_eq!(events[1].target.as_deref(), Some("github:owner/repo"));
    assert_eq!(events[1].error.as_deref(), Some("rate limited"));

    assert_eq!(events[2].event, "completed");
    assert_eq!(events[2].kind, "extract");
    assert_eq!(events[2].docs, Some(141));
    assert_eq!(events[2].chunks, Some(3834));

    assert_eq!(state.status_of("source", running_source), Some("running"));
    assert_eq!(
        state.status_of("extract", completed_extract),
        Some("completed")
    );
    assert_eq!(state.status_of("source", failed_source), Some("failed"));
}

#[test]
fn emits_terminal_event_for_new_job_after_baseline_even_when_running_was_missed() {
    let completed_source = Uuid::parse_str("dddddddd-dddd-dddd-dddd-dddddddddddd").unwrap();
    let mut state = JobMonitorState::default();

    let baseline = detect_job_events(&mut state, &[], &[]);
    assert!(baseline.is_empty());

    let events = detect_job_events(
        &mut state,
        &[(
            "source",
            job(
                completed_source,
                "completed",
                "https://example.com",
                json!({"pages_crawled": 1, "embed_job_id": null}),
            ),
        )],
        &[],
    );

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event, "completed");
    assert_eq!(events[0].kind, "source");
    assert_eq!(events[0].docs, Some(1));
}

#[test]
fn emits_terminal_event_on_first_poll_when_job_started_after_monitor() {
    let completed_source = Uuid::parse_str("eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee").unwrap();
    let mut state = JobMonitorState::default();
    state.mark_monitor_started_at(Utc.with_ymd_and_hms(2026, 5, 24, 11, 59, 0).unwrap());

    let events = detect_job_events(
        &mut state,
        &[(
            "source",
            job(
                completed_source,
                "completed",
                "https://example.com",
                json!({"pages_crawled": 1}),
            ),
        )],
        &[],
    );

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event, "completed");
    assert_eq!(events[0].id, completed_source);
}

#[test]
fn reports_canceled_jobs_separately_from_failed_jobs() {
    let canceled_source = Uuid::parse_str("ffffffff-ffff-ffff-ffff-ffffffffffff").unwrap();
    let mut state = JobMonitorState::default();
    state.remember("source", canceled_source, "running");

    let events = detect_job_events(
        &mut state,
        &[(
            "source",
            job(
                canceled_source,
                "canceled",
                "https://docs.example.com",
                json!({}),
            ),
        )],
        &[],
    );

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event, "canceled");
    assert_eq!(events[0].status, "canceled");
    assert_eq!(state.status_of("source", canceled_source), Some("canceled"));
}
