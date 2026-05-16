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
        verticals::github_release::INFO,
        verticals::reddit::INFO,
        verticals::pypi::INFO,
        verticals::npm::INFO,
        verticals::crates_io::INFO,
        verticals::docker_hub::INFO,
        verticals::huggingface_model::INFO,
        verticals::dev_to::INFO,
        verticals::shopify::INFO,
        verticals::youtube_video::INFO,
        verticals::amazon::INFO,
        verticals::ebay::INFO,
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
    // github_repo before github_release: repo URL is 2-segment; release is 3+
    if verticals::github_repo::INFO.auto_dispatch && verticals::github_repo::matches(url) {
        return Some(verticals::github_repo::extract(url, ctx).await);
    }
    if verticals::github_release::INFO.auto_dispatch && verticals::github_release::matches(url) {
        return Some(verticals::github_release::extract(url, ctx).await);
    }
    if verticals::reddit::INFO.auto_dispatch && verticals::reddit::matches(url) {
        return Some(verticals::reddit::extract(url, ctx).await);
    }
    if verticals::pypi::INFO.auto_dispatch && verticals::pypi::matches(url) {
        return Some(verticals::pypi::extract(url, ctx).await);
    }
    if verticals::npm::INFO.auto_dispatch && verticals::npm::matches(url) {
        return Some(verticals::npm::extract(url, ctx).await);
    }
    if verticals::crates_io::INFO.auto_dispatch && verticals::crates_io::matches(url) {
        return Some(verticals::crates_io::extract(url, ctx).await);
    }
    if verticals::docker_hub::INFO.auto_dispatch && verticals::docker_hub::matches(url) {
        return Some(verticals::docker_hub::extract(url, ctx).await);
    }
    if verticals::huggingface_model::INFO.auto_dispatch && verticals::huggingface_model::matches(url) {
        return Some(verticals::huggingface_model::extract(url, ctx).await);
    }
    if verticals::dev_to::INFO.auto_dispatch && verticals::dev_to::matches(url) {
        return Some(verticals::dev_to::extract(url, ctx).await);
    }
    if verticals::shopify::INFO.auto_dispatch && verticals::shopify::matches(url) {
        return Some(verticals::shopify::extract(url, ctx).await);
    }
    // youtube_video: auto_dispatch=false — not in auto path
    // amazon: auto_dispatch=false — not in auto path
    // ebay: auto_dispatch=false — not in auto path
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
                return Err(VerticalError::VerticalUnsupportedUrl { vertical: "github_repo", url: url.to_string() });
            }
            verticals::github_repo::extract(url, ctx).await
        }
        "github_release" => {
            if !verticals::github_release::matches(url) {
                return Err(VerticalError::VerticalUnsupportedUrl { vertical: "github_release", url: url.to_string() });
            }
            verticals::github_release::extract(url, ctx).await
        }
        "reddit" => {
            if !verticals::reddit::matches(url) {
                return Err(VerticalError::VerticalUnsupportedUrl { vertical: "reddit", url: url.to_string() });
            }
            verticals::reddit::extract(url, ctx).await
        }
        "pypi" => {
            if !verticals::pypi::matches(url) {
                return Err(VerticalError::VerticalUnsupportedUrl { vertical: "pypi", url: url.to_string() });
            }
            verticals::pypi::extract(url, ctx).await
        }
        "npm" => {
            if !verticals::npm::matches(url) {
                return Err(VerticalError::VerticalUnsupportedUrl { vertical: "npm", url: url.to_string() });
            }
            verticals::npm::extract(url, ctx).await
        }
        "crates_io" => {
            if !verticals::crates_io::matches(url) {
                return Err(VerticalError::VerticalUnsupportedUrl { vertical: "crates_io", url: url.to_string() });
            }
            verticals::crates_io::extract(url, ctx).await
        }
        "docker_hub" => {
            if !verticals::docker_hub::matches(url) {
                return Err(VerticalError::VerticalUnsupportedUrl { vertical: "docker_hub", url: url.to_string() });
            }
            verticals::docker_hub::extract(url, ctx).await
        }
        "huggingface_model" => {
            if !verticals::huggingface_model::matches(url) {
                return Err(VerticalError::VerticalUnsupportedUrl { vertical: "huggingface_model", url: url.to_string() });
            }
            verticals::huggingface_model::extract(url, ctx).await
        }
        "dev_to" => {
            if !verticals::dev_to::matches(url) {
                return Err(VerticalError::VerticalUnsupportedUrl { vertical: "dev_to", url: url.to_string() });
            }
            verticals::dev_to::extract(url, ctx).await
        }
        "shopify" => {
            if !verticals::shopify::matches(url) {
                return Err(VerticalError::VerticalUnsupportedUrl { vertical: "shopify", url: url.to_string() });
            }
            verticals::shopify::extract(url, ctx).await
        }
        "youtube_video" => {
            if !verticals::youtube_video::matches(url) {
                return Err(VerticalError::VerticalUnsupportedUrl { vertical: "youtube_video", url: url.to_string() });
            }
            verticals::youtube_video::extract(url, ctx).await
        }
        "amazon" => {
            if !verticals::amazon::matches(url) {
                return Err(VerticalError::VerticalUnsupportedUrl { vertical: "amazon", url: url.to_string() });
            }
            verticals::amazon::extract(url, ctx).await
        }
        "ebay" => {
            if !verticals::ebay::matches(url) {
                return Err(VerticalError::VerticalUnsupportedUrl { vertical: "ebay", url: url.to_string() });
            }
            verticals::ebay::extract(url, ctx).await
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
    #[tokio::test]
    async fn catalog_exhaustiveness() {
        let ctx = VerticalContext::new(std::sync::Arc::new(
            crate::core::config::Config::default_lite(),
        ));
        for info in list() {
            let result = dispatch_by_name(info.name, "https://example.com", &ctx).await;
            match result {
                Err(VerticalError::VerticalUnsupportedUrl { vertical, .. }) => {
                    assert_ne!(
                        vertical, "unknown",
                        "Extractor '{}' is in catalog but missing from dispatch_by_name match arm",
                        info.name
                    );
                }
                _ => {}
            }
        }
    }

    #[test]
    fn dispatch_by_url_matches_github_repo() {
        assert!(verticals::github_repo::matches("https://github.com/rust-lang/rust"));
    }

    #[test]
    fn dispatch_by_url_falls_through_for_unknown_url() {
        assert!(!verticals::github_repo::matches("https://example.com/some/page"));
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

    #[test]
    fn github_release_matches_releases_path() {
        assert!(verticals::github_release::matches("https://github.com/rust-lang/rust/releases"));
        assert!(verticals::github_release::matches("https://github.com/rust-lang/rust/releases/tag/1.0.0"));
        assert!(!verticals::github_release::matches("https://github.com/rust-lang/rust"));
    }

    #[test]
    fn reddit_matches_reddit_hosts() {
        assert!(verticals::reddit::matches("https://reddit.com/r/rust"));
        assert!(verticals::reddit::matches("https://old.reddit.com/r/programming"));
        assert!(!verticals::reddit::matches("https://example.com/r/test"));
    }

    #[test]
    fn pypi_matches_project_paths() {
        assert!(verticals::pypi::matches("https://pypi.org/project/requests"));
        assert!(verticals::pypi::matches("https://pypi.org/project/requests/2.28.0"));
        assert!(!verticals::pypi::matches("https://pypi.org/simple/requests"));
    }

    #[test]
    fn npm_matches_package_paths() {
        assert!(verticals::npm::matches("https://npmjs.com/package/react"));
        assert!(verticals::npm::matches("https://www.npmjs.com/package/@types/node"));
        assert!(!verticals::npm::matches("https://npmjs.com/search?q=react"));
    }

    #[test]
    fn crates_io_matches_crates_paths() {
        assert!(verticals::crates_io::matches("https://crates.io/crates/serde"));
        assert!(verticals::crates_io::matches("https://crates.io/crates/serde/1.0.0"));
        assert!(!verticals::crates_io::matches("https://crates.io/search?q=serde"));
    }

    #[test]
    fn docker_hub_matches_repository_paths() {
        assert!(verticals::docker_hub::matches("https://hub.docker.com/r/library/nginx"));
        assert!(verticals::docker_hub::matches("https://hub.docker.com/_/nginx"));
        assert!(!verticals::docker_hub::matches("https://hub.docker.com/search?q=nginx"));
    }

    #[test]
    fn youtube_video_matches_watch_urls() {
        assert!(verticals::youtube_video::matches("https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
        assert!(verticals::youtube_video::matches("https://youtu.be/dQw4w9WgXcQ"));
        assert!(!verticals::youtube_video::matches("https://youtube.com/channel/UCtest"));
    }

    #[test]
    fn dev_to_matches_article_paths() {
        assert!(verticals::dev_to::matches("https://dev.to/username/my-cool-article"));
        assert!(!verticals::dev_to::matches("https://dev.to/t/rust"));
        assert!(!verticals::dev_to::matches("https://dev.to/username"));
    }

    #[test]
    fn shopify_matches_product_paths() {
        assert!(verticals::shopify::matches("https://mystore.myshopify.com/products/cool-shirt"));
        assert!(!verticals::shopify::matches("https://github.com/products/cool-shirt"));
        assert!(!verticals::shopify::matches("https://amazon.com/products/thing"));
    }

    #[test]
    fn huggingface_model_matches_model_paths() {
        assert!(verticals::huggingface_model::matches("https://huggingface.co/openai/whisper-large"));
        assert!(!verticals::huggingface_model::matches("https://huggingface.co/datasets/squad"));
        assert!(!verticals::huggingface_model::matches("https://huggingface.co/spaces/gradio/hello"));
    }

    #[test]
    fn amazon_matches_product_urls() {
        assert!(verticals::amazon::matches("https://amazon.com/dp/B08N5WRWNW"));
        assert!(verticals::amazon::matches("https://www.amazon.com/gp/product/B08N5WRWNW"));
        assert!(verticals::amazon::matches("https://amazon.co.uk/dp/B08N5WRWNW"));
        assert!(!verticals::amazon::matches("https://amazon.com/s?k=shirts"));
    }

    #[test]
    fn ebay_matches_listing_urls() {
        assert!(verticals::ebay::matches("https://ebay.com/itm/123456789"));
        assert!(verticals::ebay::matches("https://www.ebay.co.uk/itm/987654321"));
        assert!(!verticals::ebay::matches("https://ebay.com/motors"));
    }
}
