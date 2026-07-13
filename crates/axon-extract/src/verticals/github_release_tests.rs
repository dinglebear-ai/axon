use super::*;

#[test]
fn matches_releases_url() {
    assert!(matches("https://github.com/owner/repo/releases"));
    assert!(matches("https://github.com/owner/repo/releases/tag/v1.0.0"));
}

#[test]
fn rejects_non_release_url() {
    assert!(!matches("https://github.com/owner/repo"));
    assert!(!matches("https://github.com/owner/repo/issues/1"));
}

#[test]
fn rejects_non_github() {
    assert!(!matches("https://gitlab.com/owner/repo/releases"));
}

#[test]
fn build_extra_sets_fields() {
    let extra = build_extra("owner", "repo");
    assert_eq!(extra["provider"], "github");
    assert_eq!(extra["git_host"], "github.com");
    assert_eq!(extra["git_owner"], "owner");
    assert_eq!(extra["git_repo"], "repo");
    assert_eq!(extra["git_content_kind"], "release");
}
