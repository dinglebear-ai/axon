use super::*;
use crate::services::types::{DiffResult, DiffStatus, LinkEntry, MetadataChange};

fn make_same_result() -> DiffResult {
    DiffResult {
        url_a: "https://example.com/a".to_string(),
        url_b: "https://example.com/b".to_string(),
        status: DiffStatus::Same,
        text_diff: None,
        metadata_changes: vec![],
        links_added: vec![],
        links_removed: vec![],
        word_count_delta: 0,
    }
}

fn make_changed_result() -> DiffResult {
    DiffResult {
        url_a: "https://example.com/a".to_string(),
        url_b: "https://example.com/b".to_string(),
        status: DiffStatus::Changed,
        text_diff: Some("--- a\n+++ b\n@@ -1 +1 @@\n-old\n+new\n".to_string()),
        metadata_changes: vec![MetadataChange {
            field: "title".to_string(),
            old: Some("Old".to_string()),
            new: Some("New".to_string()),
        }],
        links_added: vec![LinkEntry {
            href: "https://new.com".to_string(),
            text: "New Link".to_string(),
        }],
        links_removed: vec![],
        word_count_delta: 1,
    }
}

#[test]
fn test_format_same_result_human() {
    let result = make_same_result();
    let output = format_diff_summary(&result);
    assert!(
        output.contains("same") || output.contains("Same") || output.contains("no changes"),
        "same result should indicate no changes, got: {output}"
    );
}

#[test]
fn test_format_changed_result_human() {
    let result = make_changed_result();
    let output = format_diff_summary(&result);
    assert!(
        output.contains("changed") || output.contains("Changed"),
        "changed result should indicate changes, got: {output}"
    );
}

#[test]
fn test_format_diff_shows_word_count_delta() {
    let result = make_changed_result();
    let output = format_diff_summary(&result);
    assert!(
        output.contains('+') || output.contains("word"),
        "output should mention word count delta, got: {output}"
    );
}
