use super::*;

#[tokio::test]
async fn fetch_feed_rejects_ssrf_target() {
    // A loopback/private target is rejected before any request is sent.
    let err = fetch_feed_to_file("https://127.0.0.1/feed.rss")
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("refusing to fetch"),
        "expected SSRF rejection, got: {err}"
    );
}

#[tokio::test]
async fn fetch_feed_rejects_ssrf_target_behind_prefix() {
    // The `rss:` prefix is normalized to the real URL, which is then
    // SSRF-validated — a private target is still rejected before any fetch.
    let err = fetch_feed_to_file("rss:https://127.0.0.1/blog")
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("refusing to fetch"),
        "expected SSRF rejection through prefix, got: {err}"
    );
}

#[test]
fn fetch_target_resolves_from_prefix_without_network() {
    // The URL the fetch helper targets is derived purely from the input, so we
    // can assert it without executing a request.
    assert_eq!(
        normalize_feed_target("rss:example.com/feed"),
        "https://example.com/feed"
    );
    assert_eq!(
        normalize_feed_target("https://example.com/feed.atom"),
        "https://example.com/feed.atom"
    );
}

#[test]
fn feed_cache_path_is_stable_per_url_and_distinct_across_urls() {
    // Load-bearing: the feed bridge hashes the (canonicalized) feed_path to form
    // the source id, so the same feed URL MUST map to the same path across runs
    // for generation/manifest-diff refresh to work — a random temp name would
    // make every run a brand-new source.
    let a1 = feed_cache_path("https://example.com/feed.atom");
    let a2 = feed_cache_path("https://example.com/feed.atom");
    let b = feed_cache_path("https://other.example.com/feed.rss");
    assert_eq!(a1, a2, "same feed URL must map to the same cache path");
    assert_ne!(
        a1, b,
        "different feed URLs must map to different cache paths"
    );
    assert!(
        a1.extension().is_some_and(|ext| ext == "xml"),
        "expected a .xml cache path, got {a1:?}"
    );
}
