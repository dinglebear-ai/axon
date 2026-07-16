//! Adapter-owned routing registry for vertical extractor implementations.
//!
//! `axon-extract` owns implementation functions and descriptors. This module
//! owns canonical ordering, automatic-dispatch policy, named lookup, and the
//! fallback contract used by source acquisition.

use axon_extract::{ExtractorInfo, ScrapedDoc, VerticalContext, VerticalError, verticals};

/// Full implementation catalog in canonical automatic-dispatch priority order.
pub fn list() -> Vec<ExtractorInfo> {
    vec![
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
        verticals::amazon::INFO,
        verticals::ebay::INFO,
    ]
}

/// Dispatch the first automatic implementation that claims `url`.
pub async fn dispatch_by_url(
    url: &str,
    ctx: &VerticalContext,
) -> Option<Result<ScrapedDoc, VerticalError>> {
    macro_rules! dispatch_if_matched {
        ($implementation:ident) => {
            if auto_enabled(verticals::$implementation::INFO, ctx)
                && verticals::$implementation::matches(url)
            {
                return Some(verticals::$implementation::extract(url, ctx).await);
            }
        };
    }

    dispatch_if_matched!(github_pr);
    dispatch_if_matched!(github_issue);
    dispatch_if_matched!(github_repo);
    dispatch_if_matched!(github_release);
    dispatch_if_matched!(reddit);
    dispatch_if_matched!(pypi);
    dispatch_if_matched!(npm);
    dispatch_if_matched!(crates_io);
    dispatch_if_matched!(docs_rs);
    dispatch_if_matched!(docker_hub);
    dispatch_if_matched!(huggingface_model);
    dispatch_if_matched!(dev_to);
    dispatch_if_matched!(shopify);
    dispatch_if_matched!(hackernews);
    dispatch_if_matched!(stackoverflow);
    dispatch_if_matched!(arxiv);
    None
}

fn auto_enabled(info: ExtractorInfo, ctx: &VerticalContext) -> bool {
    info.auto_dispatch && !ctx.auto_dispatch_skipped(info.name)
}

async fn guard_and_dispatch<F, Fut>(
    implementation: &'static str,
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
            vertical: implementation,
            url: url.to_string(),
        });
    }
    extract().await
}

/// Dispatch one explicitly named implementation, including opt-in-only ones.
pub async fn dispatch_by_name(
    name: &str,
    url: &str,
    ctx: &VerticalContext,
) -> Result<ScrapedDoc, VerticalError> {
    macro_rules! dispatch {
        ($implementation:ident) => {{
            guard_and_dispatch(
                stringify!($implementation),
                verticals::$implementation::matches(url),
                url,
                || verticals::$implementation::extract(url, ctx),
            )
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
#[path = "vertical_registry_tests.rs"]
mod tests;
