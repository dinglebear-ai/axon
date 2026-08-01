use super::*;
use serde_json::{Value, json};
use uuid::Uuid;

fn service_job(status: &str, phase: PipelinePhase, progress_json: Option<Value>) -> ServiceJob {
    let now = chrono::Utc::now();
    ServiceJob {
        id: Uuid::from_u128(42),
        status: status.to_string(),
        phase,
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
        progress_json,
        result_json: None,
        config_json: None,
        attempt_count: 1,
        active_attempt_id: None,
        last_reclaimed_at: None,
        last_reclaimed_reason: None,
    }
}

fn unified_counts() -> Value {
    json!({
        "items_total": 5,
        "items_done": 3,
        "documents_total": 5,
        "documents_done": 2,
        "chunks_total": 20,
        "chunks_done": 17
    })
}

#[test]
fn source_progress_summary_renders_each_pipeline_coordinate_system() {
    let cases = [
        (PipelinePhase::Enriching, "3/5 items · 60.0%"),
        (PipelinePhase::Normalizing, "2/5 docs · 40.0%"),
        (PipelinePhase::Preparing, "2/5 docs · 40.0% · 17 chunks"),
        (PipelinePhase::Batching, "17/20 chunks · 85.0%"),
        (PipelinePhase::Embedding, "17/20 chunks · 85.0%"),
        (PipelinePhase::Vectorizing, "17/20 vectors · 85.0%"),
        (PipelinePhase::Upserting, "17/20 vectors · 85.0%"),
    ];

    for (phase, expected) in cases {
        let job = service_job("running", phase, Some(unified_counts()));
        assert_eq!(
            source_progress_summary(&job).as_deref(),
            Some(expected),
            "phase {phase:?}"
        );
    }
}

#[test]
fn fetching_uses_canonical_source_kind_units() {
    let cases = [
        (SourceKind::Web, "3/5 pages · 60.0%"),
        (SourceKind::Local, "3/5 files · 60.0%"),
        (SourceKind::Git, "3/5 files · 60.0%"),
        (SourceKind::Upload, "3/5 files · 60.0%"),
        (SourceKind::Registry, "3/5 versions · 60.0%"),
        (SourceKind::Feed, "3/5 entries · 60.0%"),
        (SourceKind::Reddit, "3/5 items · 60.0%"),
        (SourceKind::Youtube, "3/5 videos · 60.0%"),
        (SourceKind::Session, "3/5 transcripts · 60.0%"),
        (SourceKind::CliTool, "3/5 tool calls · 60.0%"),
        (SourceKind::McpTool, "3/5 tool calls · 60.0%"),
        (SourceKind::Memory, "3/5 memories · 60.0%"),
    ];

    for (source_kind, expected) in cases {
        let mut job = service_job("running", PipelinePhase::Fetching, Some(unified_counts()));
        job.source_kind = Some(source_kind);
        assert_eq!(
            source_progress_summary(&job).as_deref(),
            Some(expected),
            "source kind {source_kind:?}"
        );
    }
}

#[test]
fn fetching_without_source_kind_degrades_to_generic_items() {
    let job = service_job("running", PipelinePhase::Fetching, Some(unified_counts()));
    assert_eq!(
        source_progress_summary(&job).as_deref(),
        Some("3/5 items · 60.0%")
    );
}

#[test]
fn source_units_use_singular_labels() {
    let mut job = service_job(
        "running",
        PipelinePhase::Fetching,
        Some(json!({ "items_total": 1, "items_done": 1 })),
    );
    job.source_kind = Some(SourceKind::Web);

    assert_eq!(
        source_progress_summary(&job).as_deref(),
        Some("1/1 page · 100%")
    );
}

#[test]
fn unknown_total_renders_completed_count_without_percentage() {
    let job = service_job(
        "running",
        PipelinePhase::Embedding,
        Some(json!({ "chunks_done": 7 })),
    );

    assert_eq!(source_progress_summary(&job).as_deref(), Some("7 chunks"));
}

#[test]
fn zero_total_falls_back_to_the_active_phase() {
    let job = service_job(
        "running",
        PipelinePhase::Embedding,
        Some(json!({ "chunks_total": 0, "chunks_done": 0 })),
    );

    assert_eq!(source_progress_summary(&job).as_deref(), Some("embedding…"));
}

#[test]
fn progress_percentage_is_clamped_at_one_hundred() {
    let job = service_job(
        "running",
        PipelinePhase::Embedding,
        Some(json!({ "chunks_total": 5, "chunks_done": 99 })),
    );

    assert_eq!(
        source_progress_summary(&job).as_deref(),
        Some("99/5 chunks · 100%")
    );
}

#[test]
fn publishing_reports_generation_commit_state() {
    let running = service_job(
        "running",
        PipelinePhase::Publishing,
        Some(json!({ "items_total": 1, "items_done": 0 })),
    );
    let published = service_job(
        "running",
        PipelinePhase::Publishing,
        Some(json!({ "items_total": 1, "items_done": 1 })),
    );

    assert_eq!(
        source_progress_summary(&running).as_deref(),
        Some("committing generation")
    );
    assert_eq!(
        source_progress_summary(&published).as_deref(),
        Some("published generation")
    );
}

#[test]
fn source_progress_summary_renders_legacy_discovered_page_total() {
    let job = service_job(
        "running",
        PipelinePhase::Fetching,
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
    let job = service_job("running", PipelinePhase::Fetching, Some(json!({})));

    assert_eq!(source_progress_summary(&job).as_deref(), Some("fetching…"));
}

#[test]
fn source_progress_summary_renders_terminal_shared_counts() {
    let mut job = service_job("completed", PipelinePhase::Complete, None);
    job.result_json = Some(json!({
        "items_total": 12,
        "items_done": 12,
        "documents_total": 10,
        "documents_done": 10,
        "chunks_total": 84,
        "chunks_done": 84,
        "bytes_done": 0
    }));

    assert_eq!(
        source_progress_summary(&job).as_deref(),
        Some("10/10 docs · 100% · 84 chunks")
    );
}

#[test]
fn source_progress_summary_renders_terminal_map_item_counts() {
    let mut job = service_job("completed", PipelinePhase::Complete, None);
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
    let mut job = service_job("completed_degraded", PipelinePhase::Complete, None);
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
