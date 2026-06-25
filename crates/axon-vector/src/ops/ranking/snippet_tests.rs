use super::*;

// Build a minimal AskCandidate for testing snippet functions.
fn make_candidate(url: &str, chunk_text: &str) -> AskCandidate {
    use std::collections::HashSet;
    AskCandidate {
        score: 0.9,
        url: url.to_string(),
        path: "/docs/test".to_string(),
        chunk_text: chunk_text.to_string(),
        url_tokens: HashSet::new(),
        chunk_tokens: HashSet::new(),
        rerank_score: 0.9,
    }
}

// ── get_meaningful_snippet ────────────────────────────────────────────────────

#[test]
fn snippet_returns_string_for_empty_input() {
    let result = get_meaningful_snippet("", &[]);
    // Should not panic; may be empty or a short fallback.
    let _ = result;
}

#[test]
fn snippet_extracts_relevant_sentence() {
    let text = "The quick brown fox jumps over the lazy dog. \
                Axon is a fast RAG engine. \
                Another sentence here for padding purposes.";
    let tokens = vec!["axon".to_string(), "rag".to_string()];
    let snippet = get_meaningful_snippet(text, &tokens);
    assert!(
        snippet.contains("Axon") || snippet.contains("axon") || snippet.contains("RAG"),
        "expected relevant sentence in snippet, got: {snippet:?}"
    );
}

#[test]
fn snippet_no_tokens_returns_first_sentences() {
    let text = "First useful sentence about embedding documents. \
                Second useful sentence about vector search. \
                Third useful sentence about ranking results.";
    let snippet = get_meaningful_snippet(text, &[]);
    assert!(!snippet.is_empty(), "empty snippet from non-empty input");
}

#[test]
fn snippet_strips_markdown_links() {
    let text = "Click [here](https://example.com) to read more about embedding. \
                The quick brown fox jumps over the lazy dog with ease.";
    let tokens = vec!["embedding".to_string()];
    let snippet = get_meaningful_snippet(text, &tokens);
    // Snippet should not contain raw markdown link syntax.
    assert!(
        !snippet.contains("](https"),
        "raw markdown link leaked into snippet: {snippet:?}"
    );
}

#[test]
fn snippet_multibyte_chars_no_panic() {
    // Chunk text with multibyte UTF-8 characters.  The snippet extractor must
    // not slice on non-char-boundary bytes.
    let text = "café au lait est une boisson populaire en France. \
                Le système de recherche vectorielle est rapide et précis. \
                Björk est une artiste islandaise célèbre dans le monde entier.";
    let tokens = vec!["café".to_string(), "système".to_string()];
    // Must not panic regardless of content.
    let snippet = get_meaningful_snippet(text, &tokens);
    assert!(
        snippet.is_ascii()
            || snippet.chars().all(|c| c.is_alphanumeric()
                || c.is_whitespace()
                || c == '.'
                || c == ','
                || c == '\''
                || c == '-'
                || !c.is_ascii()),
        "snippet should be valid UTF-8 text: {snippet:?}"
    );
}

#[test]
fn snippet_multibyte_highlight_window_no_panic() {
    // This specifically tests the case where a multibyte char (e.g. 'é', 'ü', '日')
    // falls at or near a 220-char truncation boundary in the fallback path.
    // The char_indices-based truncation in the fallback must not panic.
    let mut text = "x".repeat(215);
    text.push_str("日本語テスト"); // multibyte chars at the boundary
    text.push_str(" more content here for the fallback path to use.");
    let snippet = get_meaningful_snippet(&text, &[]);
    // Should not panic; result is valid UTF-8.
    assert!(std::str::from_utf8(snippet.as_bytes()).is_ok());
}

#[test]
fn snippet_multibyte_only_text() {
    // All-multibyte text should not cause panics or invalid UTF-8 output.
    let text = "日本語のテキストはベクトル検索に使用されます。\
                エンベディングモデルはテキストを数値に変換します。\
                Qdrantはベクトルデータベースです。";
    let tokens = vec!["qdrant".to_string()];
    let snippet = get_meaningful_snippet(text, &tokens);
    assert!(std::str::from_utf8(snippet.as_bytes()).is_ok());
}

// ── select_best_preview_chunk ─────────────────────────────────────────────────

#[test]
fn select_best_preview_chunk_returns_fallback_for_no_url_match() {
    let candidates = vec![
        make_candidate(
            "https://example.com/a",
            "Some text about embedding and search.",
        ),
        make_candidate(
            "https://example.com/b",
            "Other text about vector databases.",
        ),
    ];
    let tokens = vec!["axon".to_string()];
    // Searching for url /c which doesn't exist → should return fallback_idx.
    let idx = select_best_preview_chunk(&candidates, "https://example.com/c", &tokens, 0);
    assert_eq!(idx, 0, "should fall back to index 0");
}

#[test]
fn select_best_preview_chunk_prefers_relevant_chunk() {
    let candidates = vec![
        make_candidate("https://example.com/docs", "Navigation. Prev. Next. Home."),
        make_candidate(
            "https://example.com/docs",
            "Axon is a fast RAG pipeline that embeds documents into Qdrant for hybrid search.",
        ),
    ];
    let tokens = vec!["axon".to_string(), "rag".to_string()];
    let idx = select_best_preview_chunk(&candidates, "https://example.com/docs", &tokens, 0);
    // Chunk at index 1 has more relevant prose; should win over nav boilerplate.
    assert_eq!(idx, 1, "should prefer the prose-rich chunk at index 1");
}

#[test]
fn select_best_preview_chunk_multibyte_url_and_text() {
    // Ensure multibyte content in chunk_text doesn't cause issues.
    let candidates = vec![make_candidate(
        "https://example.com/日本語",
        "日本語のドキュメントです。RAG検索に使用されます。これは重要な情報です。",
    )];
    let tokens = vec![];
    // Should not panic.
    let idx = select_best_preview_chunk(&candidates, "https://example.com/日本語", &tokens, 0);
    assert_eq!(idx, 0);
}
