use super::*;

// ── query_tokens ──────────────────────────────────────────────────────────────

#[test]
fn query_tokens_basic_split() {
    let tokens = query_tokens("hello world");
    assert!(tokens.contains(&"hello".to_string()));
    assert!(tokens.contains(&"world".to_string()));
}

#[test]
fn query_tokens_lowercases_input() {
    let tokens = query_tokens("Rust TEI Embed");
    assert!(tokens.contains(&"rust".to_string()));
    assert!(tokens.contains(&"tei".to_string()));
    assert!(tokens.contains(&"embed".to_string()));
}

#[test]
fn query_tokens_filters_single_char() {
    // Tokens shorter than 2 chars are filtered.
    let tokens = query_tokens("a b c rust");
    assert!(!tokens.contains(&"a".to_string()));
    assert!(!tokens.contains(&"b".to_string()));
    assert!(!tokens.contains(&"c".to_string()));
    assert!(tokens.contains(&"rust".to_string()));
}

#[test]
fn query_tokens_removes_stop_words() {
    // Common stop words (e.g. "the", "is", "in") should be filtered.
    let tokens = query_tokens("the rust compiler is fast");
    assert!(!tokens.contains(&"the".to_string()));
    assert!(!tokens.contains(&"is".to_string()));
    assert!(tokens.contains(&"rust".to_string()));
    assert!(tokens.contains(&"compiler".to_string()));
    assert!(tokens.contains(&"fast".to_string()));
}

#[test]
fn query_tokens_empty_string() {
    assert!(query_tokens("").is_empty());
}

#[test]
fn query_tokens_all_stop_words_returns_empty() {
    // "the" and "is" are common stop words; result should be empty or very small.
    let tokens = query_tokens("the the the");
    assert!(
        tokens.iter().all(|t| t != "the"),
        "stop word 'the' leaked through: {tokens:?}"
    );
}

#[test]
fn query_tokens_non_alphanumeric_delimiters() {
    let tokens = query_tokens("embed/query-result.json");
    assert!(tokens.contains(&"embed".to_string()));
    assert!(tokens.contains(&"query".to_string()));
    assert!(tokens.contains(&"result".to_string()));
    assert!(tokens.contains(&"json".to_string()));
}

// ── identity_tokens ───────────────────────────────────────────────────────────

#[test]
fn identity_tokens_does_not_filter_stop_words() {
    // identity_tokens keeps all tokens ≥2 chars, including stop words.
    let tokens = identity_tokens("the rust compiler");
    assert!(tokens.contains("rust"));
    assert!(tokens.contains("compiler"));
    // "the" is ≥2 chars; identity_tokens does NOT strip stop words.
    assert!(tokens.contains("the"));
}

#[test]
fn identity_tokens_empty() {
    assert!(identity_tokens("").is_empty());
}

#[test]
fn identity_tokens_deduplicates() {
    let tokens = identity_tokens("rust rust rust");
    assert_eq!(tokens.len(), 1);
}

// ── is_generic_topical_token ──────────────────────────────────────────────────

#[test]
fn is_generic_topical_token_known_entries() {
    assert!(is_generic_topical_token("api"));
    assert!(is_generic_topical_token("guide"));
    assert!(is_generic_topical_token("docs"));
    assert!(is_generic_topical_token("setup"));
    assert!(is_generic_topical_token("plugin"));
}

#[test]
fn is_generic_topical_token_rejects_unknowns() {
    assert!(!is_generic_topical_token("qdrant"));
    assert!(!is_generic_topical_token("axon"));
    assert!(!is_generic_topical_token(""));
    assert!(!is_generic_topical_token("rust")); // language token, not topical
}

// ── is_generic_authority_token ────────────────────────────────────────────────

#[test]
fn is_generic_authority_token_accepts_topical_and_language() {
    // Topical tokens qualify.
    assert!(is_generic_authority_token("api"));
    assert!(is_generic_authority_token("docs"));
    // Language identity tokens also qualify.
    assert!(is_generic_authority_token("rust"));
    assert!(is_generic_authority_token("python"));
    assert!(is_generic_authority_token("typescript"));
    assert!(is_generic_authority_token("js"));
}

#[test]
fn is_generic_authority_token_rejects_product_specific() {
    assert!(!is_generic_authority_token("qdrant"));
    assert!(!is_generic_authority_token("axon"));
    assert!(!is_generic_authority_token("anthropic"));
}

// ── cap boundary ─────────────────────────────────────────────────────────────

#[test]
fn query_tokens_length_boundary() {
    // One-char token filtered, two-char token kept.
    let tokens = query_tokens("ax rust");
    assert!(
        tokens.contains(&"ax".to_string()),
        "two-char token should be kept"
    );
    // "ax" is not a stop word
}
