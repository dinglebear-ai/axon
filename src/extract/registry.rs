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
        // GitHub: PR and issue must come before repo (longer path match)
        verticals::github_pr::INFO,
        verticals::github_issue::INFO,
        verticals::github_repo::INFO,
        verticals::github_release::INFO,
        verticals::reddit::INFO,
        verticals::pypi::INFO,
        verticals::npm::INFO,
        verticals::crates_io::INFO,
        verticals::docs_rs::INFO,
        verticals::docker_hub::INFO,
        verticals::huggingface_model::INFO,
        verticals::dev_to::INFO,
        verticals::shopify::INFO,
        verticals::hackernews::INFO,
        verticals::stackoverflow::INFO,
        verticals::arxiv::INFO,
        // auto_dispatch: false — explicit opt-in only
        verticals::amazon::INFO,
        verticals::ebay::INFO,
        // youtube_video removed: HTML scraping of ytInitialPlayerResponse is fragile
        // and produces no transcript. Use `axon ingest <youtube-url>` (yt-dlp path) instead.
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
    // github_pr and github_issue before github_repo: they have longer path matches
    if verticals::github_pr::INFO.auto_dispatch && verticals::github_pr::matches(url) {
        return Some(verticals::github_pr::extract(url, ctx).await);
    }
    if verticals::github_issue::INFO.auto_dispatch && verticals::github_issue::matches(url) {
        return Some(verticals::github_issue::extract(url, ctx).await);
    }
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
    if verticals::docs_rs::INFO.auto_dispatch && verticals::docs_rs::matches(url) {
        return Some(verticals::docs_rs::extract(url, ctx).await);
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
    if verticals::hackernews::INFO.auto_dispatch && verticals::hackernews::matches(url) {
        return Some(verticals::hackernews::extract(url, ctx).await);
    }
    if verticals::stackoverflow::INFO.auto_dispatch && verticals::stackoverflow::matches(url) {
        return Some(verticals::stackoverflow::extract(url, ctx).await);
    }
    if verticals::arxiv::INFO.auto_dispatch && verticals::arxiv::matches(url) {
        return Some(verticals::arxiv::extract(url, ctx).await);
    }
    // amazon:  auto_dispatch=false — not in auto path
    // ebay:    auto_dispatch=false — not in auto path
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
        "github_pr" => dispatch!(github_pr),
        "github_issue" => dispatch!(github_issue),
        "github_repo" => dispatch!(github_repo),
        "github_release" => dispatch!(github_release),
        "reddit" => dispatch!(reddit),
        "pypi" => dispatch!(pypi),
        "npm" => dispatch!(npm),
        "crates_io" => dispatch!(crates_io),
        "docs_rs" => dispatch!(docs_rs),
        "docker_hub" => dispatch!(docker_hub),
        "huggingface_model" => dispatch!(huggingface_model),
        "dev_to" => dispatch!(dev_to),
        "shopify" => dispatch!(shopify),
        "hackernews" => dispatch!(hackernews),
        "stackoverflow" => dispatch!(stackoverflow),
        "arxiv" => dispatch!(arxiv),
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
