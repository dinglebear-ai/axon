use super::*;
use crate::types::{DiffResult, DiffStatus, LinkEntry};

/// Helper: build a minimal pair of markdown strings and call the pure diff logic.
fn run_pure_diff(md_a: &str, md_b: &str) -> DiffResult {
    compute_diff(
        "https://example.com/a",
        md_a,
        &[],
        &serde_json::Value::Object(serde_json::Map::new()),
        "https://example.com/b",
        md_b,
        &[],
        &serde_json::Value::Object(serde_json::Map::new()),
    )
}

#[test]
fn test_identical_content_is_same() {
    let r = run_pure_diff("# Hello\n\nContent.", "# Hello\n\nContent.");
    assert_eq!(r.status, DiffStatus::Same);
    assert!(r.text_diff.is_none());
    assert!(r.metadata_changes.is_empty());
    assert_eq!(r.word_count_delta, 0);
}

#[test]
fn test_changed_content_produces_diff() {
    let r = run_pure_diff("# Hello\n\nOld paragraph.", "# Hello\n\nNew paragraph.");
    assert_eq!(r.status, DiffStatus::Changed);
    let diff_text = r.text_diff.unwrap();
    assert!(diff_text.contains('-'), "should have removal markers");
    assert!(diff_text.contains('+'), "should have addition markers");
}

#[test]
fn test_word_count_delta_positive() {
    let r = run_pure_diff("one two three", "one two three four five");
    assert_eq!(r.word_count_delta, 2);
}

#[test]
fn test_word_count_delta_negative() {
    let r = run_pure_diff("one two three four five", "one two three");
    assert_eq!(r.word_count_delta, -2);
}

#[test]
fn test_link_added() {
    let links_b = vec![LinkEntry {
        href: "https://new.com".to_string(),
        text: "New".to_string(),
    }];
    let result = compute_diff(
        "https://example.com/a",
        "Content",
        &[],
        &serde_json::Value::Object(serde_json::Map::new()),
        "https://example.com/b",
        "Content",
        &links_b,
        &serde_json::Value::Object(serde_json::Map::new()),
    );
    assert_eq!(result.links_added.len(), 1);
    assert_eq!(result.links_added[0].href, "https://new.com");
    assert!(result.links_removed.is_empty());
}

#[test]
fn test_link_removed() {
    let links_a = vec![LinkEntry {
        href: "https://old.com".to_string(),
        text: "Old".to_string(),
    }];
    let result = compute_diff(
        "https://example.com/a",
        "Content",
        &links_a,
        &serde_json::Value::Object(serde_json::Map::new()),
        "https://example.com/b",
        "Content",
        &[],
        &serde_json::Value::Object(serde_json::Map::new()),
    );
    assert!(result.links_added.is_empty());
    assert_eq!(result.links_removed.len(), 1);
    assert_eq!(result.links_removed[0].href, "https://old.com");
}

#[test]
fn test_metadata_title_change() {
    let mut meta_a = serde_json::Map::new();
    meta_a.insert(
        "title".to_string(),
        serde_json::Value::String("Old Title".to_string()),
    );
    let mut meta_b = serde_json::Map::new();
    meta_b.insert(
        "title".to_string(),
        serde_json::Value::String("New Title".to_string()),
    );

    let result = compute_diff(
        "https://example.com/a",
        "Content",
        &[],
        &serde_json::Value::Object(meta_a),
        "https://example.com/b",
        "Content",
        &[],
        &serde_json::Value::Object(meta_b),
    );
    assert_eq!(result.status, DiffStatus::Changed);
    assert_eq!(result.metadata_changes.len(), 1);
    assert_eq!(result.metadata_changes[0].field, "title");
    assert_eq!(result.metadata_changes[0].old.as_deref(), Some("Old Title"));
    assert_eq!(result.metadata_changes[0].new.as_deref(), Some("New Title"));
}
