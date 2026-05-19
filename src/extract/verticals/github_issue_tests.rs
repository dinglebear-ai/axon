use super::*;

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
