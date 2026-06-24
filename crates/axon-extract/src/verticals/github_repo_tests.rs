use super::*;

#[test]
fn matches_repo_url() {
    assert!(matches("https://github.com/rust-lang/rust"));
    assert!(matches("https://github.com/tokio-rs/tokio"));
}

#[test]
fn rejects_reserved_owners() {
    assert!(!matches("https://github.com/settings/profile"));
    assert!(!matches("https://github.com/marketplace/actions"));
}

#[test]
fn rejects_sub_paths() {
    assert!(!matches("https://github.com/rust-lang/rust/issues"));
    assert!(!matches(
        "https://github.com/rust-lang/rust/blob/main/README.md"
    ));
}

#[test]
fn rejects_file_extension_in_repo() {
    assert!(!matches("https://github.com/owner/repo.git"));
}

#[test]
fn rejects_non_github() {
    assert!(!matches("https://gitlab.com/owner/repo"));
}

#[test]
fn build_extra_sets_provider_and_kind() {
    let data = serde_json::json!({
        "stargazers_count": 42,
        "forks_count": 7,
        "language": "Rust",
        "topics": ["async", "networking"],
        "visibility": "public",
        "clone_url": "https://github.com/owner/repo.git",
    });
    let extra = build_extra("owner", "repo", &data);
    assert_eq!(extra["provider"], "github");
    assert_eq!(extra["git_content_kind"], "repo_metadata");
    assert_eq!(extra["git_owner"], "owner");
    assert_eq!(extra["git_repo"], "repo");
    assert_eq!(extra["git_meta"]["stars"], 42);
    assert_eq!(extra["git_meta"]["language"], "Rust");
    let topics = extra["git_meta"]["topics"].as_array().unwrap();
    assert_eq!(topics.len(), 2);
    assert_eq!(topics[0], "async");
}

#[test]
fn build_extra_empty_topics() {
    let data = serde_json::json!({});
    let extra = build_extra("owner", "repo", &data);
    let topics = extra["git_meta"]["topics"].as_array().unwrap();
    assert!(topics.is_empty());
}
