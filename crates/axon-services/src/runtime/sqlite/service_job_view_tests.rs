use super::*;
use axon_api::source::{
    JobId, JobPriority, JobSummary, LifecycleStatus, PipelinePhase, ProgressCurrent, StageCounts,
    Timestamp,
};
use serde_json::json;

fn completed_summary() -> JobSummary {
    let now = Timestamp::from(chrono::Utc::now());
    JobSummary {
        job_id: JobId::new(uuid::Uuid::from_u128(42)),
        kind: JobKind::Source,
        status: LifecycleStatus::Completed,
        phase: PipelinePhase::Complete,
        created_at: now.clone(),
        updated_at: now.clone(),
        source_id: None,
        watch_id: None,
        intent: None,
        started_at: Some(now.clone()),
        finished_at: Some(now),
        parent_job_id: None,
        root_job_id: None,
        attempt: 1,
        priority: JobPriority::Normal,
        counts: Some(StageCounts {
            items_total: Some(12),
            items_done: 12,
            documents_total: Some(10),
            documents_done: 10,
            chunks_total: Some(84),
            chunks_done: 84,
            bytes_total: None,
            bytes_done: 0,
        }),
        current: None,
        heartbeat: None,
        last_error: None,
        warnings: Vec::new(),
    }
}

#[test]
fn terminal_job_counts_are_the_shared_result_projection() {
    let job = summary_to_service_job(completed_summary(), None);

    assert!(job.progress_json.is_none());
    assert_eq!(
        job.result_json,
        Some(json!({
            "items_total": 12,
            "items_done": 12,
            "documents_total": 10,
            "documents_done": 10,
            "chunks_total": 84,
            "chunks_done": 84,
            "bytes_done": 0,
        }))
    );
}

#[test]
fn running_job_projects_adapter_into_source_type() {
    let mut summary = completed_summary();
    summary.status = LifecycleStatus::Running;
    summary.phase = PipelinePhase::Fetching;
    summary.finished_at = None;
    summary.current = Some(ProgressCurrent {
        source_item_key: None,
        document_id: None,
        chunk_id: None,
        adapter: Some("web".to_string()),
        provider: None,
        message: Some("30/300 pages fetched".to_string()),
    });

    let job = summary_to_service_job(
        summary,
        Some(json!({ "source_request": { "source": "https://example.com/docs" } })),
    );

    assert_eq!(job.source_type.as_deref(), Some("web"));
    assert!(job.progress_json.is_some());
}

#[test]
fn nested_source_request_shape_extracts_url() {
    let req = json!({ "source_request": { "source": "https://www.reddit.com/r/rust/" } });
    let (url, source_type, target, urls) = request_target_fields(JobKind::Source, Some(&req));
    assert_eq!(url.as_deref(), Some("https://www.reddit.com/r/rust/"));
    assert_eq!(target.as_deref(), Some("https://www.reddit.com/r/rust/"));
    assert!(source_type.is_none());
    assert!(urls.is_none());
}

#[test]
fn flat_source_shape_extracts_url() {
    // Legacy `{"scope","source","source_kind"}` shape — the source lives at the
    // top level. Regression guard for the `axon status` `[REDACTED]` bug where
    // this fell through to `job.id` and the UUID tripped the secret redactor.
    let req = json!({
        "scope": "page",
        "source": "https://news.ycombinator.com/item?id=1",
        "source_kind": "web"
    });
    let (url, source_type, target, urls) = request_target_fields(JobKind::Source, Some(&req));
    assert_eq!(
        url.as_deref(),
        Some("https://news.ycombinator.com/item?id=1")
    );
    assert_eq!(
        target.as_deref(),
        Some("https://news.ycombinator.com/item?id=1")
    );
    assert!(source_type.is_none());
    assert!(urls.is_none());
}

#[test]
fn nested_shape_takes_precedence_over_flat_source() {
    // If both are somehow present, the canonical nested shape wins.
    let req = json!({
        "source": "https://flat.example/legacy",
        "source_request": { "source": "https://nested.example/canonical" }
    });
    let (url, _, target, _) = request_target_fields(JobKind::Source, Some(&req));
    assert_eq!(url.as_deref(), Some("https://nested.example/canonical"));
    assert_eq!(target.as_deref(), Some("https://nested.example/canonical"));
}

#[test]
fn no_source_anywhere_yields_none() {
    let req = json!({ "scope": "page", "source_kind": "web" });
    let (url, source_type, target, urls) = request_target_fields(JobKind::Source, Some(&req));
    assert!(url.is_none());
    assert!(source_type.is_none());
    assert!(target.is_none());
    assert!(urls.is_none());
}

#[test]
fn missing_request_json_yields_all_none() {
    let (url, source_type, target, urls) = request_target_fields(JobKind::Source, None);
    assert!(url.is_none() && source_type.is_none() && target.is_none() && urls.is_none());
}
