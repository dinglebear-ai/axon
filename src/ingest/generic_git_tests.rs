use super::*;

#[test]
fn parses_explicit_https_git_target() {
    let target = parse_generic_git_target("git:https://example.com/org/repo.git").unwrap();
    assert_eq!(target.host, "example.com");
    assert_eq!(target.name, "repo");
    assert_eq!(target.clone_url, "https://example.com/org/repo.git");
    assert_eq!(target.web_url, "https://example.com/org/repo");
}

#[test]
fn rejects_non_https_generic_git_target() {
    assert!(parse_generic_git_target("git:ssh://example.com/org/repo.git").is_err());
    assert!(parse_generic_git_target("git:http://example.com/org/repo.git").is_err());
}
