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

fn repo_fixture() -> client::GiteaRepo {
    client::GiteaRepo {
        full_name: Some("org/repo".to_string()),
        description: None,
        html_url: Some("https://codeberg.org/org/repo".to_string()),
        clone_url: Some("https://codeberg.org/org/repo.git".to_string()),
        default_branch: Some("main".to_string()),
        private: Some(false),
        stars_count: Some(10),
        forks_count: Some(2),
        open_issues_count: Some(1),
    }
}

fn target_fixture() -> GiteaTarget {
    GiteaTarget {
        host: "codeberg.org".to_string(),
        owner: "org".to_string(),
        repo: "repo".to_string(),
        web_url: "https://codeberg.org/org/repo".to_string(),
        api_base: "https://codeberg.org/api/v1".to_string(),
    }
}

#[test]
fn gitea_issue_doc_uses_plain_text_source_metadata() {
    let target = target_fixture();
    let repo = repo_fixture();
    let doc = embed::issue_doc(
        &target,
        &repo,
        client::GiteaIssue {
            number: 7,
            title: "Bug report".to_string(),
            body: Some("body".to_string()),
            state: Some("open".to_string()),
            html_url: None,
            user: Some(client::GiteaUser {
                login: Some("alice".to_string()),
                full_name: None,
            }),
            labels: Some(vec![client::GiteaLabel {
                name: "bug".to_string(),
            }]),
            created_at: Some("2026-01-01T00:00:00Z".to_string()),
            updated_at: Some("2026-01-02T00:00:00Z".to_string()),
            comments: Some(3),
        },
    )
    .expect("issue prep should not fail")
    .expect("issue doc");

    assert_eq!(doc.source_type(), "gitea");
    assert_eq!(doc.content_type(), "text");
    assert_eq!(doc.extra().unwrap()["git_content_kind"], "issue");
    assert_eq!(doc.chunk_extra()[0]["chunk_content_kind"], "plain_text");
}

#[test]
fn gitea_pull_doc_uses_plain_text_source_metadata() {
    let target = target_fixture();
    let repo = repo_fixture();
    let doc = embed::pull_doc(
        &target,
        &repo,
        client::GiteaPullRequest {
            number: 11,
            title: "Add feature".to_string(),
            body: Some("body".to_string()),
            state: Some("closed".to_string()),
            html_url: None,
            user: Some(client::GiteaUser {
                login: Some("bob".to_string()),
                full_name: None,
            }),
            labels: None,
            created_at: None,
            updated_at: None,
            comments: Some(1),
            merged: Some(true),
        },
    )
    .expect("pull prep should not fail")
    .expect("pull doc");

    assert_eq!(doc.source_type(), "gitea");
    assert_eq!(doc.content_type(), "text");
    assert_eq!(doc.extra().unwrap()["git_content_kind"], "pr");
    assert_eq!(doc.chunk_extra()[0]["chunk_content_kind"], "plain_text");
}
