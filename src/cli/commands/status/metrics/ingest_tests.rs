use super::*;

fn strip_ansi(s: &str) -> String {
    console::strip_ansi_codes(s).into_owned()
}

#[test]
fn ingest_suffix_shows_phase_and_tasks_done() {
    let result = serde_json::json!({
        "files_done": 150, "files_total": 155, "chunks_embedded": 1700,
        "tasks_done": 3, "tasks_total": 5,
        "phase": "fetching_issues", "issues_fetched": 42, "issues_page": 2,
    });
    let suffix = strip_ansi(&ingest_metrics_suffix("running", Some(&result)));
    assert!(
        suffix.contains("fetching_issues"),
        "should show phase: {suffix}"
    );
    assert!(
        suffix.contains("3/5"),
        "should show task progress: {suffix}"
    );
}

#[test]
fn ingest_suffix_shows_embedding_issues_phase() {
    let result = serde_json::json!({
        "tasks_done": 2, "tasks_total": 5,
        "phase": "embedding_issues", "issues_total": 100, "chunks_embedded": 2400,
    });
    let suffix = strip_ansi(&ingest_metrics_suffix("running", Some(&result)));
    assert!(
        suffix.contains("embedding_issues"),
        "should show phase: {suffix}"
    );
}

#[test]
fn ingest_suffix_backward_compatible_files_only() {
    let result = serde_json::json!({
        "files_done": 10, "files_total": 20, "chunks_embedded": 500,
    });
    let suffix = strip_ansi(&ingest_metrics_suffix("running", Some(&result)));
    assert!(
        suffix.contains("10/20"),
        "should show file progress: {suffix}"
    );
    assert!(suffix.contains("500"), "should show chunk count: {suffix}");
}

#[test]
fn ingest_suffix_phase_specific_detail_fetching_issues() {
    let result = serde_json::json!({
        "tasks_done": 1, "tasks_total": 4,
        "phase": "fetching_issues", "issues_fetched": 42, "issues_page": 2,
    });
    let suffix = strip_ansi(&ingest_metrics_suffix("running", Some(&result)));
    assert!(
        suffix.contains("42"),
        "should show issues fetched: {suffix}"
    );
    assert!(
        suffix.contains("page 2"),
        "should show page number: {suffix}"
    );
}
