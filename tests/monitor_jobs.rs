use axon::cli::commands::monitor::{JobMonitorState, detect_job_events};
use axon::services::types::ServiceJob;
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
    let running_crawl = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap();
    let completed_embed = Uuid::parse_str("bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb").unwrap();
    let failed_ingest = Uuid::parse_str("cccccccc-cccc-cccc-cccc-cccccccccccc").unwrap();

    let mut state = JobMonitorState::default();
    state.remember("embed", completed_embed, "running");
    state.remember("ingest", failed_ingest, "running");

    let mut failed = job(failed_ingest, "failed", "github:owner/repo", json!({}));
    failed.url = None;
    failed.source_type = Some("github".to_string());
    failed.error_text = Some("rate limited".to_string());

    let events = detect_job_events(
        &mut state,
        &[(
            "crawl",
            job(
                running_crawl,
                "running",
                "https://docs.example.com",
                json!({}),
            ),
        )],
        &[(
            "embed",
            job(
                completed_embed,
                "completed",
                "https://docs.example.com",
                json!({"docs_embedded": 141, "chunks_embedded": 3834}),
            ),
        )],
        &[("ingest", failed)],
    );

    assert_eq!(events.len(), 3);
    assert_eq!(events[0].event, "started");
    assert_eq!(events[0].kind, "crawl");
    assert_eq!(
        events[0].target.as_deref(),
        Some("https://docs.example.com")
    );

    assert_eq!(events[1].event, "completed");
    assert_eq!(events[1].kind, "embed");
    assert_eq!(events[1].docs, Some(141));
    assert_eq!(events[1].chunks, Some(3834));

    assert_eq!(events[2].event, "failed");
    assert_eq!(events[2].kind, "ingest");
    assert_eq!(events[2].target.as_deref(), Some("github:owner/repo"));
    assert_eq!(events[2].error.as_deref(), Some("rate limited"));

    assert_eq!(state.status_of("crawl", running_crawl), Some("running"));
    assert_eq!(state.status_of("embed", completed_embed), Some("completed"));
    assert_eq!(state.status_of("ingest", failed_ingest), Some("failed"));
}

#[test]
fn emits_terminal_event_for_new_job_after_baseline_even_when_running_was_missed() {
    let completed_crawl = Uuid::parse_str("dddddddd-dddd-dddd-dddd-dddddddddddd").unwrap();
    let mut state = JobMonitorState::default();

    let baseline = detect_job_events(&mut state, &[], &[], &[]);
    assert!(baseline.is_empty());

    let events = detect_job_events(
        &mut state,
        &[(
            "crawl",
            job(
                completed_crawl,
                "completed",
                "https://example.com",
                json!({"pages_crawled": 1, "embed_job_id": null}),
            ),
        )],
        &[],
        &[],
    );

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event, "completed");
    assert_eq!(events[0].kind, "crawl");
    assert_eq!(events[0].docs, Some(1));
}

#[test]
fn emits_terminal_event_on_first_poll_when_job_started_after_monitor() {
    let completed_crawl = Uuid::parse_str("eeeeeeee-eeee-eeee-eeee-eeeeeeeeeeee").unwrap();
    let mut state = JobMonitorState::default();
    state.mark_monitor_started_at(Utc.with_ymd_and_hms(2026, 5, 24, 11, 59, 0).unwrap());

    let events = detect_job_events(
        &mut state,
        &[(
            "crawl",
            job(
                completed_crawl,
                "completed",
                "https://example.com",
                json!({"pages_crawled": 1}),
            ),
        )],
        &[],
        &[],
    );

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event, "completed");
    assert_eq!(events[0].id, completed_crawl);
}

#[test]
fn reports_canceled_jobs_separately_from_failed_jobs() {
    let canceled_embed = Uuid::parse_str("ffffffff-ffff-ffff-ffff-ffffffffffff").unwrap();
    let mut state = JobMonitorState::default();
    state.remember("embed", canceled_embed, "running");

    let events = detect_job_events(
        &mut state,
        &[],
        &[(
            "embed",
            job(
                canceled_embed,
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
    assert_eq!(state.status_of("embed", canceled_embed), Some("canceled"));
}
