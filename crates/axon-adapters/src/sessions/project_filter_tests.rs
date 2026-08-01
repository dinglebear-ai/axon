use super::*;
use std::fs;

#[test]
fn empty_filter_allows_all() {
    assert!(matches_project_filter(
        None,
        Path::new("/tmp/root"),
        Path::new("/tmp/root/session.jsonl"),
        "session.jsonl",
    ));
    assert!(matches_project_filter(
        Some("  "),
        Path::new("/tmp/root"),
        Path::new("/tmp/root/session.jsonl"),
        "session.jsonl",
    ));
}

#[test]
fn matches_relative_or_root_path_case_insensitively() {
    assert!(matches_project_filter(
        Some("axon"),
        Path::new("/home/me/.claude/projects"),
        Path::new("/home/me/.claude/projects/-home-me-workspace-Axon/session.jsonl"),
        "-home-me-workspace-Axon/session.jsonl",
    ));
    assert!(matches_project_filter(
        Some("/home/me/workspace/axon"),
        Path::new("/home/me/.claude/projects/-home-me-workspace-axon"),
        Path::new("/home/me/.claude/projects/-home-me-workspace-axon/session.jsonl"),
        "session.jsonl",
    ));
}

#[test]
fn rejects_unmatched_project() {
    assert!(!matches_project_filter(
        Some("other-project"),
        Path::new("/home/me/.codex/sessions"),
        Path::new("/home/me/.codex/sessions/2026/07/15/session.jsonl"),
        "2026/07/15/session.jsonl",
    ));
}

#[test]
fn matches_project_in_structured_session_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("rollout.jsonl");
    fs::write(
        &file,
        r#"{"type":"session_meta","payload":{"cwd":"/home/me/workspace/axon"}}"#,
    )
    .unwrap();

    assert!(matches_project_filter(
        Some("/home/me/workspace/axon"),
        dir.path(),
        &file,
        "rollout.jsonl",
    ));
}
