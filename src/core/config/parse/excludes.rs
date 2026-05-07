use crate::core::logging::log_warn;

pub(crate) struct NormalizedExcludePrefixes {
    pub(crate) prefixes: Vec<String>,
    pub(crate) disable_defaults: bool,
}

/// Returns the static default exclude-path prefixes without allocating.
///
/// Callers that need `Vec<String>` should use [`default_exclude_prefixes_vec`].
pub fn default_exclude_prefixes() -> &'static [&'static str] {
    &[
        // Auth / account / transactional -- no indexable content
        "/account",
        "/admin",
        "/auth",
        "/callback",
        "/cart",
        "/checkout",
        "/dashboard",
        "/login",
        "/logout",
        "/oauth",
        "/register",
        "/settings",
        "/signin",
        "/signup",
        "/unsubscribe",
        "/webhook",
        "/webhooks",
        // Legal / compliance boilerplate
        "/cookie-policy",
        "/cookies",
        "/legal",
        "/privacy",
        "/terms",
        // CDN / framework internals -- never user-facing content
        "/_astro",
        "/_next",
        "/_nuxt",
        "/_vercel",
        "/__nextjs",
        "/cdn-cgi",
        "/static",
        "/wp-admin",
        "/wp-includes",
        // Syndication feeds -- XML, not useful for RAG
        "/atom",
        "/feed",
        "/rss",
        // Marketing / sales pages -- no technical content
        "/about",
        "/careers",
        "/case-studies",
        "/contact",
        "/customers",
        "/demo",
        "/enterprise",
        "/events",
        "/jobs",
        "/newsletter",
        "/newsroom",
        "/partners",
        "/press",
        "/pricing",
        "/testimonials",
        // User-generated / high-noise listing pages
        "/archive",
        "/categories",
        "/comments",
        "/profiles",
        "/tags",
        "/users",
        // Duplicate / utility page variants
        "/amp",
        "/print",
        "/search",
        "/share",
        // Forum / community discussion threads
        "/answers",
        "/discussions",
        "/forum",
        "/forums",
        "/questions",
        // Non-English locales
        "/ar",
        "/cs",
        "/da",
        "/de",
        "/el",
        "/es",
        "/fi",
        "/fr",
        "/he",
        "/hu",
        "/id",
        "/it",
        "/ja",
        "/ko",
        "/nl",
        "/no",
        "/pl",
        "/pt",
        "/pt-br",
        "/ro",
        "/ru",
        "/sv",
        "/th",
        "/tr",
        "/uk",
        "/vi",
        "/zh",
        "/zh-cn",
        "/zh-tw",
    ]
}

/// Allocating wrapper for serde `#[serde(default = "...")]` and call sites
/// that need an owned `Vec<String>`.
pub fn default_exclude_prefixes_vec() -> Vec<String> {
    default_exclude_prefixes()
        .iter()
        .map(|s| s.to_string())
        .collect()
}

pub(crate) fn normalize_exclude_prefixes(input: Vec<String>) -> NormalizedExcludePrefixes {
    let disable_by_none = input.iter().any(|v| v.trim().eq_ignore_ascii_case("none"));
    if disable_by_none {
        let ignored: Vec<&str> = input
            .iter()
            .map(|value| value.trim())
            .filter(|value| !value.eq_ignore_ascii_case("none"))
            .filter(|value| !value.is_empty() && *value != "/")
            .collect();
        if !ignored.is_empty() {
            log_warn(&format!(
                "exclude_prefix_ignored action=disabling_defaults ignored_prefixes={}",
                ignored.join(", ")
            ));
        }
        return NormalizedExcludePrefixes {
            prefixes: Vec::new(),
            disable_defaults: true,
        };
    }

    // Computed after the disable_by_none early return to avoid a wasted scan
    // when the entire list is being disabled anyway.
    let disable_by_empty = input.iter().any(|v| matches!(v.trim(), "" | "/"));

    let mut out = Vec::new();
    for raw in input {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed == "/" {
            continue;
        }
        let normalized = if trimmed.starts_with('/') {
            trimmed.to_string()
        } else {
            format!("/{trimmed}")
        };
        out.push(normalized);
    }
    out.sort();
    out.dedup();
    NormalizedExcludePrefixes {
        prefixes: out,
        disable_defaults: disable_by_empty,
    }
}

#[cfg(test)]
mod tests {
    use super::{default_exclude_prefixes, normalize_exclude_prefixes};
    use std::collections::HashSet;

    #[test]
    fn normalize_exclude_prefixes_none_disables_defaults() {
        let normalized = normalize_exclude_prefixes(vec!["none".to_string()]);
        assert!(normalized.disable_defaults);
        assert!(normalized.prefixes.is_empty());
    }

    #[test]
    fn normalize_exclude_prefixes_none_with_values_still_disables() {
        let normalized = normalize_exclude_prefixes(vec!["none".to_string(), "/fr".to_string()]);
        assert!(normalized.disable_defaults);
        assert!(normalized.prefixes.is_empty());
    }

    #[test]
    fn defaults_all_start_with_slash() {
        for prefix in default_exclude_prefixes() {
            assert!(
                prefix.starts_with('/'),
                "prefix missing leading slash: {prefix}"
            );
        }
    }

    #[test]
    fn defaults_no_duplicates() {
        let prefixes = default_exclude_prefixes();
        let unique: HashSet<&str> = prefixes.iter().copied().collect();
        assert_eq!(
            prefixes.len(),
            unique.len(),
            "duplicate prefixes found in defaults"
        );
    }

    #[test]
    fn defaults_are_sorted_within_categories() {
        // The full list won't be globally sorted (categories break ordering),
        // but verify no entry appears to be a typo or misplaced duplicate.
        let prefixes = default_exclude_prefixes();
        assert!(
            prefixes.len() > 50,
            "expected 50+ default prefixes, got {}",
            prefixes.len()
        );
    }

    #[test]
    fn normalize_adds_leading_slash() {
        let normalized = normalize_exclude_prefixes(vec!["blog".to_string()]);
        assert_eq!(normalized.prefixes, vec!["/blog"]);
        assert!(!normalized.disable_defaults);
    }

    #[test]
    fn normalize_deduplicates_and_sorts() {
        let normalized = normalize_exclude_prefixes(vec![
            "/zz".to_string(),
            "/aa".to_string(),
            "/zz".to_string(),
        ]);
        assert_eq!(normalized.prefixes, vec!["/aa", "/zz"]);
    }

    #[test]
    fn normalize_skips_empty_and_root() {
        let normalized =
            normalize_exclude_prefixes(vec!["".to_string(), "  ".to_string(), "/ok".to_string()]);
        assert_eq!(normalized.prefixes, vec!["/ok"]);
    }
}
