use super::*;

fn req(
    kind: &str,
    owner: &str,
    repo: Option<&str>,
    branch: Option<&str>,
    path: Option<&str>,
) -> GitHubBrowseRequest {
    GitHubBrowseRequest {
        kind: kind.to_string(),
        owner: owner.to_string(),
        repo: repo.map(str::to_string),
        branch: branch.map(str::to_string),
        path: path.map(str::to_string),
    }
}

#[test]
fn parses_known_kinds() {
    assert_eq!(parse_kind("repos").unwrap(), GitHubRequestKind::ListRepos);
    assert_eq!(parse_kind("repo").unwrap(), GitHubRequestKind::RepoInfo);
    assert_eq!(parse_kind("tree").unwrap(), GitHubRequestKind::Tree);
    assert_eq!(parse_kind("file").unwrap(), GitHubRequestKind::FileContents);
}

#[test]
fn rejects_unknown_kind() {
    assert!(parse_kind("bogus").is_err());
}

// Note: `GitHubRequestKind::Feed` is intentionally NOT exercised against
// `build_request_url` here — that arm is unreachable in production (see its
// `unreachable!()` body and doc comment above) because `github_browse`
// special-cases `Feed` and routes to `github_feed::github_browse_feed`
// before `build_request_url` is ever called for that kind. Feed's URL
// construction is covered by `github_feed_tests.rs` instead.

#[test]
fn parse_kind_accepts_feed() {
    assert_eq!(parse_kind("feed").unwrap(), GitHubRequestKind::Feed);
}

#[test]
fn builds_list_repos_url() {
    let request = req("repos", "jmagar", None, None, None);
    let url = build_request_url(&request, GitHubRequestKind::ListRepos).unwrap();
    assert_eq!(
        url,
        "https://api.github.com/users/jmagar/repos?sort=updated&per_page=50"
    );
}

#[test]
fn builds_repo_info_url() {
    let request = req("repo", "jmagar", Some("axon"), None, None);
    let url = build_request_url(&request, GitHubRequestKind::RepoInfo).unwrap();
    assert_eq!(url, "https://api.github.com/repos/jmagar/axon");
}

#[test]
fn builds_tree_url_with_default_branch() {
    let request = req("tree", "jmagar", Some("axon"), None, None);
    let url = build_request_url(&request, GitHubRequestKind::Tree).unwrap();
    assert_eq!(
        url,
        "https://api.github.com/repos/jmagar/axon/git/trees/main?recursive=1"
    );
}

#[test]
fn builds_tree_url_with_explicit_branch() {
    let request = req("tree", "jmagar", Some("axon"), Some("dev"), None);
    let url = build_request_url(&request, GitHubRequestKind::Tree).unwrap();
    assert_eq!(
        url,
        "https://api.github.com/repos/jmagar/axon/git/trees/dev?recursive=1"
    );
}

#[test]
fn builds_file_contents_url() {
    let request = req("file", "jmagar", Some("axon"), None, Some("src/main.rs"));
    let url = build_request_url(&request, GitHubRequestKind::FileContents).unwrap();
    assert_eq!(
        url,
        "https://api.github.com/repos/jmagar/axon/contents/src/main.rs"
    );
}

#[test]
fn builds_file_contents_url_with_branch_ref() {
    let request = req(
        "file",
        "jmagar",
        Some("axon"),
        Some("dev"),
        Some("src/main.rs"),
    );
    let url = build_request_url(&request, GitHubRequestKind::FileContents).unwrap();
    assert_eq!(
        url,
        "https://api.github.com/repos/jmagar/axon/contents/src/main.rs?ref=dev"
    );
}

#[test]
fn file_contents_path_percent_encodes_segments() {
    let request = req(
        "file",
        "jmagar",
        Some("axon"),
        None,
        Some("docs/my file.md"),
    );
    let url = build_request_url(&request, GitHubRequestKind::FileContents).unwrap();
    assert_eq!(
        url,
        "https://api.github.com/repos/jmagar/axon/contents/docs/my%20file.md"
    );
}

#[test]
fn rejects_missing_repo_for_repo_scoped_kinds() {
    let request = req("repo", "jmagar", None, None, None);
    assert!(build_request_url(&request, GitHubRequestKind::RepoInfo).is_err());

    let request = req("tree", "jmagar", None, None, None);
    assert!(build_request_url(&request, GitHubRequestKind::Tree).is_err());

    let request = req("file", "jmagar", None, None, Some("a.rs"));
    assert!(build_request_url(&request, GitHubRequestKind::FileContents).is_err());
}

#[test]
fn rejects_missing_path_for_file_kind() {
    let request = req("file", "jmagar", Some("axon"), None, None);
    assert!(build_request_url(&request, GitHubRequestKind::FileContents).is_err());
}

#[test]
fn rejects_owner_with_path_traversal() {
    let request = req("repos", "../etc", None, None, None);
    assert!(build_request_url(&request, GitHubRequestKind::ListRepos).is_err());
}

