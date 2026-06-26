use super::*;
use serde_json::json;

fn derive(family: JobFamily, status: &str, result: Value) -> JobProgress {
    JobProgress::derive(family, status, Some(&result), None)
}

#[test]
fn pending_is_indeterminate() {
    let p = derive(JobFamily::Ingest, "pending", json!({}));
    assert_eq!(p.phase, JobPhase::Pending);
    assert_eq!(p.percent, None);
}

#[test]
fn completed_is_done_at_100() {
    let p = derive(
        JobFamily::Embed,
        "completed",
        json!({ "docs_embedded": 3, "chunks_embedded": 42 }),
    );
    assert_eq!(p.phase, JobPhase::Done);
    assert_eq!(p.percent, Some(100.0));
    assert_eq!(
        p.metrics,
        vec![
            JobMetric {
                label: "Docs".into(),
                value: "3".into()
            },
            JobMetric {
                label: "Chunks".into(),
                value: "42".into()
            },
        ]
    );
}

#[test]
fn ingest_percent_from_task_counts() {
    let p = derive(
        JobFamily::Ingest,
        "running",
        json!({ "phase": "ingesting", "tasks_done": 2, "tasks_total": 5 }),
    );
    assert_eq!(p.phase, JobPhase::Running);
    assert_eq!(p.percent, Some(40.0));
    assert_eq!(
        p.metrics[0],
        JobMetric {
            label: "Phase".into(),
            value: "ingesting".into()
        }
    );
}

#[test]
fn running_without_task_counts_is_indeterminate() {
    let p = derive(
        JobFamily::Extract,
        "running",
        json!({ "pages_visited": 4, "total_items": 9 }),
    );
    assert_eq!(p.percent, None);
    assert_eq!(
        p.metrics,
        vec![
            JobMetric {
                label: "Pages".into(),
                value: "4".into()
            },
            JobMetric {
                label: "Items".into(),
                value: "9".into()
            },
        ]
    );
}

#[test]
fn failed_surfaces_error_text() {
    let p = JobProgress::derive(
        JobFamily::Ingest,
        "failed",
        Some(&json!({})),
        Some("github_repo target not found: owner/typo"),
    );
    assert_eq!(p.phase, JobPhase::Failed);
    assert!(p.error.as_deref().unwrap().contains("not found"));
    assert!(p.phase.is_terminal());
}

#[test]
fn thousands_separator_formatting() {
    let p = derive(
        JobFamily::Embed,
        "completed",
        json!({ "docs_embedded": 1, "chunks_embedded": 1234567 }),
    );
    assert_eq!(
        p.metrics[1],
        JobMetric {
            label: "Chunks".into(),
            value: "1,234,567".into()
        }
    );
}

#[test]
fn fmt_int_boundaries() {
    assert_eq!(fmt_int(0), "0");
    assert_eq!(fmt_int(100), "100");
    assert_eq!(fmt_int(1000), "1,000");
    assert_eq!(fmt_int(-1234), "-1,234");
}

#[test]
fn ingest_files_metric_sums_both_branches() {
    // Both present → summed.
    let p = derive(
        JobFamily::Ingest,
        "running",
        json!({ "files_ast_chunked": 7, "files_prose_fallback": 3 }),
    );
    let files = p.metrics.iter().find(|m| m.label == "Files").unwrap();
    assert_eq!(files.value, "10");

    // Only one present → the absent one counts as 0, metric still emitted.
    let p = derive(
        JobFamily::Ingest,
        "running",
        json!({ "files_ast_chunked": 4 }),
    );
    let files = p.metrics.iter().find(|m| m.label == "Files").unwrap();
    assert_eq!(files.value, "4");

    // Neither present → no Files metric at all.
    let p = derive(JobFamily::Ingest, "running", json!({ "phase": "cloning" }));
    assert!(p.metrics.iter().all(|m| m.label != "Files"));
}

#[test]
fn ingest_chunks_falls_back_to_chunks_key() {
    // Prefer chunks_embedded when present.
    let p = derive(
        JobFamily::Ingest,
        "running",
        json!({ "chunks_embedded": 11, "chunks": 99 }),
    );
    let chunks = p.metrics.iter().find(|m| m.label == "Chunks").unwrap();
    assert_eq!(chunks.value, "11");

    // Fall back to bare `chunks` when chunks_embedded is absent.
    let p = derive(JobFamily::Ingest, "running", json!({ "chunks": 5 }));
    let chunks = p.metrics.iter().find(|m| m.label == "Chunks").unwrap();
    assert_eq!(chunks.value, "5");
}

#[test]
fn ingest_percent_clamps_and_guards_zero_total() {
    // done > total would exceed 100 without the clamp.
    let p = derive(
        JobFamily::Ingest,
        "running",
        json!({ "tasks_done": 9, "tasks_total": 5 }),
    );
    assert_eq!(p.percent, Some(100.0));

    // total == 0 must not divide-by-zero → indeterminate.
    let p = derive(
        JobFamily::Ingest,
        "running",
        json!({ "tasks_done": 0, "tasks_total": 0 }),
    );
    assert_eq!(p.percent, None);
}

#[test]
fn non_ingest_family_ignores_task_counts() {
    // Only Ingest derives percent from tasks_*; embed/extract stay indeterminate.
    let p = derive(
        JobFamily::Embed,
        "running",
        json!({ "tasks_done": 2, "tasks_total": 4 }),
    );
    assert_eq!(p.percent, None);
}

#[test]
fn canceled_phase_is_terminal_and_indeterminate() {
    let p = derive(JobFamily::Extract, "canceled", json!({}));
    assert_eq!(p.phase, JobPhase::Canceled);
    assert!(p.phase.is_terminal());
    assert_eq!(p.percent, None);

    // British spelling maps to the same phase.
    let p = derive(JobFamily::Extract, "cancelled", json!({}));
    assert_eq!(p.phase, JobPhase::Canceled);
}

#[test]
fn unknown_status_is_treated_as_running() {
    let p = derive(JobFamily::Embed, "reticulating_splines", json!({}));
    assert_eq!(p.phase, JobPhase::Running);
    assert!(!p.phase.is_terminal());
}

#[test]
fn from_wire_value_reads_status_and_error() {
    let value = json!({
        "status": "failed",
        "error_text": "boom",
        "result_json": { "docs_embedded": 1 },
    });
    let p = JobProgress::from_wire_value(JobFamily::Embed, &value);
    assert_eq!(p.phase, JobPhase::Failed);
    assert_eq!(p.error.as_deref(), Some("boom"));

    // Missing status defaults to pending; empty error_text is dropped.
    let p = JobProgress::from_wire_value(JobFamily::Embed, &json!({ "error_text": "" }));
    assert_eq!(p.phase, JobPhase::Pending);
    assert_eq!(p.error, None);
}
