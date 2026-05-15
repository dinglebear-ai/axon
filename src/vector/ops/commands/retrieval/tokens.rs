use crate::vector::ops::ranking;
use spider::url::Url;
use std::collections::HashSet;

pub(crate) fn product_authority_boost_for_url(
    url: &str,
    query_tokens: &[String],
    product_authority_boost: f64,
) -> f64 {
    if product_authority_boost <= 0.0 || query_tokens.is_empty() {
        return 0.0;
    }
    let Some(host) = host_from_url(url) else {
        return 0.0;
    };
    if !is_docs_like_url(&host, url) {
        return 0.0;
    }

    let identity_tokens = product_identity_tokens(url);
    let product_token_match = query_tokens.iter().any(|token| {
        !is_generic_authority_token(token.as_str()) && identity_tokens.contains(token.as_str())
    });
    if product_token_match {
        product_authority_boost
    } else {
        0.0
    }
}

fn host_from_url(url: &str) -> Option<String> {
    Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|h| h.to_ascii_lowercase()))
}

fn is_docs_like_url(host: &str, url: &str) -> bool {
    host == "docs.rs"
        || host.starts_with("docs.")
        || host.contains(".readthedocs.")
        || host.contains("developer")
        || url.contains("/documentation/")
        || url.contains("/docs/")
        || url.contains("/guides/")
        || url.contains("/guide/")
        || url.contains("/api/")
        || url.contains("/reference/")
        || url.contains("/book/")
        || url.contains("/learn/")
}

fn product_identity_tokens(url: &str) -> HashSet<String> {
    let Ok(parsed) = Url::parse(url) else {
        return HashSet::new();
    };
    let mut tokens = HashSet::new();
    if let Some(host) = parsed.host_str() {
        tokens.extend(tokenize_identity_segment(host));
    }
    for segment in parsed
        .path_segments()
        .into_iter()
        .flatten()
        .filter(|segment| !segment.is_empty())
        .take(2)
    {
        tokens.extend(tokenize_identity_segment(segment));
    }
    tokens
}

fn tokenize_identity_segment(text: &str) -> HashSet<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 2)
        .map(str::to_string)
        .collect()
}

fn is_generic_authority_token(token: &str) -> bool {
    matches!(
        token,
        "api"
            | "app"
            | "book"
            | "build"
            | "cli"
            | "code"
            | "command"
            | "commands"
            | "config"
            | "create"
            | "documentation"
            | "error"
            | "errors"
            | "find"
            | "docs"
            | "guide"
            | "guides"
            | "handling"
            | "install"
            | "java"
            | "javascript"
            | "js"
            | "dependency"
            | "dependencies"
            | "go"
            | "manage"
            | "management"
            | "marketplace"
            | "node"
            | "nodejs"
            | "package"
            | "packages"
            | "plugin"
            | "plugins"
            | "publish"
            | "publishing"
            | "py"
            | "python"
            | "reference"
            | "registry"
            | "rs"
            | "rust"
            | "setup"
            | "structure"
            | "structured"
            | "structuring"
            | "tool"
            | "tools"
            | "ts"
            | "typescript"
            | "using"
            | "view"
            | "views"
    )
}

fn is_generic_topical_token(token: &str) -> bool {
    matches!(
        token,
        "api"
            | "app"
            | "book"
            | "build"
            | "cli"
            | "code"
            | "command"
            | "commands"
            | "config"
            | "create"
            | "documentation"
            | "error"
            | "errors"
            | "find"
            | "docs"
            | "guide"
            | "guides"
            | "handling"
            | "install"
            | "dependency"
            | "dependencies"
            | "manage"
            | "management"
            | "marketplace"
            | "package"
            | "packages"
            | "plugin"
            | "plugins"
            | "publish"
            | "publishing"
            | "reference"
            | "registry"
            | "setup"
            | "structure"
            | "structured"
            | "structuring"
            | "tool"
            | "tools"
            | "using"
            | "view"
            | "views"
    )
}

fn candidate_topical_overlap_count(
    candidate: &ranking::AskCandidate,
    query_tokens: &[String],
) -> usize {
    query_tokens
        .iter()
        .filter(|token| {
            candidate.url_tokens.contains(token.as_str())
                || candidate.chunk_tokens.contains(token.as_str())
        })
        .count()
}

fn candidate_matches_any_token(candidate: &ranking::AskCandidate, tokens: &[&String]) -> bool {
    tokens.iter().any(|token| {
        candidate.url_tokens.contains(token.as_str())
            || candidate.chunk_tokens.contains(token.as_str())
    })
}

pub(crate) fn candidate_has_topical_overlap(
    candidate: &ranking::AskCandidate,
    query_tokens: &[String],
) -> bool {
    if query_tokens.is_empty() {
        return true;
    }
    let salient_tokens = query_tokens
        .iter()
        .filter(|token| !is_generic_topical_token(token.as_str()))
        .collect::<Vec<_>>();
    if !salient_tokens.is_empty() && !candidate_matches_any_token(candidate, &salient_tokens) {
        return false;
    }
    let overlap = candidate_topical_overlap_count(candidate, query_tokens);
    let coverage = overlap as f64 / query_tokens.len() as f64;
    match query_tokens.len() {
        0 => true,
        1 | 2 => overlap >= 1,
        3 | 4 => overlap >= 1 || coverage >= 0.5,
        _ => overlap >= 2 && coverage >= 0.34,
    }
}
