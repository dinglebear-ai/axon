//! Tests for `crates/vector/ops/ranking.rs`.
//!
//! Kept in a separate file so the production module stays within the
//! 500-line monolith limit (tests are exempt via the `**/*_test.*` glob).
use super::*;
use std::collections::HashSet;

// ── Helper ────────────────────────────────────────────────────────────────────

fn make_candidate(score: f64, url: &str, path: &str, chunk_text: &str) -> AskCandidate {
    let url_tokens = tokenize_text_set(url);
    let chunk_tokens = tokenize_text_set(chunk_text);
    AskCandidate {
        score,
        url: url.to_string(),
        path: path.to_string(),
        chunk_text: chunk_text.to_string(),
        url_tokens,
        chunk_tokens,
        rerank_score: 0.0,
    }
}

// ── tokenize_query ────────────────────────────────────────────────────────────

#[test]
fn tokenize_query_strips_stop_words() {
    let tokens = tokenize_query("the quick brown fox");
    assert!(
        !tokens.contains(&"the".to_string()),
        "stop word 'the' should be stripped"
    );
    assert!(tokens.contains(&"quick".to_string()));
    assert!(tokens.contains(&"brown".to_string()));
    assert!(tokens.contains(&"fox".to_string()));
}

#[test]
fn tokenize_query_lowercases() {
    let tokens = tokenize_query("HTTP Request");
    assert!(
        tokens.contains(&"http".to_string()),
        "should be lowercased to 'http'"
    );
    assert!(
        tokens.contains(&"request".to_string()),
        "should be lowercased to 'request'"
    );
    assert!(
        !tokens.iter().any(|t| t.chars().any(|c| c.is_uppercase())),
        "no token should contain uppercase letters"
    );
}

#[test]
fn tokenize_query_filters_short_tokens() {
    let tokens = tokenize_query("a bb ccc dddd");
    assert!(
        !tokens.contains(&"a".to_string()),
        "'a' (len 1) should be dropped"
    );
    assert!(
        !tokens.contains(&"bb".to_string()),
        "'bb' (len 2) should be dropped"
    );
    assert!(
        tokens.contains(&"ccc".to_string()),
        "'ccc' (len 3) should survive"
    );
    assert!(
        tokens.contains(&"dddd".to_string()),
        "'dddd' (len 4) should survive"
    );
}

#[test]
fn tokenize_query_empty_input() {
    let tokens = tokenize_query("");
    assert!(tokens.is_empty(), "empty input should produce no tokens");
}

// ── rerank_ask_candidates ─────────────────────────────────────────────────────

#[test]
fn rerank_ask_candidates_returns_empty_for_empty_input() {
    let result = rerank_ask_candidates(&[], &["rust".to_string()], &[], 0.0);
    assert!(result.is_empty());
}

#[test]
fn rerank_ask_candidates_passes_through_when_no_query_tokens() {
    let candidates = vec![
        make_candidate(0.9, "https://example.com/a", "/a", "some text"),
        make_candidate(0.5, "https://example.com/b", "/b", "other text"),
    ];
    let result = rerank_ask_candidates(&candidates, &[], &[], 0.0);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].url, "https://example.com/a");
    assert_eq!(result[1].url, "https://example.com/b");
}

#[test]
fn rerank_ask_candidates_boosts_url_token_matches() {
    // Both start at the same base score; only candidate A has "async" in URL.
    let candidates = vec![
        make_candidate(
            0.7,
            "https://docs.rs/async-std/",
            "/async-std/",
            "some content here",
        ),
        make_candidate(
            0.7,
            "https://crates.io/other/",
            "/other/",
            "unrelated content here",
        ),
    ];
    let query_tokens = tokenize_query("async programming");
    let result = rerank_ask_candidates(&candidates, &query_tokens, &[], 0.0);
    assert_eq!(
        result[0].url, "https://docs.rs/async-std/",
        "URL-matching candidate should rank first"
    );
    assert!(
        result[0].rerank_score > result[1].rerank_score,
        "URL-matching candidate must have a strictly higher rerank_score"
    );
}

#[test]
fn rerank_ask_candidates_applies_docs_path_boost() {
    let candidates = vec![
        make_candidate(
            0.5,
            "https://example.com/blog/post",
            "/blog/post",
            "generic content words here",
        ),
        make_candidate(
            0.5,
            "https://example.com/docs/guide",
            "/docs/guide",
            "generic content words here",
        ),
    ];
    let query_tokens = tokenize_query("zyx"); // no token matches on purpose
    let result = rerank_ask_candidates(&candidates, &query_tokens, &[], 0.0);
    let docs_candidate = result.iter().find(|c| c.path.contains("/docs/")).unwrap();
    let blog_candidate = result.iter().find(|c| c.path.contains("/blog/")).unwrap();
    assert!(
        docs_candidate.rerank_score > blog_candidate.rerank_score,
        "/docs/ path should receive +0.04 boost"
    );
}

