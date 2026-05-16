use super::super::GitHubCommonFields;
use super::clone::should_retry_unauthenticated_clone;
use super::prepare::next_search_start;
use crate::vector::ops::input::{chunk_text, code::chunk_code};

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
fn search_start_stays_on_char_boundary_with_multibyte_content() {
    let mut text = String::new();
    text.push_str(&"a".repeat(2000));
    text.push_str("─".repeat(200).as_str());
    text.push_str(&"b".repeat(500));

    let mut search_start = 0usize;
    for chunk in &chunk_text(&text) {
        let byte_offset = text[search_start..]
            .find(chunk.as_str())
            .map(|pos| search_start + pos)
            .unwrap_or(search_start);
        search_start = next_search_start(&text, byte_offset, chunk.len());
        assert!(
            text.is_char_boundary(search_start),
            "search_start {search_start} is not a char boundary"
        );
    }
}

#[test]
fn chunk_code_unknown_ext_falls_back() {
    if let Some(chunks) = chunk_code(&"hello world ".repeat(200), "unknownext") {
        assert!(chunks.iter().all(|chunk| chunk.len() <= 2200));
    }
}

#[test]
fn unauthenticated_clone_retry_respects_visibility_and_auth_errors() {
    assert!(!should_retry_unauthenticated_clone(
        &github_common(Some(true)),
        "remote: Repository not found.\nfatal: Authentication failed",
    ));
    assert!(!should_retry_unauthenticated_clone(
        &github_common(Some(false)),
        "remote: Invalid username or token.\nfatal: Authentication failed",
    ));
    assert!(should_retry_unauthenticated_clone(
        &github_common(Some(false)),
        "error: RPC failed; curl 56 GnuTLS recv error",
    ));
    assert!(!should_retry_unauthenticated_clone(
        &github_common(None),
        "remote: Permission to owner/repo.git denied to user.",
    ));
}
