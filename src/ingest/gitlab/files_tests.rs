use super::super::embed::gitlab_file_chunk_payload;
use crate::ingest::gitlab::types::{GitLabProject, GitLabTarget};
use crate::vector::ops::input::code::{CodeChunk, Symbol, SymbolKind};

fn make_target(namespace_path: &str) -> GitLabTarget {
    let project = namespace_path
        .rsplit('/')
        .next()
        .unwrap_or(namespace_path)
        .to_string();
    GitLabTarget {
        host: "gitlab.com".into(),
        namespace_path: namespace_path.into(),
        project,
        web_url: format!("https://gitlab.com/{namespace_path}"),
        clone_url: format!("https://gitlab.com/{namespace_path}.git"),
        api_base: "https://gitlab.com/api/v4".into(),
        encoded_project_path: namespace_path.replace('/', "%2F"),
    }
}

fn make_project() -> GitLabProject {
    GitLabProject {
        path_with_namespace: "group/project".into(),
        name: "project".into(),
        description: None,
        default_branch: Some("main".into()),
        web_url: "https://gitlab.com/group/project".into(),
        visibility: Some("public".into()),
        star_count: None,
        forks_count: None,
        open_issues_count: None,
        issues_enabled: Some(true),
        merge_requests_enabled: Some(true),
        wiki_enabled: Some(false),
        last_activity_at: None,
    }
}

fn make_chunk() -> CodeChunk {
    CodeChunk {
        text: "fn x() {}".into(),
        byte_start: 0,
        byte_end: 9,
        start_line: 1,
        end_line: 1,
        declaration_start_line: 1,
        declaration_end_line: 1,
        symbol: Some(Symbol {
            kind: SymbolKind::Function,
            name: Some("x".into()),
        }),
    }
}

#[test]
fn owner_derivation_single_segment_namespace_yields_none() {
    // Single segment: no '/' in namespace_path → owner should be None / absent
    let target = make_target("project");
    let project = make_project();
    let chunk = make_chunk();
    let payload = gitlab_file_chunk_payload(
        &target,
        &project,
        "src/lib.rs",
        "main",
        &chunk,
        "tree_sitter",
        "ok",
    );
    // git_owner should be absent or null when there is no namespace prefix
    assert!(
        payload
            .get("git_owner")
            .map(|v| v.is_null())
            .unwrap_or(true),
        "single-segment namespace should produce no git_owner"
    );
}

#[test]
fn owner_derivation_three_segment_namespace() {
    // Three segments: group/subgroup/project → owner should be "group/subgroup"
    let target = make_target("group/subgroup/project");
    let project = make_project();
    let chunk = make_chunk();
    let payload = gitlab_file_chunk_payload(
        &target,
        &project,
        "src/lib.rs",
        "main",
        &chunk,
        "tree_sitter",
        "ok",
    );
    assert_eq!(
        payload["git_owner"], "group/subgroup",
        "three-segment namespace should produce owner = group/subgroup"
    );
}

#[test]
fn owner_derivation_two_segment_namespace() {
    // Two segments: group/project → owner should be "group"
    let target = make_target("group/project");
    let project = make_project();
    let chunk = make_chunk();
    let payload = gitlab_file_chunk_payload(
        &target,
        &project,
        "src/lib.rs",
        "main",
        &chunk,
        "tree_sitter",
        "ok",
    );
    assert_eq!(
        payload["git_owner"], "group",
        "two-segment namespace should produce owner = group"
    );
}
