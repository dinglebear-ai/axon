use super::*;

#[test]
fn build_extra_sets_fields() {
    let extra = build_extra(
        "rust-lang",
        "rust",
        100,
        "open",
        "ferris",
        &["bug", "help-wanted"],
        "2024-01-01T00:00:00Z",
    );
    assert_eq!(extra["provider"], "github");
    assert_eq!(extra["git_content_kind"], "issue");
    assert_eq!(extra["git_owner"], "rust-lang");
    assert_eq!(extra["git_repo"], "rust");
    assert_eq!(extra["git_state"], "open");
    assert_eq!(extra["git_number"], 100);
    assert_eq!(extra["git_author"], "ferris");
    let labels = extra["git_labels"].as_array().unwrap();
    assert_eq!(labels.len(), 2);
}

#[test]
fn build_extra_empty_state() {
    let extra = build_extra("owner", "repo", 1, "", "", &[], "");
    assert!(extra["git_state"].is_null());
    assert!(extra["git_author"].is_null());
    assert!(extra["git_labels"].is_null());
}

#[test]
fn matches_issue_url() {
    assert!(matches("https://github.com/rust-lang/rust/issues/12345"));
    assert!(matches("https://github.com/tokio-rs/tokio/issues/1"));
}

#[test]
fn rejects_pr_url() {
    // PR URLs use /pull/, not /issues/
    assert!(!matches("https://github.com/rust-lang/rust/pull/12345"));
}

#[test]
fn rejects_non_numeric_number() {
    assert!(!matches("https://github.com/owner/repo/issues/abc"));
}

#[test]
fn rejects_too_few_segments() {
    assert!(!matches("https://github.com/rust-lang/rust/issues/"));
    assert!(!matches("https://github.com/rust-lang/rust/issues"));
    assert!(!matches("https://github.com/rust-lang/rust"));
}

#[test]
fn rejects_five_segments() {
    // Issues can't have sub-paths
    assert!(!matches(
        "https://github.com/rust-lang/rust/issues/12345/comments"
    ));
}

#[test]
fn rejects_non_github() {
    assert!(!matches("https://gitlab.com/owner/repo/issues/1"));
}

#[test]
fn parse_url_parts_works() {
    let (owner, repo, number) =
        parse_url_parts("https://github.com/rust-lang/rust/issues/100").unwrap();
    assert_eq!(owner, "rust-lang");
    assert_eq!(repo, "rust");
    assert_eq!(number, 100);
}

#[test]
fn parse_url_parts_rejects_pr() {
    let result = parse_url_parts("https://github.com/rust-lang/rust/pull/100");
    assert!(result.is_none());
}
