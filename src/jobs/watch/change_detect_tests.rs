use super::*;
use crate::services::types::{DiffResult, DiffStatus, LinkEntry};

fn diff(status: DiffStatus, links: usize, word_delta: i64) -> DiffResult {
    let text_diff = if matches!(status, DiffStatus::Changed) {
        Some("d".into())
    } else {
        None
    };
    DiffResult {
        url_a: "a".into(),
        url_b: "b".into(),
        status,
        text_diff,
        metadata_changes: vec![],
        links_added: (0..links)
            .map(|i| LinkEntry {
                href: format!("h{i}"),
                text: "".into(),
            })
            .collect(),
        links_removed: vec![],
        word_count_delta: word_delta,
    }
}

#[test]
fn same_is_not_meaningful() {
    assert!(!is_meaningful(&diff(DiffStatus::Same, 0, 0), 0));
}
#[test]
fn any_text_change_meaningful_at_threshold_zero() {
    assert!(is_meaningful(&diff(DiffStatus::Changed, 0, 1), 0));
}
#[test]
fn sub_threshold_text_change_not_meaningful() {
    assert!(!is_meaningful(&diff(DiffStatus::Changed, 0, 2), 5));
}
#[test]
fn link_change_always_meaningful() {
    assert!(is_meaningful(&diff(DiffStatus::Changed, 1, 0), 100));
}

#[test]
fn snapshot_hash_detects_link_only_change() {
    // Identical visible markdown but a different links snapshot must produce a
    // different hash, so the fast-equal shortcut does not skip a link-only
    // change before compute_diff can apply the "links always count" rule.
    let md = "same visible markdown";
    let links_a = r#"[{"href":"https://a.example/x","text":""}]"#;
    let links_b = r#"[{"href":"https://a.example/y","text":""}]"#;
    assert_ne!(snapshot_hash(md, links_a), snapshot_hash(md, links_b));
    // Sanity: stable under identical inputs.
    assert_eq!(snapshot_hash(md, links_a), snapshot_hash(md, links_a));
}
