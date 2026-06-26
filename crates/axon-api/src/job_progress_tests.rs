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
