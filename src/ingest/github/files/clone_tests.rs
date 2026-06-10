//! T-L1: Tests for branch-ref validation (S-M4) and sanitized_git_stderr.

use super::*;

// ── validate_branch_ref ─────────────────────────────────────────────────────

#[test]
fn valid_branch_refs_pass() {
    for branch in &[
        "main",
        "develop",
        "feat/my-feature",
        "release/v1.2.3",
        "v2.0",
        "HEAD",
        "refs/heads/main",
    ] {
        assert!(
            validate_branch_ref(branch).is_ok(),
            "expected {branch:?} to be valid"
        );
    }
}

#[test]
fn empty_ref_is_rejected() {
    assert!(
        validate_branch_ref("").is_err(),
        "empty ref must be rejected"
    );
}

#[test]
fn leading_dash_is_rejected() {
    assert!(
        validate_branch_ref("--upload-pack=evil").is_err(),
        "leading '-' must be rejected to prevent option injection"
    );
    assert!(
        validate_branch_ref("-HEAD").is_err(),
        "single leading '-' must be rejected"
    );
}

#[test]
fn double_dot_is_rejected() {
    assert!(
        validate_branch_ref("master..evil").is_err(),
        "'..' must be rejected (git range / path traversal)"
    );
    assert!(
        validate_branch_ref("../etc/passwd").is_err(),
        "'../' path traversal must be rejected"
    );
}

#[test]
fn nul_byte_is_rejected() {
    assert!(
        validate_branch_ref("main\0malicious").is_err(),
        "NUL byte must be rejected"
    );
}

// ── sanitized_git_stderr ────────────────────────────────────────────────────

#[test]
fn token_is_redacted_from_stderr() {
    let token = "supersecrettoken";
    let raw = format!("https://x-oauth-basic:{token}@github.com/...: fatal error")
        .as_bytes()
        .to_vec();
    let out = sanitized_git_stderr(&raw, Some(token));
    assert!(!out.contains(token), "token must be redacted from stderr");
    assert!(
        out.contains("[redacted]"),
        "redacted placeholder must appear"
    );
}

#[test]
fn empty_token_leaves_stderr_unchanged() {
    let raw = b"fatal: repository not found".to_vec();
    let out = sanitized_git_stderr(&raw, Some(""));
    assert_eq!(out, "fatal: repository not found");
}

#[test]
fn none_token_leaves_stderr_unchanged() {
    let raw = b"fatal: repository not found".to_vec();
    let out = sanitized_git_stderr(&raw, None);
    assert_eq!(out, "fatal: repository not found");
}

// ── should_retry_unauthenticated_clone ──────────────────────────────────────

#[test]
fn known_private_repo_never_retries() {
    let common = GitHubCommonFields {
        owner: "o".to_string(),
        name: "r".to_string(),
        repo_slug: "o/r".to_string(),
        default_branch: "main".to_string(),
        repo_description: None,
        pushed_at: None,
        is_private: Some(true),
        has_wiki: false,
    };
    assert!(
        !should_retry_unauthenticated_clone(&common, "authentication failed"),
        "known-private repos must not retry unauthenticated"
    );
}

#[test]
fn known_public_repo_retries_on_auth_failure() {
    let common = GitHubCommonFields {
        owner: "o".to_string(),
        name: "r".to_string(),
        repo_slug: "o/r".to_string(),
        default_branch: "main".to_string(),
        repo_description: None,
        pushed_at: None,
        is_private: Some(false),
        has_wiki: false,
    };
    assert!(
        should_retry_unauthenticated_clone(&common, "authentication failed"),
        "known-public repos with auth failure should retry unauthenticated"
    );
}