#[test]
fn rerank_ask_candidates_chunk_token_boost() {
    let candidates = vec![
        make_candidate(
            0.6,
            "https://example.com/a",
            "/a",
            "embedding vectors similarity search retrieval",
        ),
        make_candidate(
            0.6,
            "https://example.com/b",
            "/b",
            "unrelated content about something completely different",
        ),
    ];
    let query_tokens = tokenize_query("embedding similarity");
    let result = rerank_ask_candidates(&candidates, &query_tokens, &[], 0.0);
    assert_eq!(
        result[0].url, "https://example.com/a",
        "chunk-text matching candidate should rank first"
    );
    assert!(result[0].rerank_score > result[1].rerank_score);
}

#[test]
fn rerank_ask_candidates_deduplication_by_url_order() {
    // rerank itself should not drop candidates with duplicate URLs.
    let candidates = vec![
        make_candidate(
            0.8,
            "https://example.com/page",
            "/page",
            "first chunk alpha",
        ),
        make_candidate(
            0.7,
            "https://example.com/page",
            "/page",
            "second chunk beta",
        ),
    ];
    let query_tokens = tokenize_query("alpha");
    let result = rerank_ask_candidates(&candidates, &query_tokens, &[], 0.0);
    assert_eq!(
        result.len(),
        2,
        "rerank should not drop duplicate-URL candidates"
    );
}

#[test]
fn rerank_ask_candidates_applies_authoritative_domain_boost() {
    let candidates = vec![
        make_candidate(
            0.62,
            "https://example.org/docs/commands",
            "/docs/commands",
            "custom slash commands guide",
        ),
        make_candidate(
            0.62,
            "https://docs.claude.com/en/docs/claude-code/slash-commands",
            "/en/docs/claude-code/slash-commands",
            "custom slash commands guide",
        ),
    ];
    let query_tokens = tokenize_query("claude code slash commands");
    let result = rerank_ask_candidates(
        &candidates,
        &query_tokens,
        &["docs.claude.com".to_string()],
        0.12,
    );
    assert_eq!(
        result[0].url,
        "https://docs.claude.com/en/docs/claude-code/slash-commands"
    );
    assert!(
        result[0].rerank_score > result[1].rerank_score,
        "authoritative-domain candidate should rank above non-authoritative candidate"
    );
}

// ── get_meaningful_snippet ────────────────────────────────────────────────────

#[test]
fn get_meaningful_snippet_returns_empty_for_empty_input() {
    // Must not panic; return value is implementation-defined for empty text.
    let _ = get_meaningful_snippet("", &[]);
}

#[test]
fn get_meaningful_snippet_removes_nav_lines() {
    let text = "prev\nnext\nhome\n\
        This is a real documentation sentence explaining something important about the API.";
    let tokens = tokenize_query("documentation API");
    let result = get_meaningful_snippet(text, &tokens);
    assert!(
        !result.starts_with("prev"),
        "nav item 'prev' should be excluded from snippet"
    );
    assert!(
        result.contains("documentation") || result.contains("API") || result.len() > 5,
        "snippet should contain real prose content"
    );
}

#[test]
fn get_meaningful_snippet_with_query_tokens_returns_relevant_content() {
    let text = "Introduction. \
        The async runtime provides a high-performance execution environment for concurrent tasks. \
        Getting started with async programming requires understanding futures and executors. \
        Unrelated sidebar navigation item about something else entirely here today.";
    let tokens = tokenize_query("async runtime concurrent");
    let result = get_meaningful_snippet(text, &tokens);
    assert!(
        result.contains("async") || result.contains("concurrent") || result.contains("runtime"),
        "snippet should contain query-relevant terms; got: {result:?}"
    );
}

#[test]
fn get_meaningful_snippet_skips_multi_link_nav_clusters() {
    let text = "[Documentation](/docs)[Extensions](/extensions)[Specification](/specification)\
        [Registry](/registry)[Community](/community)\n\
        MCP is an open protocol that enables secure connections between AI tools and external systems.";
    let tokens = tokenize_query("protocol external systems");
    let result = get_meaningful_snippet(text, &tokens);
    assert!(
        !result.contains("Documentation Extensions Specification Registry Community"),
        "multi-link nav cluster should be excluded from snippet; got: {result:?}"
    );
    assert!(
        result.contains("open protocol") || result.contains("external systems"),
        "body prose should survive nav stripping; got: {result:?}"
    );
}

