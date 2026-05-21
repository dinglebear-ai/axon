use super::*;

#[test]
fn parses_gitlab_dot_com_target() {
    let target = parse_gitlab_target("https://gitlab.com/gitlab-org/gitlab-runner").unwrap();
    assert_eq!(target.host, "gitlab.com");
    assert_eq!(target.namespace_path, "gitlab-org/gitlab-runner");
    assert_eq!(target.project, "gitlab-runner");
    assert_eq!(
        target.as_normalized_target(),
        "gitlab.com/gitlab-org/gitlab-runner"
    );
    assert_eq!(target.encoded_project_path, "gitlab-org%2Fgitlab-runner");
}

#[test]
fn parses_nested_namespace_target() {
    let target =
        parse_gitlab_target("https://gitlab.com/group/subgroup/project/-/issues/1").unwrap();
    assert_eq!(target.namespace_path, "group/subgroup/project");
    assert_eq!(target.encoded_project_path, "group%2Fsubgroup%2Fproject");
}

#[test]
fn parses_explicit_self_hosted_target() {
    let target = parse_gitlab_target("gitlab:gitlab.example.com/platform/tools/agent.git").unwrap();
    assert_eq!(target.host, "gitlab.example.com");
    assert_eq!(target.namespace_path, "platform/tools/agent");
    assert_eq!(
        target.clone_url,
        "https://gitlab.example.com/platform/tools/agent.git"
    );
    assert_eq!(target.api_base, "https://gitlab.example.com/api/v4");
}

#[test]
fn rejects_target_without_project() {
    assert!(parse_gitlab_target("https://gitlab.com/group").is_err());
}
