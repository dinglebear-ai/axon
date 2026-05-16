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
