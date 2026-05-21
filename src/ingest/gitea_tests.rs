use super::*;

#[test]
fn parses_explicit_gitea_target() {
    let target = parse_gitea_target("gitea:gitea.example.com/org/repo.git").unwrap();
    assert_eq!(target.host, "gitea.example.com");
    assert_eq!(target.owner, "org");
    assert_eq!(target.repo, "repo");
    assert_eq!(target.web_url, "https://gitea.example.com/org/repo");
    assert_eq!(target.api_base, "https://gitea.example.com/api/v1");
}

#[test]
fn parses_forgejo_prefix() {
    let target = parse_gitea_target("forgejo:codeberg.org/forgejo/forgejo").unwrap();
    assert_eq!(target.host, "codeberg.org");
    assert_eq!(target.owner, "forgejo");
    assert_eq!(target.repo, "forgejo");
}

#[test]
fn rejects_target_without_repo() {
    assert!(parse_gitea_target("gitea:gitea.example.com/org").is_err());
}
