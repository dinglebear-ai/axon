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
    if verticals::huggingface_model::INFO.auto_dispatch
        && verticals::huggingface_model::matches(url)
    {
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
/// Guard: reject the URL if it does not match the named extractor, then call extract.
async fn guard_and_dispatch<F, Fut>(
    vertical: &'static str,
    matches: bool,
    url: &str,
    extract: F,
) -> Result<ScrapedDoc, VerticalError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<ScrapedDoc, VerticalError>>,
{
    if !matches {
        return Err(VerticalError::VerticalUnsupportedUrl {
            vertical,
            url: url.to_string(),
        });
    }
    extract().await
}

pub async fn dispatch_by_name(
    name: &str,
    url: &str,
    ctx: &VerticalContext,
) -> Result<ScrapedDoc, VerticalError> {
    macro_rules! dispatch {
        ($mod:ident) => {{
            guard_and_dispatch(stringify!($mod), verticals::$mod::matches(url), url, || {
                verticals::$mod::extract(url, ctx)
            })
            .await
        }};
    }
    match name {
        "github_repo" => dispatch!(github_repo),
        "github_release" => dispatch!(github_release),
        "reddit" => dispatch!(reddit),
        "pypi" => dispatch!(pypi),
        "npm" => dispatch!(npm),
        "crates_io" => dispatch!(crates_io),
        "docker_hub" => dispatch!(docker_hub),
        "huggingface_model" => dispatch!(huggingface_model),
        "dev_to" => dispatch!(dev_to),
        "shopify" => dispatch!(shopify),
        "youtube_video" => dispatch!(youtube_video),
        "amazon" => dispatch!(amazon),
        "ebay" => dispatch!(ebay),
        other => Err(VerticalError::VerticalUnsupportedUrl {
            vertical: "unknown",
            url: format!("no extractor named '{other}'"),
        }),
    }
}

#[cfg(test)]
#[path = "registry_tests.rs"]
mod tests;
