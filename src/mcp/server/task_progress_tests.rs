use super::*;
use crate::jobs::backend::JobKind;
use crate::jobs::status::JobStatus;
use serde_json::json;

#[test]
fn maps_crawl_progress_without_leaking_paths() {
    let value = json!({
        "output_dir": "/secret/path",
        "output_path": "/secret/path/markdown",
        "pages_crawled": 4,
        "pages_discovered": 10,
        "message": "raw worker message"
    });
    let progress = map_job_progress(JobKind::Crawl, &JobStatus::Running, Some(&value));
    assert_eq!(progress.progress, 4.0);
    assert_eq!(progress.total, Some(10.0));
    assert_eq!(progress.message, "crawling");
}

#[test]
fn maps_embed_progress_with_real_total() {
    let value = json!({"docs_embedded": 2, "docs_total": 5, "chunks_embedded": 50});
    let progress = map_job_progress(JobKind::Embed, &JobStatus::Running, Some(&value));
    assert_eq!(progress.progress, 2.0);
    assert_eq!(progress.total, Some(5.0));
    assert_eq!(progress.message, "embedding");
}

#[test]
fn maps_ingest_progress_with_allowlisted_message() {
    let value = json!({
        "phase": "cloning",
        "repo": "https://token@example.com/private/repo",
        "files_done": 7,
        "files_total": 9
    });
    let progress = map_job_progress(JobKind::Ingest, &JobStatus::Running, Some(&value));
    assert_eq!(progress.progress, 7.0);
    assert_eq!(progress.total, Some(9.0));
    assert_eq!(progress.message, "ingesting");
}

#[test]
fn extract_running_progress_uses_unknown_total() {
    let progress = map_job_progress(JobKind::Extract, &JobStatus::Running, None);
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
    let progress = map_job_progress(JobKind::Crawl, &JobStatus::Running, selected);

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
    let progress = map_job_progress(JobKind::Crawl, &JobStatus::Running, selected);

    assert_eq!(progress.progress, 4.0);
    assert_eq!(progress.total, Some(10.0));
}
