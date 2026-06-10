use super::*;
use serde_json::json;

fn make_common_params() -> GitHubPayloadParams {
    GitHubPayloadParams {
        repo: "axon_rust".into(),
        owner: "jmagar".into(),
        content_kind: ContentKind::File,
        default_branch: Some("main".into()),
        ..Default::default()
    }
}

#[test]
fn payload_has_canonical_git_and_code_keys_only() {
    let params = make_common_params();
    let payload = build_github_payload(&params);
    let obj = payload.as_object().expect("payload is an object");
    let gh_count = obj.keys().filter(|k| k.starts_with("gh_")).count();
    let git_count = obj.keys().filter(|k| k.starts_with("git_")).count();
    let code_count = obj.keys().filter(|k| k.starts_with("code_")).count();
    assert_eq!(gh_count, 0, "clean schema must not emit gh_* keys");
    assert!(git_count > 0, "expected git_* keys to be present");
    assert!(code_count > 0, "expected code_* keys to be present");
    assert!(obj.contains_key("provider"), "expected provider key");
}

#[test]
fn payload_common_fields_always_present() {
    let params = make_common_params();
    let payload = build_github_payload(&params);
    assert_eq!(payload["git_repo"], "axon_rust");
    assert_eq!(payload["git_owner"], "jmagar");
    assert_eq!(payload["git_content_kind"], "file");
    assert_eq!(payload["git_default_branch"], "main");
}

#[test]
fn payload_file_fields_populated() {
    let params = GitHubPayloadParams {
        repo: "axon_rust".into(),
        owner: "jmagar".into(),
        content_kind: ContentKind::File,
        file_path: Some("src/main.rs".into()),
        file_language: Some("rust".into()),
        file_type: Some("source".into()),
        is_test: Some(false),
        file_size_bytes: Some(1024),
        ..Default::default()
    };
    let payload = build_github_payload(&params);
    // git_file_path / git_file_language were dropped (Q-M5) — file path/language
    // now live only under the code_* namespace to avoid duplicate payload keys.
    assert_eq!(payload["code_file_path"], "src/main.rs");
    assert_eq!(payload["code_language"], "rust");
    assert_eq!(payload["code_file_type"], "source");
    assert_eq!(payload["code_is_test"], false);
    assert_eq!(payload["code_file_size_bytes"], 1024);
}

#[test]
fn payload_code_chunk_metadata_populated_when_present() {
    let params = GitHubPayloadParams {
        repo: "axon_rust".into(),
        owner: "jmagar".into(),
        content_kind: ContentKind::File,
        chunking_method: Some("tree_sitter".into()),
        symbol_name: Some("Response::parse".into()),
        symbol_kind: Some("method".into()),
        symbol_extraction_status: Some("ok".into()),
        ..Default::default()
    };
    let payload = build_github_payload(&params);
    assert_eq!(payload["code_chunking_method"], "tree_sitter");
    assert_eq!(payload["symbol_name"], "Response::parse");
    assert_eq!(payload["symbol_kind"], "method");
    assert_eq!(payload["symbol_extraction_status"], "ok");
}

#[test]
fn payload_issue_fields_null_for_file_chunks() {
    let params = make_common_params();
    let payload = build_github_payload(&params);
    assert!(payload["git_number"].is_null());
    assert!(payload["git_state"].is_null());
    assert!(payload["git_author"].is_null());
    assert!(payload["git_labels"].is_null());
    assert!(payload["git_is_pr"].is_null());
    assert!(payload["git_merged_at"].is_null());
    assert!(payload["git_is_draft"].is_null());
}

#[test]
fn payload_repo_metadata_null_for_file_chunks() {
    let params = make_common_params();
    let payload = build_github_payload(&params);
    assert!(payload["git_repo_stars"].is_null());
    assert!(payload["git_repo_forks"].is_null());
    assert!(payload["git_repo_open_issues"].is_null());
    assert!(payload["git_repo_language"].is_null());
    assert!(payload["git_repo_is_fork"].is_null());
    assert!(payload["git_repo_is_archived"].is_null());
}

#[test]
fn payload_issue_params_produce_correct_values() {
    let params = GitHubPayloadParams {
        repo: "axon_rust".into(),
        owner: "jmagar".into(),
        content_kind: ContentKind::Issue,
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
    assert_eq!(payload["git_content_kind"], "issue");
    assert_eq!(payload["git_number"], 42);
    assert_eq!(payload["git_state"], "open");
    assert_eq!(payload["git_author"], "contributor");
    assert_eq!(payload["git_comment_count"], 5);
    assert_eq!(payload["git_labels"], json!(["bug", "urgent"]));
    assert_eq!(payload["git_is_pr"], false);
    // File fields should be null for issue chunks
    assert!(payload["git_file_path"].is_null());
    assert!(payload["code_file_path"].is_null());
}

#[test]
fn payload_keys_use_known_prefixes() {
    let params = make_common_params();
    let payload = build_github_payload(&params);
    let obj = payload.as_object().unwrap();
    for key in obj.keys() {
        let valid = key.starts_with("git_")
            || key.starts_with("code_")
            || matches!(
                key.as_str(),
                "provider" | "symbol_name" | "symbol_kind" | "symbol_extraction_status"
            );
        assert!(
            valid,
            "unexpected key '{key}' — expected git_*, code_*, provider, or symbol metadata"
        );
    }
}

#[test]
fn promoted_fields_are_canonical_top_level_fields() {
    let params = GitHubPayloadParams {
        repo: "axon_rust".into(),
        owner: "jmagar".into(),
        content_kind: ContentKind::File,
        stars: Some(42),
        forks: Some(7),
        language: Some("Rust".into()),
        topics: Some(vec!["cli".into(), "rag".into()]),
        is_fork: Some(false),
        is_archived: Some(false),
        file_type: Some("source".into()),
        line_start: Some(10),
        line_end: Some(50),
        ..Default::default()
    };
    let payload = build_github_payload(&params);

    assert_eq!(payload["git_repo_stars"], 42);
    assert_eq!(payload["git_repo_forks"], 7);
    assert_eq!(payload["git_repo_language"], "Rust");
    assert_eq!(payload["git_repo_topics"], json!(["cli", "rag"]));
    assert_eq!(payload["git_repo_is_fork"], false);
    assert_eq!(payload["git_repo_is_archived"], false);
    assert_eq!(payload["code_file_type"], "source");
    assert_eq!(payload["code_line_start"], 10);
    assert_eq!(payload["code_line_end"], 50);
    assert!(payload["git_meta"].is_null());
}
