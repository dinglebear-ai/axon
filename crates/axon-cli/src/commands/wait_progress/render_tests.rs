use axon_core::config::Config;
use indicatif::InMemoryTerm;

use super::*;
use crate::commands::wait_progress::format::FormattedWaitView;

fn active_formatted_view() -> FormattedWaitView {
    FormattedWaitView {
        heading: "axon  source".into(),
        milestones: Vec::new(),
        notices: Vec::new(),
        active: vec!["◐ embed  1442/1936 chunks · embedding chunks".into()],
        terminal: None,
    }
}

fn completed_formatted_view() -> FormattedWaitView {
    FormattedWaitView {
        heading: "axon  source".into(),
        milestones: vec!["✓ indexed  1936 chunks".into()],
        notices: Vec::new(),
        active: Vec::new(),
        terminal: Some("✓ indexed  1936 chunks".into()),
    }
}

#[test]
fn json_and_quiet_modes_are_silent() {
    let mut cfg = Config {
        json_output: true,
        ..Config::default()
    };
    assert_eq!(ProgressMode::for_config(&cfg, true), ProgressMode::Silent);
    cfg.json_output = false;
    cfg.quiet = true;
    assert_eq!(ProgressMode::for_config(&cfg, true), ProgressMode::Silent);
}

#[test]
fn redirected_stderr_uses_plain_important_events_only() {
    let cfg = Config::default();
    assert_eq!(ProgressMode::for_config(&cfg, false), ProgressMode::Plain);

    let term = InMemoryTerm::new(20, 100);
    let mut renderer = WaitRenderer::for_test(term.clone(), ProgressMode::Plain);
    let mut view = active_formatted_view();
    view.milestones.push("✓ prepared 10 documents".into());
    renderer.render_now(&view).unwrap();
    assert!(term.contents().is_empty());

    view.notices.push("⚠ provider retry".into());
    renderer.render_now(&view).unwrap();
    view.terminal = Some("✓ indexed 10 documents".into());
    renderer.finish(&view).unwrap();
    let output = term.contents();
    assert_eq!(output.matches("provider retry").count(), 1);
    assert_eq!(output.matches("indexed").count(), 1);
    assert!(!output.contains("prepared"));
    assert!(!output.contains("\x1b["));
}

#[tokio::test]
async fn interactive_finish_clears_the_live_region_once() {
    let term = InMemoryTerm::new(20, 100);
    let mut renderer = WaitRenderer::for_test(term.clone(), ProgressMode::Interactive);
    renderer.render(&active_formatted_view()).unwrap();
    renderer.finish(&completed_formatted_view()).unwrap();
    renderer.finish(&completed_formatted_view()).unwrap();
    let contents = term.contents();
    assert_eq!(contents.matches("indexed").count(), 1);
    assert!(!contents.contains("embedding chunks"), "{contents:?}");
}

#[tokio::test]
async fn warning_updates_stay_in_one_live_region() {
    let term = InMemoryTerm::new(20, 100);
    let mut renderer = WaitRenderer::for_test(term.clone(), ProgressMode::Interactive);
    let mut view = active_formatted_view();
    view.notices.push("⚠ policy held 1 chunk".into());
    renderer.render_now(&view).unwrap();
    view.notices[0] = "⚠ policy held 3 chunks".into();
    renderer.render_now(&view).unwrap();
    assert_eq!(term.contents().matches("policy held").count(), 1);
    assert!(term.contents().contains("3 chunks"));
}
