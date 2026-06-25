use super::super::GitHubCommonFields;
use super::clone::should_retry_unauthenticated_clone;
use axon_vector::ops::input::{chunk_text, chunk_text_with_offsets, code::chunk_code};

fn github_common(is_private: Option<bool>) -> GitHubCommonFields {
    GitHubCommonFields {
        owner: "owner".to_string(),
        name: "repo".to_string(),
        repo_slug: "owner/repo".to_string(),
        default_branch: "main".to_string(),
        repo_description: None,
        pushed_at: None,
        is_private,
        has_wiki: false,
    }
}

#[test]
fn chunk_text_produces_bounded_content() {
    let chunks = chunk_text(&"x".repeat(5000));
    assert!(chunks.iter().all(|chunk| chunk.len() <= 2200));
    assert!(chunks.len() > 1);
}

#[test]
fn chunk_offsets_stay_on_char_boundaries_with_multibyte_content() {
    let mut text = String::new();
    text.push_str(&"a".repeat(2000));
    text.push_str("─".repeat(200).as_str());
    text.push_str(&"b".repeat(500));

    for (byte_offset, chunk) in chunk_text_with_offsets(&text) {
        assert!(
            text.is_char_boundary(byte_offset),
            "offset {byte_offset} is not a char boundary"
        );
        assert_eq!(
            &text[byte_offset..byte_offset + chunk.len()],
            chunk,
            "offset must point at the chunk's true position"
        );
    }
}

#[test]
fn chunk_code_unknown_ext_falls_back() {
    // Unknown extensions return None (caller should fall back to chunk_text).
    // This asserts the contract explicitly rather than passing vacuously on None.
    let result = chunk_code(&"hello world ".repeat(200), "unknownext");
    assert!(
        result.is_none(),
        "chunk_code should return None for an unknown extension so callers fall back to chunk_text"
    );
}

#[test]
fn unauthenticated_clone_retry_respects_visibility_and_auth_errors() {
    // Private repos: never retry unauthenticated regardless of error type.
    assert!(!should_retry_unauthenticated_clone(
        &github_common(Some(true)),
        "remote: Repository not found.\nfatal: Authentication failed",
    ));
    // Public repo + auth/token failure → retry without token (token is invalid/over-scoped
    // but the public repo is still accessible without any auth).
    assert!(should_retry_unauthenticated_clone(
        &github_common(Some(false)),
        "remote: Invalid username or token.\nfatal: Authentication failed",
    ));
    // Public repo + non-auth error (network failure) → don't retry; removing auth won't help.
    assert!(!should_retry_unauthenticated_clone(
        &github_common(Some(false)),
        "error: RPC failed; curl 56 GnuTLS recv error",
    ));
    // Unknown visibility + permission error → retry unauthenticated (repo may be public
    // and the token is invalid or over-scoped).
    assert!(should_retry_unauthenticated_clone(
        &github_common(None),
        "remote: Permission to owner/repo.git denied to user.",
    ));
}
