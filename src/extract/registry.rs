//! Dispatch registry — matches a URL or name to a vertical extractor.
//!
//! ## Design
//! Plain match-chain dispatch, no trait objects. webclaw mod.rs:9-11
//! explicitly rejected a trait registry at 28 extractors. At the current
//! extractor count, named-function dispatch is cleaner and faster.
//!
//! ## Exhaustiveness guarantee
//! `list()` returns the catalog. A unit test asserts every catalog entry
//! has a corresponding arm in `dispatch_by_name()`, preventing the common
//! "added extractor to catalog but forgot the dispatch arm" bug.

use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};
use crate::extract::verticals;

/// The full extractor catalog in auto-dispatch priority order.
///
/// `auto_dispatch: true` extractors fire on `dispatch_by_url()`.
/// `auto_dispatch: false` extractors only fire on `dispatch_by_name()`.
pub fn list() -> Vec<ExtractorInfo> {
    vec![
        verticals::github_repo::INFO,
    ]
}

/// Try each registered extractor whose `matches(url) == true` and
/// `auto_dispatch == true`. Returns the first success, or `None` if
/// no extractor claims the URL.
///
/// Callers should fall through to the generic HTTP scrape path on `None`.
pub async fn dispatch_by_url(
    url: &str,
    ctx: &VerticalContext,
) -> Option<Result<ScrapedDoc, VerticalError>> {
    if verticals::github_repo::INFO.auto_dispatch
        && verticals::github_repo::matches(url)
    {
        return Some(verticals::github_repo::extract(url, ctx).await);
    }
    None
}

/// Invoke a named extractor explicitly. Does NOT check `auto_dispatch` —
/// callers opt in deliberately via `--vertical <name>` or MCP action.
///
/// Returns `Err(VerticalUnsupportedUrl)` if the URL doesn't match the
/// named extractor's `matches()` predicate.
pub async fn dispatch_by_name(
    name: &str,
    url: &str,
    ctx: &VerticalContext,
) -> Result<ScrapedDoc, VerticalError> {
    match name {
        "github_repo" => {
            if !verticals::github_repo::matches(url) {
                return Err(VerticalError::VerticalUnsupportedUrl {
                    vertical: "github_repo",
                    url: url.to_string(),
                });
            }
            verticals::github_repo::extract(url, ctx).await
        }
        other => Err(VerticalError::VerticalUnsupportedUrl {
            vertical: "unknown",
            url: format!("no extractor named '{other}'"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every catalog entry must have a dispatch arm in `dispatch_by_name`.
    /// This is the exhaustiveness check that replaces compile-time trait
    /// enforcement. If you add an entry to `list()` but forget the match
    /// arm, this test fails.
    #[tokio::test]
    async fn catalog_exhaustiveness() {
        let ctx = VerticalContext::new(std::sync::Arc::new(
            crate::core::config::Config::default_lite(),
        ));
        for info in list() {
            // We can't call dispatch_by_name without a real URL, but we can
            // verify the name round-trips: dispatch returns VerticalUnsupportedUrl
            // (wrong URL) rather than the catch-all "no extractor named" variant.
            let result = dispatch_by_name(info.name, "https://example.com", &ctx).await;
            match result {
                Err(VerticalError::VerticalUnsupportedUrl { vertical, .. }) => {
                    assert_ne!(
                        vertical, "unknown",
                        "Extractor '{}' is in catalog but missing from dispatch_by_name match arm",
                        info.name
                    );
                }
                _ => {} // actual success or other error is fine
            }
        }
    }

    #[test]
    fn dispatch_by_url_matches_github_repo() {
        // Smoke test — dispatch_by_url finds github_repo for a repo URL.
        // (async resolution skipped in unit test; just verify matches())
        let url = "https://github.com/rust-lang/rust";
        assert!(verticals::github_repo::matches(url));
    }

    #[test]
    fn dispatch_by_url_falls_through_for_unknown_url() {
        let url = "https://example.com/some/page";
        assert!(!verticals::github_repo::matches(url));
    }

    #[test]
    fn github_repo_matches_rejects_reserved_owner() {
        assert!(!verticals::github_repo::matches("https://github.com/settings/profile"));
    }

    #[test]
    fn github_repo_matches_rejects_sub_paths() {
        assert!(!verticals::github_repo::matches("https://github.com/rust-lang/rust/pulls"));
    }

    #[test]
    fn github_repo_matches_rejects_non_github() {
        assert!(!verticals::github_repo::matches("https://gitlab.com/user/repo"));
    }
}