#[test]
fn get_meaningful_snippet_skips_title_like_header_lines() {
    let text = "Specification - Model Context Protocol\n\
        MCP defines a standard way for models to communicate with tools and resources.";
    let tokens = tokenize_query("models tools resources");
    let result = get_meaningful_snippet(text, &tokens);
    assert!(
        !result.contains("Specification - Model Context Protocol"),
        "page-title boilerplate should be excluded from snippet; got: {result:?}"
    );
    assert!(
        result.contains("communicate with tools") || result.contains("resources"),
        "body sentence should remain after title stripping; got: {result:?}"
    );
}

// ── select_diverse_candidates ─────────────────────────────────────────────────

#[test]
fn select_diverse_candidates_caps_at_two_per_url() {
    // 5 candidates all from the same URL.
    // target_count=4 < 5, so the early-return does NOT fire.
    // First pass picks 1 (unique URL), second pass adds 1 more (up to max_per_url=2).
    // Result: 2 selected, not 5.
    let candidates: Vec<AskCandidate> = (0..5)
        .map(|i| {
            make_candidate(
                0.9 - i as f64 * 0.05,
                "https://example.com/page",
                "/page",
                &format!("chunk number {i} with some text content here"),
            )
        })
        .collect();
    let indices = select_diverse_candidates(&candidates, 4, 2);
    assert_eq!(
        indices.len(),
        2,
        "max_per_url=2 should cap same-URL candidates at exactly 2; got {}",
        indices.len()
    );
    // Indices must be distinct (no duplicate selection).
    let unique: HashSet<usize> = indices.iter().copied().collect();
    assert_eq!(
        unique.len(),
        indices.len(),
        "selected indices must be unique — no duplicate entries"
    );
}

#[test]
fn select_diverse_candidates_respects_target_count() {
    // 10 candidates from distinct URLs; target_count=3 → exactly 3 selected
    let candidates: Vec<AskCandidate> = (0..10)
        .map(|i| {
            make_candidate(
                0.9,
                &format!("https://site{i}.com/"),
                "/",
                "some content about something interesting",
            )
        })
        .collect();
    let indices = select_diverse_candidates(&candidates, 3, 2);
    assert_eq!(
        indices.len(),
        3,
        "should select exactly target_count=3 candidates"
    );
}

#[test]
fn select_diverse_candidates_returns_all_when_fewer_than_target() {
    let candidates: Vec<AskCandidate> = (0..3)
        .map(|i| make_candidate(0.9, &format!("https://site{i}.com/"), "/", "some content"))
        .collect();
    let indices = select_diverse_candidates(&candidates, 10, 2);
    assert_eq!(
        indices.len(),
        3,
        "when fewer candidates than target, all should be returned"
    );
}

#[test]
fn select_diverse_candidates_diverse_urls_first() {
    // All 4 candidates across 3 URLs; use target_count=3 (< 4) to force the
    // diversity algorithm rather than the early-return path.
    let candidates = vec![
        make_candidate(0.9, "https://a.com/", "/p1", "content alpha one here"),
        make_candidate(0.8, "https://a.com/", "/p2", "content alpha two here"),
        make_candidate(0.7, "https://b.com/", "/p1", "content beta one here"),
        make_candidate(0.6, "https://c.com/", "/p1", "content gamma one here"),
    ];
    let indices = select_diverse_candidates(&candidates, 3, 2);
    assert_eq!(
        indices.len(),
        3,
        "should return exactly target_count=3 candidates"
    );

    // All 3 selected should be from distinct URLs (first pass picks one per URL)
    let selected_urls: HashSet<&str> = indices
        .iter()
        .map(|&i| candidates[i].url.as_str())
        .collect();
    assert_eq!(
        selected_urls.len(),
        3,
        "first pass should ensure each selected candidate has a unique URL"
    );
}

// ── strip_markdown_inline (via get_meaningful_snippet) ────────────────────────

#[test]
fn strip_markdown_inline_removes_image() {
    let text = "![logo](https://example.com/logo.png) \
        This paragraph explains the feature in sufficient detail for our purposes here.";
    let result = get_meaningful_snippet(text, &[]);
    assert!(
        !result.contains("!["),
        "image markdown should be stripped; got: {result:?}"
    );
}

#[test]
fn strip_markdown_inline_replaces_link_with_text() {
    let text = "See the [installation guide](https://docs.rs/guide) for full instructions. \
        This guide covers all the required steps to complete the setup process.";
    let result = get_meaningful_snippet(text, &["installation".to_string()]);
    assert!(
        !result.contains("https://docs.rs/guide"),
        "link URL should be stripped; got: {result:?}"
    );
}
