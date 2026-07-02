use super::*;

#[test]
fn parses_github_https_target() {
    let t = parse_git_target("https://github.com/jmagar/axon.git").unwrap();
    assert_eq!(t.provider, "github");
    assert_eq!(t.host, "github.com");
    assert_eq!(t.owner.as_deref(), Some("jmagar"));
    assert_eq!(t.repo, "axon");
    assert_eq!(t.web_url, "https://github.com/jmagar/axon");
    assert_eq!(t.clone_url, "https://github.com/jmagar/axon.git");
}

#[test]
fn detects_gitlab_and_gitea_providers() {
    assert_eq!(
        parse_git_target("https://gitlab.com/group/proj")
            .unwrap()
            .provider,
        "gitlab"
    );
    assert_eq!(
        parse_git_target("https://codeberg.org/o/r")
            .unwrap()
            .provider,
        "gitea"
    );
    assert_eq!(
        parse_git_target("https://git.example.com/o/r")
            .unwrap()
            .provider,
        "git"
    );
}

#[test]
fn strips_embedded_credentials_from_web_url() {
    let t = parse_git_target("https://user:token@github.com/o/secret.git").unwrap();
    assert_eq!(t.web_url, "https://github.com/o/secret");
    assert!(!t.web_url.contains("token"));
    assert!(!t.web_url.contains("user"));
}

#[test]
fn accepts_git_prefix() {
    let t = parse_git_target("git:https://github.com/o/r").unwrap();
    assert_eq!(t.repo, "r");
}

#[test]
fn rejects_non_https() {
    assert!(parse_git_target("git@github.com:o/r.git").is_err());
    assert!(parse_git_target("http://github.com/o/r").is_err());
    assert!(parse_git_target("https://github.com").is_err());
}

#[test]
fn nested_group_owner_is_last_path_segment_before_repo() {
    let t = parse_git_target("https://gitlab.com/group/sub/proj.git").unwrap();
    assert_eq!(t.owner.as_deref(), Some("sub"));
    assert_eq!(t.repo, "proj");
}
