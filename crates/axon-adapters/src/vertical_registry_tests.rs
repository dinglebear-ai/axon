use std::collections::BTreeSet;
use std::sync::Arc;

use axon_core::config::Config;
use axon_extract::{VerticalContext, VerticalError};

use super::*;

fn context() -> VerticalContext {
    VerticalContext::new(Arc::new(Config::default_minimal()))
}

#[test]
fn catalog_is_complete_ordered_and_unique() {
    let names = list().into_iter().map(|info| info.name).collect::<Vec<_>>();
    let unique = names.iter().copied().collect::<BTreeSet<_>>();

    assert_eq!(names.len(), 18);
    assert_eq!(unique.len(), names.len());
    assert_eq!(
        names,
        vec![
            "github_pr",
            "github_issue",
            "github_repo",
            "github_release",
            "reddit",
            "pypi",
            "npm",
            "crates_io",
            "docs_rs",
            "docker_hub",
            "huggingface_model",
            "dev_to",
            "shopify",
            "hackernews",
            "stackoverflow",
            "arxiv",
            "amazon",
            "ebay",
        ]
    );
}

#[tokio::test]
async fn every_catalog_entry_has_named_dispatch() {
    let ctx = context();
    for info in list() {
        let result = dispatch_by_name(info.name, "https://example.com", &ctx).await;
        if let Err(VerticalError::VerticalUnsupportedUrl { vertical, .. }) = result {
            assert_ne!(
                vertical, "unknown",
                "implementation '{}' is listed but missing named dispatch",
                info.name
            );
        }
    }
}

#[tokio::test]
async fn automatic_dispatch_honors_adapter_skip_policy() {
    let mut cfg = Config::default_minimal();
    cfg.auto_dispatch_skip = vec!["github_repo".to_string()];
    let ctx = VerticalContext::new(Arc::new(cfg));

    let result = dispatch_by_url("https://github.com/rust-lang/rust", &ctx).await;

    assert!(result.is_none());
}

#[tokio::test]
async fn opt_in_implementations_are_excluded_from_automatic_dispatch() {
    let result = dispatch_by_url("https://amazon.com/dp/B08N5WRWNW", &context()).await;

    assert!(result.is_none());
}

#[tokio::test]
async fn unknown_named_implementation_is_rejected_by_registry() {
    let result = dispatch_by_name("missing", "https://example.com", &context()).await;

    assert!(matches!(
        result,
        Err(VerticalError::VerticalUnsupportedUrl {
            vertical: "unknown",
            ..
        })
    ));
}
