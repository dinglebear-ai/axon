use super::*;

/// Every catalog entry must have a dispatch arm in `dispatch_by_name`.
#[tokio::test]
async fn catalog_exhaustiveness() {
    let ctx = VerticalContext::new(std::sync::Arc::new(
        axon_core::config::Config::default_minimal(),
    ));
    for info in list() {
        let result = dispatch_by_name(info.name, "https://example.com", &ctx).await;
        if let Err(VerticalError::VerticalUnsupportedUrl { vertical, .. }) = result {
            assert_ne!(
                vertical, "unknown",
                "Extractor '{}' is in catalog but missing from dispatch_by_name match arm",
                info.name
            );
        }
    }
}

#[test]
fn dispatch_by_url_matches_github_repo() {
    assert!(verticals::github_repo::matches(
        "https://github.com/rust-lang/rust"
    ));
}

#[test]
fn dispatch_by_url_falls_through_for_unknown_url() {
    assert!(!verticals::github_repo::matches(
        "https://example.com/some/page"
    ));
}

#[test]
fn github_repo_matches_rejects_reserved_owner() {
    assert!(!verticals::github_repo::matches(
        "https://github.com/settings/profile"
    ));
}

#[test]
fn github_repo_matches_rejects_sub_paths() {
    assert!(!verticals::github_repo::matches(
        "https://github.com/rust-lang/rust/pulls"
    ));
}

#[test]
fn github_repo_matches_rejects_non_github() {
    assert!(!verticals::github_repo::matches(
        "https://gitlab.com/user/repo"
    ));
}

#[test]
fn github_release_matches_releases_path() {
    assert!(verticals::github_release::matches(
        "https://github.com/rust-lang/rust/releases"
    ));
    assert!(verticals::github_release::matches(
        "https://github.com/rust-lang/rust/releases/tag/1.0.0"
    ));
    assert!(!verticals::github_release::matches(
        "https://github.com/rust-lang/rust"
    ));
}

#[test]
fn reddit_matches_reddit_hosts() {
    assert!(verticals::reddit::matches("https://reddit.com/r/rust"));
    assert!(verticals::reddit::matches(
        "https://old.reddit.com/r/programming"
    ));
    assert!(!verticals::reddit::matches("https://example.com/r/test"));
}

#[test]
fn pypi_matches_project_paths() {
    assert!(verticals::pypi::matches(
        "https://pypi.org/project/requests"
    ));
    assert!(verticals::pypi::matches(
        "https://pypi.org/project/requests/2.28.0"
    ));
    assert!(!verticals::pypi::matches(
        "https://pypi.org/simple/requests"
    ));
}

#[test]
fn npm_matches_package_paths() {
    assert!(verticals::npm::matches("https://npmjs.com/package/react"));
    assert!(verticals::npm::matches(
        "https://www.npmjs.com/package/@types/node"
    ));
    assert!(!verticals::npm::matches("https://npmjs.com/search?q=react"));
}

#[test]
fn crates_io_matches_crates_paths() {
    assert!(verticals::crates_io::matches(
        "https://crates.io/crates/serde"
    ));
    assert!(verticals::crates_io::matches(
        "https://crates.io/crates/serde/1.0.0"
    ));
    assert!(!verticals::crates_io::matches(
        "https://crates.io/search?q=serde"
    ));
}

#[test]
fn docker_hub_matches_repository_paths() {
    assert!(verticals::docker_hub::matches(
        "https://hub.docker.com/r/library/nginx"
    ));
    assert!(verticals::docker_hub::matches(
        "https://hub.docker.com/_/nginx"
    ));
    assert!(!verticals::docker_hub::matches(
        "https://hub.docker.com/search?q=nginx"
    ));
}

#[test]
fn dev_to_matches_article_paths() {
    assert!(verticals::dev_to::matches(
        "https://dev.to/username/my-cool-article"
    ));
    assert!(!verticals::dev_to::matches("https://dev.to/t/rust"));
    assert!(!verticals::dev_to::matches("https://dev.to/username"));
}

#[test]
fn shopify_matches_product_paths() {
    assert!(verticals::shopify::matches(
        "https://mystore.myshopify.com/products/cool-shirt"
    ));
    assert!(!verticals::shopify::matches(
        "https://github.com/products/cool-shirt"
    ));
    assert!(!verticals::shopify::matches(
        "https://amazon.com/products/thing"
    ));
}

#[test]
fn huggingface_model_matches_model_paths() {
    assert!(verticals::huggingface_model::matches(
        "https://huggingface.co/openai/whisper-large"
    ));
    assert!(!verticals::huggingface_model::matches(
        "https://huggingface.co/datasets/squad"
    ));
    assert!(!verticals::huggingface_model::matches(
        "https://huggingface.co/spaces/gradio/hello"
    ));
}

#[test]
fn amazon_matches_product_urls() {
    assert!(verticals::amazon::matches(
        "https://amazon.com/dp/B08N5WRWNW"
    ));
    assert!(verticals::amazon::matches(
        "https://www.amazon.com/gp/product/B08N5WRWNW"
    ));
    assert!(verticals::amazon::matches(
        "https://amazon.co.uk/dp/B08N5WRWNW"
    ));
    assert!(!verticals::amazon::matches("https://amazon.com/s?k=shirts"));
}

#[test]
fn ebay_matches_listing_urls() {
    assert!(verticals::ebay::matches("https://ebay.com/itm/123456789"));
    assert!(verticals::ebay::matches(
        "https://www.ebay.co.uk/itm/987654321"
    ));
    assert!(!verticals::ebay::matches("https://ebay.com/motors"));
}