#[test]
fn rejects_owner_with_slash() {
    let request = req("repos", "jmagar/evil", None, None, None);
    assert!(build_request_url(&request, GitHubRequestKind::ListRepos).is_err());
}

#[test]
fn rejects_owner_with_scheme_injection() {
    let request = req("repos", "jmagar@evil.example", None, None, None);
    assert!(build_request_url(&request, GitHubRequestKind::ListRepos).is_err());
}

#[test]
fn rejects_empty_owner() {
    let request = req("repos", "", None, None, None);
    assert!(build_request_url(&request, GitHubRequestKind::ListRepos).is_err());
}

#[test]
fn rejects_file_path_with_traversal() {
    let request = req(
        "file",
        "jmagar",
        Some("axon"),
        None,
        Some("../../etc/passwd"),
    );
    assert!(build_request_url(&request, GitHubRequestKind::FileContents).is_err());
}

#[test]
fn rejects_file_path_with_leading_slash() {
    let request = req("file", "jmagar", Some("axon"), None, Some("/etc/passwd"));
    assert!(build_request_url(&request, GitHubRequestKind::FileContents).is_err());
}

#[test]
fn rejects_branch_with_query_injection() {
    let request = req("tree", "jmagar", Some("axon"), Some("main?x=1"), None);
    assert!(build_request_url(&request, GitHubRequestKind::Tree).is_err());
}

#[test]
fn describes_rate_limit_error_with_reset_time() {
    let payload = serde_json::json!({ "message": "API rate limit exceeded" });
    let message = describe_error_with_retry_after(
        reqwest::StatusCode::FORBIDDEN,
        Some(0),
        Some(1_700_000_000),
        None,
        &payload,
    );
    assert!(message.contains("rate limited"));
    assert!(message.contains("retry at"));
}

#[test]
fn describes_rate_limit_error_without_reset_time() {
    let payload = serde_json::json!({});
    let message = describe_error_with_retry_after(
        reqwest::StatusCode::FORBIDDEN,
        Some(0),
        None,
        None,
        &payload,
    );
    assert!(message.contains("rate limited"));
}

#[test]
fn describes_forbidden_without_exhausted_quota_as_generic_error() {
    let payload = serde_json::json!({ "message": "Resource not accessible" });
    let message = describe_error_with_retry_after(
        reqwest::StatusCode::FORBIDDEN,
        Some(10),
        None,
        None,
        &payload,
    );
    assert!(message.contains("Resource not accessible"));
    assert!(!message.contains("rate limited"));
}

#[test]
fn describes_secondary_rate_limit_with_retry_after_despite_nonzero_remaining() {
    // GitHub's secondary/abuse-detection limiter returns 403 with
    // `Retry-After` set but does NOT necessarily zero out the primary
    // `x-ratelimit-remaining` count — this must still be recognized as a
    // rate limit, not fall through to a generic 403 message.
    let payload = serde_json::json!({ "message": "You have exceeded a secondary rate limit" });
    let message = describe_error_with_retry_after(
        reqwest::StatusCode::FORBIDDEN,
        Some(42),
        None,
        Some(30),
        &payload,
    );
    assert!(message.contains("secondary rate limit"));
    assert!(message.contains("30"));
}

#[test]
fn describes_not_found_error() {
    let payload = serde_json::json!({});
    let message =
        describe_error_with_retry_after(reqwest::StatusCode::NOT_FOUND, None, None, None, &payload);
    assert_eq!(message, "not found on GitHub");
}

#[test]
fn describes_generic_error_with_github_message() {
    let payload = serde_json::json!({ "message": "Bad credentials" });
    let message = describe_error_with_retry_after(
        reqwest::StatusCode::UNAUTHORIZED,
        None,
        None,
        None,
        &payload,
    );
    assert!(message.contains("Bad credentials"));
}

#[test]
fn describes_generic_error_without_github_message() {
    let payload = serde_json::json!({});
    let message = describe_error_with_retry_after(
        reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        None,
        None,
        None,
        &payload,
    );
    assert!(message.contains("500"));
}

#[test]
fn truncates_oversized_file_content() {
    let huge = "a".repeat(MAX_FILE_PREVIEW_BYTES + 1);
    let payload = serde_json::json!({ "content": huge, "encoding": "base64" });
    let truncated = truncate_file_payload(payload);
    assert_eq!(truncated["content"], "");
    assert_eq!(truncated["truncated"], true);
}

#[test]
fn keeps_small_file_content_untouched() {
    let payload = serde_json::json!({ "content": "aGVsbG8=", "encoding": "base64" });
    let untouched = truncate_file_payload(payload.clone());
    assert_eq!(untouched, payload);
}

#[test]
fn format_unix_time_matches_known_value() {
    // 2023-11-14 22:13:20 UTC
    assert_eq!(format_unix_time(1_700_000_000), "2023-11-14 22:13:20 UTC");
}

#[test]
fn format_unix_time_handles_epoch() {
    assert_eq!(format_unix_time(0), "1970-01-01 00:00:00 UTC");
}
