use super::*;

#[test]
fn complexity_hint_matches_use_dual_signal() {
    let simple = build_query_forms("rust");
    assert_eq!(simple.complexity_hint, QueryComplexity::Simple);
    assert!(!simple.use_dual);

    let complex = build_query_forms("how do PreToolUse hook fields work in Claude Code");
    assert_eq!(complex.complexity_hint, QueryComplexity::Complex);
    assert!(complex.use_dual);
}

#[test]
fn full_docs_default_simple_is_four() {
    assert_eq!(QueryComplexity::Simple.full_docs_default(), 4);
}

#[test]
fn full_docs_default_complex_is_six() {
    assert_eq!(QueryComplexity::Complex.full_docs_default(), 6);
}

// -- dual-embedding gate boundary tests (TEST-M3) --

/// The keyword-form / dual-embedding pass requires 3+ non-stop tokens. At exactly
/// two tokens it must NOT engage (`use_dual == false`).
#[test]
fn use_dual_false_at_two_tokens() {
    let forms = build_query_forms("rust macros");
    assert_eq!(
        forms.query_tokens.len(),
        2,
        "expected exactly two tokens, got {:?}",
        forms.query_tokens
    );
    assert!(
        !forms.use_dual,
        "two-token query must not trigger dual embedding"
    );
    assert_eq!(forms.complexity_hint, QueryComplexity::Simple);
}

/// At three-plus non-stop tokens (and an NL form that differs from the keyword
/// form), the dual pass engages. `tokenize_query` strips stop words and very
/// short tokens, so the query needs 3+ content tokens plus filler that the
/// keyword form drops.
#[test]
fn use_dual_true_at_three_tokens() {
    let forms = build_query_forms("how to configure hybrid qdrant search");
    // Stop words ("how", "to") drop out, leaving 4 content tokens; the keyword
    // form ("configure hybrid qdrant search") differs from the trimmed NL form.
    assert!(
        forms.query_tokens.len() >= 3,
        "expected 3+ content tokens, got {:?}",
        forms.query_tokens
    );
    assert_ne!(
        forms.keyword_query.to_lowercase(),
        "how to configure hybrid qdrant search",
        "keyword form must differ from the NL form for dual to engage"
    );
    assert!(
        forms.use_dual,
        "3+ token NL query that differs from its keyword form must use dual embedding"
    );
    assert_eq!(forms.complexity_hint, QueryComplexity::Complex);
}

/// When the query is already keyword-shaped (NL form == keyword form after
/// tokenization), the second dispatch is skipped even with 3+ tokens.
#[test]
fn use_dual_false_when_already_keyword_shaped() {
    // Three content tokens, no stop words / punctuation: keyword form is
    // identical to the trimmed lowercase NL form, so dual is pointless.
    let forms = build_query_forms("qdrant vector search");
    assert_eq!(forms.query_tokens.len(), 3);
    assert_eq!(
        forms.keyword_query.to_lowercase(),
        "qdrant vector search".to_lowercase(),
        "keyword form should match the NL form for an already-keyword-shaped query"
    );
    assert!(
        !forms.use_dual,
        "already-keyword-shaped query must skip the dual dispatch"
    );
}

/// The keyword form is document-shaped and must NOT carry the asymmetric
/// `QUERY_INSTRUCTION` prefix (prepending it would push the keyword vector into
/// query space and defeat the dual-embedding pass — a documented 2-3s/ask
/// regression source).
#[test]
fn keyword_query_does_not_carry_query_instruction() {
    use crate::ops::tei::QUERY_INSTRUCTION;
    let forms = build_query_forms("how do PreToolUse hook fields work in Claude Code");
    assert!(forms.use_dual, "this query should engage dual embedding");
    assert!(
        !forms.keyword_query.contains(QUERY_INSTRUCTION),
        "keyword form must not embed the QUERY_INSTRUCTION prefix, got: {:?}",
        forms.keyword_query
    );
    assert!(
        !forms.keyword_query.starts_with(QUERY_INSTRUCTION),
        "keyword form must not be prefixed with QUERY_INSTRUCTION"
    );
}
