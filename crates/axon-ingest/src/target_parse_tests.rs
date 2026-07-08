use super::*;

// ── Gitea/Forgejo ────────────────────────────────────────────────────────────

#[test]
fn parses_explicit_gitea_target() {
    let target = parse_gitea_target("gitea:gitea.example.com/org/repo.git").unwrap();
    assert_eq!(target.host, "gitea.example.com");
    assert_eq!(target.owner, "org");
    assert_eq!(target.repo, "repo");
}

#[test]
fn parses_forgejo_prefix() {
    let target = parse_gitea_target("forgejo:codeberg.org/forgejo/forgejo").unwrap();
    assert_eq!(target.host, "codeberg.org");
    assert_eq!(target.owner, "forgejo");
    assert_eq!(target.repo, "forgejo");
}

#[test]
fn rejects_gitea_target_without_repo() {
    assert!(parse_gitea_target("gitea:gitea.example.com/org").is_err());
}

#[test]
fn normalizes_gitea_target_to_host_owner_repo() {
    let normalized = normalize_gitea_target("https://codeberg.org/org/repo").unwrap();
    assert_eq!(normalized, "codeberg.org/org/repo");
}

// ── GitLab ───────────────────────────────────────────────────────────────────

#[test]
fn parses_gitlab_dot_com_target() {
    let target = parse_gitlab_target("https://gitlab.com/gitlab-org/gitlab-runner").unwrap();
    assert_eq!(target.host, "gitlab.com");
    assert_eq!(target.namespace_path, "gitlab-org/gitlab-runner");
    assert_eq!(
        target.as_normalized_target(),
        "gitlab.com/gitlab-org/gitlab-runner"
    );
}

#[test]
fn parses_nested_namespace_target() {
    let target =
        parse_gitlab_target("https://gitlab.com/group/subgroup/project/-/issues/1").unwrap();
    assert_eq!(target.namespace_path, "group/subgroup/project");
}

#[test]
fn parses_explicit_self_hosted_gitlab_target() {
    let target = parse_gitlab_target("gitlab:gitlab.example.com/platform/tools/agent.git").unwrap();
    assert_eq!(target.host, "gitlab.example.com");
    assert_eq!(target.namespace_path, "platform/tools/agent");
}

#[test]
fn rejects_gitlab_target_without_project() {
    assert!(parse_gitlab_target("https://gitlab.com/group").is_err());
}

// ── Generic git ──────────────────────────────────────────────────────────────

#[test]
fn parses_explicit_https_git_target() {
    let target = parse_generic_git_target("git:https://example.com/org/repo.git").unwrap();
    assert_eq!(target.clone_url, "https://example.com/org/repo.git");
}

#[test]
fn rejects_non_https_generic_git_target() {
    assert!(parse_generic_git_target("git:ssh://example.com/org/repo.git").is_err());
    assert!(parse_generic_git_target("git:http://example.com/org/repo.git").is_err());
}

#[test]
fn parse_generic_git_target_preserves_credentials_in_clone_url() {
    let target =
        parse_generic_git_target("git:https://token:secret@example.com/org/repo.git").unwrap();
    assert_eq!(
        target.clone_url,
        "https://token:secret@example.com/org/repo.git"
    );
}
