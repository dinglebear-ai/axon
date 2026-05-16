use super::*;

fn make_common_params() -> GitHubPayloadParams {
    GitHubPayloadParams {
        repo: "axon_rust".into(),
        owner: "jmagar".into(),
        content_kind: "file".into(),
        default_branch: Some("main".into()),
        ..Default::default()
    }
}

#[test]
fn payload_has_32_keys() {
    let params = make_common_params();
    let payload = build_github_payload(&params);
    let obj = payload.as_object().expect("payload is an object");
    assert_eq!(obj.len(), 32, "expected 32 gh_* keys, got {}", obj.len());
}

#[test]
fn payload_common_fields_always_present() {
    let params = make_common_params();
    let payload = build_github_payload(&params);
    assert_eq!(payload["gh_repo"], "axon_rust");
    assert_eq!(payload["gh_owner"], "jmagar");
    assert_eq!(payload["gh_content_kind"], "file");
    assert_eq!(payload["gh_default_branch"], "main");
}

#[test]
fn payload_file_fields_populated() {
    let params = GitHubPayloadParams {
        repo: "axon_rust".into(),
        owner: "jmagar".into(),
        content_kind: "file".into(),
        file_path: Some("src/main.rs".into()),
        file_language: Some("rust".into()),
        file_type: Some("source".into()),
        is_test: Some(false),
        file_size_bytes: Some(1024),
        ..Default::default()
    };
    let payload = build_github_payload(&params);
    assert_eq!(payload["gh_file_path"], "src/main.rs");
    assert_eq!(payload["gh_file_language"], "rust");
    assert_eq!(payload["gh_file_type"], "source");
    assert_eq!(payload["gh_is_test"], false);
    assert_eq!(payload["gh_file_size_bytes"], 1024);
}

#[test]
fn payload_issue_fields_null_for_file_chunks() {
    let params = make_common_params();
    let payload = build_github_payload(&params);
    assert!(payload["gh_issue_number"].is_null());
    assert!(payload["gh_state"].is_null());
    assert!(payload["gh_author"].is_null());
    assert!(payload["gh_labels"].is_null());
    assert!(payload["gh_is_pr"].is_null());
    assert!(payload["gh_merged_at"].is_null());
    assert!(payload["gh_is_draft"].is_null());
}

#[test]
fn payload_repo_metadata_null_for_file_chunks() {
    let params = make_common_params();
    let payload = build_github_payload(&params);
    assert!(payload["gh_stars"].is_null());
    assert!(payload["gh_forks"].is_null());
    assert!(payload["gh_open_issues"].is_null());
    assert!(payload["gh_language"].is_null());
    assert!(payload["gh_is_fork"].is_null());
    assert!(payload["gh_is_archived"].is_null());
}

#[test]
fn payload_issue_params_produce_correct_values() {
    let params = GitHubPayloadParams {
        repo: "axon_rust".into(),
        owner: "jmagar".into(),
        content_kind: "issue".into(),
        issue_number: Some(42),
        state: Some("open".into()),
        author: Some("contributor".into()),
        created_at: Some("2026-01-15T12:00:00Z".into()),
        updated_at: Some("2026-01-16T08:30:00Z".into()),
        comment_count: Some(5),
        labels: Some(vec!["bug".into(), "urgent".into()]),
        is_pr: Some(false),
        ..Default::default()
    };
    let payload = build_github_payload(&params);
    assert_eq!(payload["gh_content_kind"], "issue");
    assert_eq!(payload["gh_issue_number"], 42);
    assert_eq!(payload["gh_state"], "open");
    assert_eq!(payload["gh_author"], "contributor");
    assert_eq!(payload["gh_comment_count"], 5);
    assert_eq!(payload["gh_labels"], json!(["bug", "urgent"]));
    assert_eq!(payload["gh_is_pr"], false);
    // File fields should be null for issue chunks
    assert!(payload["gh_file_path"].is_null());
}

#[test]
fn payload_all_keys_start_with_gh_prefix() {
    let params = make_common_params();
    let payload = build_github_payload(&params);
    let obj = payload.as_object().unwrap();
    for key in obj.keys() {
        assert!(key.starts_with("gh_"), "key {key} missing gh_ prefix");
    }
}
