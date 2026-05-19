use super::*;

#[test]
fn matches_pr_url() {
    assert!(matches("https://github.com/rust-lang/rust/pull/12345"));
    assert!(matches("https://github.com/tokio-rs/tokio/pull/1"));
}

#[test]
fn rejects_issue_url() {
    assert!(!matches("https://github.com/rust-lang/rust/issues/12345"));
}

#[test]
fn rejects_non_numeric_number() {
    assert!(!matches("https://github.com/owner/repo/pull/abc"));
}

#[test]
fn rejects_too_few_segments() {
    assert!(!matches("https://github.com/rust-lang/rust/pull/"));
    assert!(!matches("https://github.com/rust-lang/rust/pull"));
}

#[test]
fn rejects_five_segments() {
    assert!(!matches(
        "https://github.com/rust-lang/rust/pull/12345/files"
    ));
    assert!(!matches(
        "https://github.com/rust-lang/rust/pull/12345/commits"
    ));
}

#[test]
fn rejects_non_github() {
    assert!(!matches("https://gitlab.com/owner/repo/merge_requests/1"));
}

#[test]
fn parse_url_parts_works() {
    let (owner, repo, number) =
        parse_url_parts("https://github.com/rust-lang/rust/pull/100").unwrap();
    assert_eq!(owner, "rust-lang");
    assert_eq!(repo, "rust");
    assert_eq!(number, 100);
}

#[test]
fn parse_url_parts_rejects_issue() {
    let result = parse_url_parts("https://github.com/rust-lang/rust/issues/100");
    assert!(result.is_none());
}

#[test]
fn build_scraped_doc_draft_merged_labels() {
    let data = serde_json::json!({
        "title": "Test PR",
        "body": "Hello world",
        "state": "closed",
        "draft": true,
        "merged": true,
        "merged_at": "2024-01-01T00:00:00Z",
        "user": {"login": "author"},
        "head": {"ref": "feature-branch"},
        "base": {"ref": "main"},
        "additions": 10,
        "deletions": 5,
        "changed_files": 3,
        "commits": 2,
        "comments": 1,
        "review_comments": 4,
        "html_url": "https://github.com/owner/repo/pull/42",
        "labels": [{"name": "bug"}, {"name": "enhancement"}],
    });
    let doc = build_scraped_doc(
        "https://github.com/owner/repo/pull/42",
        "owner",
        "repo",
        42,
        &data,
    )
    .unwrap();
    assert!(doc.markdown.contains("[DRAFT]"));
    assert!(doc.markdown.contains("[MERGED]"));
    assert!(doc.markdown.contains("bug"));
    assert!(doc.markdown.contains("feature-branch"));
}
