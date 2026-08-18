use std::time::Duration;

use axon_api::source::*;

use super::*;
use crate::commands::wait_progress::model::WaitViewModel;
use crate::commands::wait_progress::timing::RateEstimate;

fn empty_counts() -> StageCounts {
    StageCounts {
        items_total: None,
        items_done: 0,
        documents_total: None,
        documents_done: 0,
        chunks_total: None,
        chunks_done: 0,
        bytes_total: None,
        bytes_done: 0,
    }
}

fn representative_embedding_view() -> WaitViewModel {
    let mut view = WaitViewModel::source("https://gofastmcp.com", Some(SourceScope::Site));
    let mut counts = empty_counts();
    counts.chunks_total = Some(1936);
    counts.chunks_done = 1442;
    view.apply_snapshot(JobStatusUpdate {
        job_id: JobId::new(uuid::Uuid::from_u128(9)),
        source_id: None,
        status: LifecycleStatus::Running,
        phase: PipelinePhase::Embedding,
        stage_id: None,
        counts: Some(counts),
        current: Some(ProgressCurrent {
            source_item_key: Some(SourceItemKey::new("authentication/index.html")),
            document_id: None,
            chunk_id: None,
            adapter: Some("web".into()),
            provider: None,
            message: None,
        }),
        message: Some("embedding chunks".into()),
        error: None,
    });
    view
}

fn stable_timing() -> RateEstimate {
    RateEstimate {
        per_second: 210.0,
        remaining: Duration::from_secs(2),
    }
}

#[test]
fn narrow_layout_drops_current_then_eta_then_bar() {
    let view = representative_embedding_view();
    let wide = format_wait_view(&view, 100, Some(stable_timing()), false);
    assert!(wide.active.join("\n").contains("authentication/index.html"));
    assert!(wide.active.join("\n").contains("ETA"));
    assert!(wide.active.join("\n").contains('━'));

    let narrow = format_wait_view(&view, 42, Some(stable_timing()), false);
    assert!(
        !narrow
            .active
            .join("\n")
            .contains("authentication/index.html")
    );
    assert!(!narrow.active.join("\n").contains("ETA"));
    assert!(!narrow.active.join("\n").contains('━'));
    assert!(narrow.active.join("\n").contains("1442/1936"));
}

#[test]
fn terminal_text_removes_controls_and_middle_truncates_paths() {
    assert_eq!(sanitize_terminal_text("ok\x1b[31m\n"), "ok[31m");
    let truncated = middle_truncate("authentication/reference/index.html", 20);
    assert_eq!(truncated.chars().count(), 20);
    assert!(truncated.starts_with("auth"));
    assert!(truncated.ends_with("index.html"));
}

#[test]
fn color_disabled_snapshot_is_plain_and_keeps_operator_hierarchy() {
    let view = representative_embedding_view();
    let formatted = format_wait_view(&view, 100, None, false);
    assert_eq!(
        formatted.heading,
        "  axon source  https://gofastmcp.com · site  job 00000000"
    );
    assert!(!formatted.active.join("\n").contains("\x1b["));
    assert!(formatted.active[0].contains("embed"));
    assert!(formatted.active.join("\n").contains("74.5%"));
}

#[test]
fn terminal_statuses_render_truthful_distinct_outcomes() {
    for (status, expected) in [
        (LifecycleStatus::Completed, "✓ indexed"),
        (LifecycleStatus::CompletedDegraded, "⚠ degraded"),
        (LifecycleStatus::Failed, "✗ failed"),
        (LifecycleStatus::Canceled, "⚠ canceled"),
        (LifecycleStatus::Expired, "⚠ expired"),
        (LifecycleStatus::Skipped, "↷ skipped"),
    ] {
        let mut view = WaitViewModel::source("https://example.com", Some(SourceScope::Page));
        view.apply_snapshot(JobStatusUpdate {
            job_id: JobId::new(uuid::Uuid::from_u128(9)),
            source_id: None,
            status,
            phase: PipelinePhase::Complete,
            stage_id: None,
            counts: Some(empty_counts()),
            current: None,
            message: None,
            error: None,
        });
        let terminal = format_wait_view(&view, 80, None, false)
            .terminal
            .expect("terminal line");
        assert!(terminal.contains(expected), "{status:?}: {terminal}");
    }
}
