use super::*;

#[test]
fn feed_extension_urls_are_feeds() {
    assert!(is_feed_target("https://example.com/blog/feed.rss"));
    assert!(is_feed_target("https://example.com/releases.atom"));
    assert!(is_feed_target("https://example.com/index.rdf"));
}

#[test]
fn feed_prefix_forces_feed_classification() {
    // A bare web-shaped URL with an explicit prefix is a feed.
    assert!(is_feed_target("rss:https://example.com/blog"));
    assert!(is_feed_target("feed:example.com/updates"));
    assert!(is_feed_target("atom:https://news.example.com/"));
}

#[test]
fn empty_prefix_remainder_is_not_a_feed() {
    assert!(!is_feed_target("rss:"));
    assert!(!is_feed_target("feed:   "));
}

#[test]
fn feed_path_segment_and_query_are_feeds() {
    assert!(is_feed_target("https://example.com/feed"));
    assert!(is_feed_target("https://example.com/blog/rss.xml"));
    assert!(is_feed_target("https://example.com/?feed=rss2"));
    assert!(is_feed_target("https://example.com/?format=atom"));
}

#[test]
fn plain_web_urls_are_not_feeds() {
    // A plain docs URL must NOT be mis-detected as a feed — this is the seam
    // where feed classification must not swallow ordinary web pages.
    assert!(!is_feed_target("https://docs.example.com/guide"));
    assert!(!is_feed_target("http://example.com"));
    assert!(!is_feed_target("https://example.com/team/guide"));
    // Feed-word-in-query traps that must NOT count.
    assert!(!is_feed_target("https://example.com/?feedback=1"));
    assert!(!is_feed_target("https://example.com/?category=atom"));
}

#[test]
fn non_urls_are_not_feeds() {
    assert!(!is_feed_target("just-a-word"));
    assert!(!is_feed_target("ftp://example.com/feed.rss"));
    assert!(!is_feed_target(""));
}

#[test]
fn normalize_strips_prefix_and_upgrades_scheme() {
    assert_eq!(
        normalize_feed_target("rss:https://example.com/blog"),
        "https://example.com/blog"
    );
    assert_eq!(
        normalize_feed_target("feed:example.com/updates"),
        "https://example.com/updates"
    );
    assert_eq!(
        normalize_feed_target("atom:news.example.com"),
        "https://news.example.com"
    );
}

#[test]
fn normalize_leaves_unprefixed_urls_unchanged() {
    assert_eq!(
        normalize_feed_target("https://example.com/feed.rss"),
        "https://example.com/feed.rss"
    );
    assert_eq!(
        normalize_feed_target("  https://example.com/feed  "),
        "https://example.com/feed"
    );
}
