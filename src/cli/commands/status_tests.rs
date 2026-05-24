use super::*;
use crate::services::system::{StatusJobs, build_status_payload};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

fn strip_ansi(s: &str) -> String {
    console::strip_ansi_codes(s).into_owned()
}

fn job(status: &str) -> ServiceJob {
    ServiceJob {
        id: Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
        status: status.to_string(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        started_at: None,
        finished_at: None,
        error_text: None,
        url: Some("https://example.com/docs".to_string()),
        source_type: None,
        target: Some("https://example.com/docs".to_string()),
        urls_json: None,
        result_json: Some(json!({
            "pages_crawled": 3,
            "md_created": 2,
            "elapsed_ms": 1200,
            "docs_embedded": 2,
            "docs_total": 2,
            "chunks_embedded": 8
        })),
        config_json: None,
        attempt_count: 0,
        active_attempt_id: None,
        last_reclaimed_at: None,
        last_reclaimed_reason: None,
    }
}

#[test]
fn render_status_payload_matches_local_renderer() {
    let jobs = StatusJobs {
        crawl: vec![job("completed")],
        extract: Vec::new(),
        embed: vec![job("completed")],
        ingest: Vec::new(),
    };
    let payload = build_status_payload(
        &jobs.crawl,
        &jobs.extract,
        &jobs.embed,
        &jobs.ingest,
        &crate::services::types::StatusTotals::default(),
    );

    let totals = crate::services::types::StatusTotals::default();
    let from_jobs = render_status_jobs(&jobs, totals.crawl);
    let from_payload = render_status_payload(&payload).expect("payload should render");

    assert_eq!(from_payload, from_jobs);
    assert!(from_payload.contains("Crawl"));
    assert!(from_payload.contains("Embed"));
    assert!(from_payload.contains("2 docs"));
}

#[test]
fn render_status_payload_mentions_when_crawl_rows_are_truncated() {
    let payload = build_status_payload(
        &[job("running"), job("pending")],
        &[],
        &[],
        &[],
        &crate::services::types::StatusTotals {
            crawl: 24,
            ..Default::default()
        },
    );

    let rendered = render_status_payload(&payload).expect("payload should render");

    assert!(
        rendered.contains("showing 2 of 24"),
        "expected truncation note; got:\n{rendered}"
    );
    assert!(
        rendered.contains("running jobs listed first"),
        "expected ordering note; got:\n{rendered}"
    );
}

#[test]
fn render_status_payload_surfaces_reclaimed_pending_crawl_rows() {
    let mut reclaimed = job("pending");
    reclaimed.error_text = Some(RECLAIMED_ERROR_TEXT.to_string());
    reclaimed.result_json = None;

    let payload = build_status_payload(
        &[reclaimed],
        &[],
        &[],
        &[],
        &crate::services::types::StatusTotals::default(),
    );

    let rendered = render_status_payload(&payload).expect("payload should render");

    assert!(
        rendered.contains("recovered after worker shutdown"),
        "expected reclaim hint; got:\n{rendered}"
    );
    assert!(
        !rendered.contains(RECLAIMED_ERROR_TEXT),
        "raw reclaim marker leaked into output:\n{rendered}"
    );
}

#[test]
fn render_status_payload_surfaces_reclaimed_running_crawl_rows() {
    let mut reclaimed = job("running");
    reclaimed.error_text = Some(RECLAIMED_ERROR_TEXT.to_string());
    reclaimed.result_json = Some(json!({
        "pages_crawled": 42,
        "md_created": 30,
        "error_pages": 2
    }));

    let payload = build_status_payload(
        &[reclaimed],
        &[],
        &[],
        &[],
        &crate::services::types::StatusTotals::default(),
    );

    let rendered = render_status_payload(&payload).expect("payload should render");

    assert!(
        rendered.contains("reclaimed retry"),
        "expected reclaimed retry suffix; got:\n{rendered}"
    );
    assert!(
        rendered.contains("2 errors"),
        "expected crawl error count; got:\n{rendered}"
    );
    assert!(
        rendered.contains("processing resumed"),
        "expected reclaim hint; got:\n{rendered}"
    );
}

#[test]
fn render_status_payload_truncates_long_labels_and_errors() {
    let mut long = job("failed");
    long.url = Some(format!("https://example.com/{}", "x".repeat(240)));
    long.error_text = Some("error: ".to_string() + &"y".repeat(240));

    let payload = build_status_payload(
        &[long],
        &[],
        &[],
        &[],
        &crate::services::types::StatusTotals::default(),
    );

    let rendered = render_status_payload(&payload).expect("payload should render");

    assert!(
        !rendered.contains(&"x".repeat(180)),
        "long URL label leaked without truncation:\n{rendered}"
    );
    assert!(
        !rendered.contains(&"y".repeat(180)),
        "long error leaked without truncation:\n{rendered}"
    );
    assert!(
        rendered.contains('…'),
        "expected truncation marker:\n{rendered}"
    );
    let visible = strip_ansi(&rendered);
    assert!(
        visible
            .lines()
            .all(|line| line.chars().count() <= STATUS_TEXT_DISPLAY_LIMIT),
        "status output exceeded display cap:\n{rendered}"
    );
}

#[test]
fn render_status_payload_keeps_normal_rows_with_progress_under_display_cap() {
    let mut crawl = job("completed");
    crawl.url = Some(format!("https://example.com/{}", "long-path/".repeat(30)));
    crawl.result_json = Some(json!({
        "md_created": 222347,
        "elapsed_ms": 375100,
        "embed_job_id": "11111111-1111-1111-1111-111111111111"
    }));
    let mut embed = job("completed");
    embed.result_json = Some(json!({
        "docs_embedded": 222347,
        "docs_total": 222347,
        "chunks_embedded": 631679
    }));

    let payload = build_status_payload(
        &[crawl],
        &[],
        &[embed],
        &[],
        &crate::services::types::StatusTotals::default(),
    );

    let rendered = render_status_payload(&payload).expect("payload should render");

    let visible = strip_ansi(&rendered);
    assert!(
        visible
            .lines()
            .all(|line| line.chars().count() <= STATUS_TEXT_DISPLAY_LIMIT),
        "status output exceeded display cap:\n{rendered}"
    );
}
