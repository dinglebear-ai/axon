use super::*;
use serde_json::{Value, json};
use uuid::Uuid;

fn service_job(status: &str, progress_json: Option<Value>) -> ServiceJob {
    let now = chrono::Utc::now();
    ServiceJob {
        id: Uuid::from_u128(42),
        status: status.to_string(),
        phase: axon_api::source::PipelinePhase::Fetching,
        created_at: now,
        updated_at: now,
        started_at: None,
        finished_at: None,
        error_text: None,
        url: None,
        source_type: None,
        target: None,
        urls_json: None,
        progress_json,
        result_json: None,
        config_json: None,
        attempt_count: 1,
        active_attempt_id: None,
        last_reclaimed_at: None,
        last_reclaimed_reason: None,
    }
}

#[test]
fn source_progress_summary_uses_unified_stage_counts() {
    let job = service_job(
        "running",
        Some(json!({
            "items_total": 5,
            "items_done": 3,
            "documents_total": 5,
            "documents_done": 2,
            "chunks_total": 20,
            "chunks_done": 17
        })),
    );

    assert_eq!(
        source_progress_summary(&job).as_deref(),
        Some("2/5 docs · 40.0% · 17 chunks")
    );
}

#[test]
fn source_progress_summary_uses_item_counts_when_documents_are_pending() {
    let job = service_job(
        "running",
        Some(json!({
            "items_total": 5,
            "items_done": 3,
            "documents_total": 5,
            "documents_done": 0,
            "chunks_total": 0,
            "chunks_done": 0
        })),
    );

    assert_eq!(
        source_progress_summary(&job).as_deref(),
        Some("3/5 items · preparing")
    );
}

#[test]
fn source_progress_summary_renders_live_web_page_counts() {
    let mut job = service_job(
        "running",
        Some(json!({
            "items_total": 300,
            "items_done": 30,
            "documents_total": 300,
            "documents_done": 29,
            "chunks_done": 0
        })),
    );
    job.source_type = Some("web".to_string());

    assert_eq!(
        source_progress_summary(&job).as_deref(),
        Some("30/300 pages · 10.0%")
    );
}

#[test]
fn source_progress_summary_renders_legacy_discovered_page_total() {
    let job = service_job(
        "running",
        Some(json!({
            "pages_crawled": 30,
            "pages_discovered": 300,
            "md_created": 25,
            "error_pages": 2
        })),
    );

    assert_eq!(
        source_progress_summary(&job).as_deref(),
        Some("30/300 pages · 10.0% · 25 docs · 2 errors")
    );
}

#[test]
fn source_progress_summary_uses_the_shared_phase_before_counters_arrive() {
    let job = service_job("running", Some(json!({})));

    assert_eq!(source_progress_summary(&job).as_deref(), Some("fetching…"));
}

#[test]
fn source_progress_summary_renders_terminal_shared_counts() {
    let mut job = service_job("completed", None);
    job.result_json = Some(json!({
        "items_total": 12,
        "items_done": 12,
        "documents_total": 10,
        "documents_done": 10,
        "chunks_total": 84,
        "chunks_done": 84,
        "bytes_done": 0,
    }));

    assert_eq!(
        source_progress_summary(&job).as_deref(),
        Some("10/10 docs · 100% · 84 chunks")
    );
}

#[test]
fn source_progress_summary_renders_terminal_map_item_counts() {
    let mut job = service_job("completed", None);
    job.result_json = Some(json!({
        "items_total": 381,
        "items_done": 381,
        "documents_total": 0,
        "documents_done": 0,
        "chunks_total": 0,
        "chunks_done": 0,
    }));

    assert_eq!(
        source_progress_summary(&job).as_deref(),
        Some("381/381 items · 100%")
    );
}

#[test]
fn source_progress_summary_renders_degraded_terminal_counts() {
    let mut job = service_job("completed_degraded", None);
    job.result_json = Some(json!({
        "documents_total": 10,
        "documents_done": 9,
        "chunks_total": 84,
        "chunks_done": 80,
    }));

    assert_eq!(
        source_progress_summary(&job).as_deref(),
        Some("9/10 docs · 90.0% · 80 chunks")
    );
}
