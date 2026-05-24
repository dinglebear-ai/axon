use super::*;

#[test]
fn test_matches_article_url() {
    assert!(matches("https://dev.to/johndoe/my-cool-article-123"));
    assert!(matches("https://dev.to/rustlang/async-rust-tips-abc"));
    // Non-article paths should not match
    assert!(!matches("https://dev.to/t/rust"));
    assert!(!matches("https://dev.to/search?q=rust"));
    assert!(!matches("https://dev.to/johndoe"));
}

#[test]
fn test_build_extra_fields() {
    let tags = vec!["rust", "webdev", "tutorial"];
    let extra = build_extra("johndoe", &tags, 42, 5, "2024-03-01T12:00:00Z");
    assert_eq!(extra["devto_author"], "johndoe");
    assert_eq!(extra["devto_reactions"], 42u64);
    assert_eq!(extra["devto_reading_time_mins"], 5u64);
    assert_eq!(extra["devto_tags"].as_array().unwrap().len(), 3);
    assert_eq!(extra["devto_published_at"], "2024-03-01T12:00:00Z");

    // Empty optional fields should not appear
    let extra_minimal = build_extra("user", &[], 0, 0, "");
    assert!(extra_minimal.get("devto_tags").is_none());
    assert!(extra_minimal.get("devto_published_at").is_none());
}

#[test]
fn test_article_detail_api_url_uses_numeric_id() {
    assert_eq!(
        article_detail_api_url(3348526),
        "https://dev.to/api/articles/3348526"
    );
}

#[test]
fn test_select_article_body_prefers_full_markdown() {
    let listing_shape = serde_json::json!({
        "description": "Listing summary only",
        "body_markdown": null
    });
    assert_eq!(select_article_body(&listing_shape), "Listing summary only");

    let detail_shape = serde_json::json!({
        "description": "Listing summary only",
        "body_markdown": "Full article body"
    });
    assert_eq!(select_article_body(&detail_shape), "Full article body");
}
