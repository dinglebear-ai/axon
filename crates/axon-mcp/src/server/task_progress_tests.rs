use super::*;
use axon_api::{
    job_status::JobStatus,
    source::{JobKind, PipelinePhase},
};
use axon_services::types::ServiceJob;
use rmcp::model::{NumberOrString, ProgressToken};
use serde_json::json;
use uuid::Uuid;

fn service_job(status: &str) -> ServiceJob {
    let now = chrono::Utc::now();
    ServiceJob {
        id: Uuid::new_v4(),
        status: status.to_string(),
        phase: PipelinePhase::Fetching,
        created_at: now,
        updated_at: now,
        started_at: None,
        finished_at: None,
        error_text: None,
        url: None,
        source_type: None,
        source_kind: None,
        target: None,
        urls_json: None,
        progress_json: Some(json!({"items_done": 1, "items_total": 4})),
        result_json: None,
        config_json: None,
        attempt_count: 1,
        active_attempt_id: None,
        last_reclaimed_at: None,
        last_reclaimed_reason: None,
    }
}

#[test]
fn initial_task_progress_is_ready_before_create_task_returns() {
    let (notification, fingerprint, is_active) = initial_progress_notification(
        JobKind::Source,
        &service_job("running"),
        ProgressToken(NumberOrString::Number(7)),
    );

    assert_eq!(notification.progress, 1.0);
    assert_eq!(notification.total, Some(4.0));
    assert_eq!(notification.message.as_deref(), Some("indexing"));
    assert!(is_active);
    assert!(!fingerprint.is_empty());
}

#[test]
fn maps_source_page_progress_without_leaking_paths() {
    let value = json!({
        "output_dir": "/secret/path",
        "output_path": "/secret/path/markdown",
        "pages_crawled": 4,
        "pages_discovered": 10,
        "message": "raw worker message"
    });
    let progress = map_job_progress(
        JobKind::Source,
        &JobStatus::Running,
        PipelinePhase::Fetching,
        Some(&value),
    );
    assert_eq!(progress.progress, 4.0);
    assert_eq!(progress.total, Some(10.0));
    assert_eq!(progress.message, "indexing");
}

#[test]
fn maps_source_document_progress_with_real_total() {
    let value = json!({"docs_embedded": 2, "docs_total": 5, "chunks_embedded": 50});
    let progress = map_job_progress(
        JobKind::Source,
        &JobStatus::Running,
        PipelinePhase::Embedding,
        Some(&value),
    );
    assert_eq!(progress.progress, 2.0);
    assert_eq!(progress.total, Some(5.0));
    assert_eq!(progress.message, "embedding");
}

#[test]
fn maps_source_unified_stage_counts_with_real_total() {
    let value = json!({
        "items_total": 5,
        "items_done": 3,
        "documents_total": 5,
        "documents_done": 2,
        "chunks_total": 20,
        "chunks_done": 17
    });
    let progress = map_job_progress(
        JobKind::Source,
        &JobStatus::Running,
        PipelinePhase::Embedding,
        Some(&value),
    );
    assert_eq!(progress.progress, 17.0);
    assert_eq!(progress.total, Some(20.0));
    assert_eq!(progress.message, "embedding");
}

#[test]
fn maps_source_fetching_counts_by_items() {
    let value = json!({
        "items_total": 5,
        "items_done": 3,
        "documents_total": 5,
        "documents_done": 2,
        "chunks_total": 20,
        "chunks_done": 17
    });
    let progress = map_job_progress(
        JobKind::Source,
        &JobStatus::Running,
        PipelinePhase::Fetching,
        Some(&value),
    );
    assert_eq!(progress.progress, 3.0);
    assert_eq!(progress.total, Some(5.0));
    assert_eq!(progress.message, "fetching");
}

#[test]
fn maps_source_provider_progress_with_allowlisted_message() {
    let value = json!({
        "phase": "cloning",
        "repo": "https://token@example.com/private/repo",
        "files_done": 7,
        "files_total": 9
    });
    let progress = map_job_progress(
        JobKind::Source,
        &JobStatus::Running,
        PipelinePhase::Fetching,
        Some(&value),
    );
    assert_eq!(progress.progress, 7.0);
    assert_eq!(progress.total, Some(9.0));
    assert_eq!(progress.message, "indexing");
}

#[test]
fn extract_running_progress_uses_unknown_total() {
    let progress = map_job_progress(
        JobKind::Extract,
        &JobStatus::Running,
        PipelinePhase::Fetching,
        None,
    );
    assert_eq!(progress.progress, 0.0);
    assert_eq!(progress.total, None);
    assert_eq!(progress.message, "running");
}

#[test]
fn active_progress_prefers_progress_json_over_legacy_result_json() {
    let progress_json = json!({"pages_crawled": 4, "pages_discovered": 10});
    let result_json = json!({"pages_crawled": 99, "pages_discovered": 100});

    let selected = progress_metrics_for_status(
        &JobStatus::Running,
        Some(&progress_json),
        Some(&result_json),
    );
    let progress = map_job_progress(
        JobKind::Source,
        &JobStatus::Running,
        PipelinePhase::Fetching,
        selected,
    );

    assert_eq!(progress.progress, 4.0);
    assert_eq!(progress.total, Some(10.0));
}

#[test]
fn terminal_progress_uses_final_result_json() {
    let progress_json = json!({"pages_crawled": 4, "pages_discovered": 10});
    let result_json = json!({"pages_crawled": 99, "pages_discovered": 100});

    let selected = progress_metrics_for_status(
        &JobStatus::Completed,
        Some(&progress_json),
        Some(&result_json),
    );

    assert_eq!(selected, Some(&result_json));
}

#[test]
fn active_progress_ignores_degraded_progress_json_marker() {
    let progress_json = json!({
        "degraded": true,
        "field": "progress_json",
        "error": "corrupt job JSON"
    });
    let result_json = json!({"pages_crawled": 4, "pages_discovered": 10});

    let selected = progress_metrics_for_status(
        &JobStatus::Running,
        Some(&progress_json),
        Some(&result_json),
    );
    let progress = map_job_progress(
        JobKind::Source,
        &JobStatus::Running,
        PipelinePhase::Fetching,
        selected,
    );

    assert_eq!(progress.progress, 4.0);
    assert_eq!(progress.total, Some(10.0));
}

#[test]
fn structured_source_progress_normalizes_flat_counts_and_event_diagnostics() {
    let stored = json!({
        "items_total": 6,
        "items_done": 4,
        "current": { "adapter": "github" },
        "warning": {
            "code": "source.partial",
            "severity": "warning",
            "message": "partial result",
            "retryable": true
        },
        "error": {
            "code": "source.item_failed",
            "message": "one item failed"
        }
    });

    let progress = structured_source_progress(Some(&stored)).expect("structured progress");
    assert_eq!(progress["counts"]["items_total"], 6);
    assert_eq!(progress["counts"]["items_done"], 4);
    assert_eq!(progress["current"]["adapter"], "github");
    assert_eq!(progress["warnings"][0]["code"], "source.partial");
    assert_eq!(progress["errors"][0]["code"], "source.item_failed");
}
