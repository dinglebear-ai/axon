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
fn payload_has_gh_and_git_keys() {
    let params = make_common_params();
    let payload = build_github_payload(&params);
    let obj = payload.as_object().expect("payload is an object");
    // Backwards-compat gh_* keys (32) plus canonical git_* keys and provider.
    let gh_count = obj.keys().filter(|k| k.starts_with("gh_")).count();
    let git_count = obj.keys().filter(|k| k.starts_with("git_")).count();
    assert_eq!(gh_count, 32, "expected 32 gh_* keys, got {gh_count}");
    assert!(git_count > 0, "expected git_* keys to be present");
    assert!(obj.contains_key("provider"), "expected provider key");
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
fn payload_keys_use_known_prefixes() {
    let params = make_common_params();
    let payload = build_github_payload(&params);
    let obj = payload.as_object().unwrap();
    for key in obj.keys() {
        let valid = key.starts_with("gh_") || key.starts_with("git_") || key == "provider";
        assert!(
            valid,
            "unexpected key '{key}' — expected gh_*, git_*, or provider"
        );
    }
}

#[test]
fn promoted_fields_not_in_git_meta_blob() {
    // gh_stars, gh_forks, gh_language, gh_topics, gh_is_fork, gh_is_archived,
    // gh_file_type, gh_line_start, gh_line_end must live at the TOP LEVEL of the
    // payload — not inside git_meta — so Qdrant can index and filter them.
    let params = GitHubPayloadParams {
        repo: "axon_rust".into(),
        owner: "jmagar".into(),
        content_kind: "file".into(),
        stars: Some(42),
        forks: Some(7),
        language: Some("Rust".into()),
        topics: Some(vec!["cli".into(), "rag".into()]),
        is_fork: Some(false),
        is_archived: Some(false),
        file_type: Some("source".into()),
        gh_line_start: Some(10),
        gh_line_end: Some(50),
        ..Default::default()
    };
    let payload = build_github_payload(&params);

    // Top-level assertions — these must exist as flat keys.
    assert_eq!(payload["gh_stars"], 42, "gh_stars must be a top-level key");
    assert_eq!(payload["gh_forks"], 7, "gh_forks must be a top-level key");
    assert_eq!(
        payload["gh_language"], "Rust",
        "gh_language must be a top-level key"
    );
    assert_eq!(
        payload["gh_topics"],
        json!(["cli", "rag"]),
        "gh_topics must be a top-level key"
    );
    assert_eq!(
        payload["gh_is_fork"], false,
        "gh_is_fork must be a top-level key"
    );
    assert_eq!(
        payload["gh_is_archived"], false,
        "gh_is_archived must be a top-level key"
    );
    assert_eq!(
        payload["gh_file_type"], "source",
        "gh_file_type must be a top-level key"
    );
    assert_eq!(
        payload["gh_line_start"], 10,
        "gh_line_start must be a top-level key"
    );
    assert_eq!(
        payload["gh_line_end"], 50,
        "gh_line_end must be a top-level key"
    );

    // git_meta assertions — git_meta exists (lower-priority extras) but must NOT
    // contain the promoted fields. Use as_object() to fail loudly if git_meta is
    // not a JSON object (which would indicate a structural regression).
    let meta = payload["git_meta"]
        .as_object()
        .expect("git_meta must be a JSON object");
    assert!(
        !meta.contains_key("stars"),
        "stars must not be stored in git_meta (found: {meta:?})"
    );
    assert!(
        !meta.contains_key("forks"),
        "forks must not be stored in git_meta (found: {meta:?})"
    );
    assert!(
        !meta.contains_key("language"),
        "language must not be stored in git_meta (found: {meta:?})"
    );
    assert!(
        !meta.contains_key("topics"),
        "topics must not be stored in git_meta (found: {meta:?})"
    );
    assert!(
        !meta.contains_key("is_fork"),
        "is_fork must not be stored in git_meta (found: {meta:?})"
    );
    assert!(
        !meta.contains_key("is_archived"),
        "is_archived must not be stored in git_meta (found: {meta:?})"
    );
    assert!(
        !meta.contains_key("gh_file_type"),
        "gh_file_type must not be stored in git_meta (found: {meta:?})"
    );
    assert!(
        !meta.contains_key("gh_line_start"),
        "gh_line_start must not be stored in git_meta (found: {meta:?})"
    );
    assert!(
        !meta.contains_key("gh_line_end"),
        "gh_line_end must not be stored in git_meta (found: {meta:?})"
    );
}
