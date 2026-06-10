use std::collections::HashSet;
use std::sync::LazyLock;

/// Generic topical tokens: high-frequency documentation vocabulary that is not
/// specific to any product or library. Used by the authority-boost gate to avoid
/// treating queries like "setup guide" as product-specific.
///
/// `LazyLock<HashSet>` gives O(1) membership tests (same pattern as `STOP_WORDS`
/// in `sparse.rs`); the previous `&[&str]` slice required O(n) linear scan.
static GENERIC_TOPICAL_TOKENS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "api",
        "app",
        "book",
        "build",
        "cli",
        "code",
        "command",
        "commands",
        "config",
        "create",
        "documentation",
        "error",
        "errors",
        "find",
        "docs",
        "guide",
        "guides",
        "handling",
        "install",
        "dependency",
        "dependencies",
        "manage",
        "management",
        "marketplace",
        "package",
        "packages",
        "plugin",
        "plugins",
        "publish",
        "publishing",
        "reference",
        "registry",
        "setup",
        "structure",
        "structured",
        "structuring",
        "tool",
        "tools",
        "using",
        "view",
        "views",
    ]
    .into_iter()
    .collect()
});

const LANGUAGE_IDENTITY_TOKENS: &[&str] = &[
    "java",
    "javascript",
    "js",
    "go",
    "node",
    "nodejs",
    "py",
    "python",
    "rs",
    "rust",
    "ts",
    "typescript",
];

#[must_use]
pub fn query_tokens(text: &str) -> Vec<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 2 && !is_query_stop_word(token))
        .map(str::to_string)
        .collect()
}

#[must_use]
pub fn identity_tokens(text: &str) -> HashSet<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 2)
        .map(str::to_string)
        .collect()
}

#[must_use]
pub fn is_generic_authority_token(token: &str) -> bool {
    is_generic_topical_token(token) || LANGUAGE_IDENTITY_TOKENS.contains(&token)
}

#[must_use]
pub fn is_generic_topical_token(token: &str) -> bool {
    GENERIC_TOPICAL_TOKENS.contains(token)
}

fn is_query_stop_word(token: &str) -> bool {
    super::sparse::STOP_WORDS.contains(token)
}

#[cfg(test)]
#[path = "token_policy_tests.rs"]
mod tests;
