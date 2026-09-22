//! Per-candidate relevance, trust, and source-class signals for ask ranking.

use super::AskComplexity;
use axon_retrieval::QueryServiceHit;
use reqwest::Url;
use std::collections::HashSet;

const DOCS_PATH_BOOST: f64 = 0.04;

pub(super) fn classify_complexity(question: &str, tokens: &[String]) -> AskComplexity {
    let lower = question.to_ascii_lowercase();
    if [
        "list all",
        "show all",
        "every ",
        "everything",
        "enumerate",
        "comprehensive",
        "thorough",
        "in detail",
        "deep dive",
    ]
    .iter()
    .any(|n| lower.contains(n))
    {
        AskComplexity::Exhaustive
    } else if tokens.len() >= 5
        || lower.contains("how do i")
        || lower.contains("how should")
        || lower.contains("step by step")
        || lower.matches('?').count() > 1
    {
        AskComplexity::Complex
    } else {
        AskComplexity::Simple
    }
}

pub(super) fn is_low_signal_source(hit: &QueryServiceHit) -> bool {
    let lower = hit.canonical_uri.to_ascii_lowercase();
    let source_key = hit.citation.source_item_key.0.to_ascii_lowercase();
    let web = lower.starts_with("http://") || lower.starts_with("https://");
    lower.starts_with("session://")
        || source_key.ends_with(".jsonl")
        || lower.contains("/docs/sessions/")
        || lower.contains("docs/sessions/")
        || lower.contains("/.cache/")
        || lower.contains(".cache/")
        || (!web && lower.contains("/logs/"))
        || (!web && lower.ends_with(".log"))
}

pub(super) fn matches_named_product_identity(
    url: &HashSet<String>,
    text: &HashSet<String>,
    tokens: &[String],
) -> bool {
    let query_tokens = tokens.iter().map(String::as_str).collect::<HashSet<_>>();
    let matching_rules = PRODUCT_OFFICIAL_DOMAINS
        .iter()
        .filter(|rule| rule.tokens.iter().any(|token| query_tokens.contains(token)))
        .collect::<Vec<_>>();
    if matching_rules.is_empty() {
        return true;
    }

    matching_rules.into_iter().any(|rule| {
        rule.tokens
            .iter()
            .any(|token| url.contains(*token) || text.contains(*token))
    })
}

pub(super) fn authority_deltas(
    retrieval_score: f64,
    configured: f64,
    product: f64,
    hybrid: bool,
) -> (f64, f64) {
    if !hybrid {
        return (configured, product);
    }
    let requested = configured + product;
    if requested <= 0.0 {
        return (0.0, 0.0);
    }
    let total = requested.min(retrieval_score.max(0.0));
    (total * configured / requested, total * product / requested)
}

pub(super) fn docs_path_boost(url: &str) -> f64 {
    let path = Url::parse(url)
        .ok()
        .map(|u| u.path().to_ascii_lowercase())
        .unwrap_or_else(|| url.to_ascii_lowercase());
    if path.contains("/docs/")
        || path.contains("/guides/")
        || path.contains("/api/")
        || path.contains("/reference/")
    {
        DOCS_PATH_BOOST
    } else {
        0.0
    }
}

pub(super) fn normalized_domains(domains: &[String]) -> Vec<String> {
    domains
        .iter()
        .map(|d| d.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|d| !d.is_empty())
        .collect()
}

pub(super) fn host_matches_domains(url: &str, domains: &[String]) -> bool {
    let Some(host) = Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
    else {
        return false;
    };
    domains.iter().any(|d| {
        host == *d
            || (host.len() > d.len()
                && host.ends_with(d)
                && host.as_bytes()[host.len() - d.len() - 1] == b'.')
    })
}

struct ProductOfficialDomain {
    tokens: &'static [&'static str],
    domain: &'static str,
}

// Product authority is a trust decision, not a relevance heuristic. Keep this
// registry deliberately small and fail closed. Operators can extend trusted
// sources with ask.authoritative-domains without changing this built-in list.
const PRODUCT_OFFICIAL_DOMAINS: &[ProductOfficialDomain] = &[
    ProductOfficialDomain {
        tokens: &["claude", "anthropic"],
        domain: "code.claude.com",
    },
    ProductOfficialDomain {
        tokens: &["openclaw"],
        domain: "docs.openclaw.ai",
    },
    ProductOfficialDomain {
        tokens: &["codex", "openai"],
        domain: "developers.openai.com",
    },
];

pub(super) fn product_authority_match(url: &str, tokens: &[String]) -> bool {
    if tokens.is_empty() {
        return false;
    }
    let Some(host) = Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_ascii_lowercase))
    else {
        return false;
    };
    let token_set = tokens.iter().map(String::as_str).collect::<HashSet<_>>();
    PRODUCT_OFFICIAL_DOMAINS.iter().any(|rule| {
        rule.tokens.iter().any(|token| token_set.contains(token))
            && host_matches_domain(&host, rule.domain)
    })
}

fn host_matches_domain(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

pub(super) fn ascii_lowercase_contains(haystack: &str, needle: &str) -> bool {
    if needle.len() > haystack.len() {
        return false;
    }
    haystack.as_bytes().windows(needle.len()).any(|w| {
        w.iter()
            .zip(needle.as_bytes())
            .all(|(a, b)| a.to_ascii_lowercase() == *b)
    })
}
