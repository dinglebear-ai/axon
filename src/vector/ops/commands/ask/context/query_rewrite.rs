use crate::vector::ops::ranking;

/// Coarse query-shape signal derived from the existing `use_dual` decision in
/// `build_query_forms`. Reused by the adaptive `ask_full_docs` resolver so the
/// classification surface stays single-sourced. (bd axon_rust-721)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum QueryComplexity {
    /// Short / single-keyword / already-keyword-shaped queries — `use_dual=false`.
    Simple,
    /// Multi-keyword non-trivial natural-language queries — `use_dual=true`.
    Complex,
}

impl QueryComplexity {
    /// Default `ask_full_docs` value when the user did not pin a value via
    /// `AXON_ASK_FULL_DOCS` or a CLI flag. Fewer full-doc fetches on simple
    /// queries cuts the largest non-LLM cost without hurting recall — they
    /// already get the answer from the top-ranked chunks.
    pub(crate) fn full_docs_default(self) -> usize {
        match self {
            QueryComplexity::Simple => 2,
            QueryComplexity::Complex => 3,
        }
    }
}

pub(super) struct AskQueryForms {
    pub(super) query_tokens: Vec<String>,
    pub(super) keyword_query: String,
    pub(super) use_dual: bool,
    /// Reuses the `use_dual` signal as a coarse complexity hint. See
    /// `QueryComplexity::full_docs_default` for the resolved adaptive value.
    /// (bd axon_rust-721)
    pub(super) complexity_hint: QueryComplexity,
}

pub(super) fn build_query_forms(query: &str) -> AskQueryForms {
    let query_tokens = ranking::tokenize_query(query);
    let keyword_query = query_tokens.join(" ");
    let use_dual =
        query_tokens.len() >= 3 && keyword_query.to_lowercase() != query.trim().to_lowercase();
    let complexity_hint = if use_dual {
        QueryComplexity::Complex
    } else {
        QueryComplexity::Simple
    };
    AskQueryForms {
        query_tokens,
        keyword_query,
        use_dual,
        complexity_hint,
    }
}

#[cfg(test)]
mod tests {
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
    fn full_docs_default_simple_is_two() {
        assert_eq!(QueryComplexity::Simple.full_docs_default(), 2);
    }

    #[test]
    fn full_docs_default_complex_is_three() {
        assert_eq!(QueryComplexity::Complex.full_docs_default(), 3);
    }
}
