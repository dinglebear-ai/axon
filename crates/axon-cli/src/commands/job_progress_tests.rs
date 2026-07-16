use super::*;
use serde_json::{Value, json};
use uuid::Uuid;

fn service_job(status: &str, progress_json: Option<Value>) -> ServiceJob {
    let now = chrono::Utc::now();
    ServiceJob {
        id: Uuid::from_u128(42),
        status: status.to_string(),
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
