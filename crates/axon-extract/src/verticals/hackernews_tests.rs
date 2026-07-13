use super::*;

#[test]
fn infer_hn_type_job() {
    assert_eq!(infer_hn_type(Some("job"), "Acme is hiring"), "job");
}

#[test]
fn infer_hn_type_ask_hn() {
    assert_eq!(
        infer_hn_type(None, "Ask HN: What is your favorite editor?"),
        "ask_hn"
    );
}

#[test]
fn infer_hn_type_show_hn() {
    assert_eq!(
        infer_hn_type(Some("story"), "Show HN: My new project"),
        "show_hn"
    );
}

#[test]
fn infer_hn_type_default_story() {
    assert_eq!(infer_hn_type(None, "Some interesting article"), "story");
    assert_eq!(infer_hn_type(Some("story"), "Another post"), "story");
}

#[test]
fn build_extra_sets_required_fields() {
    let extra = build_extra(
        42,
        "story",
        "testuser",
        100,
        50,
        "2024-01-01T00:00:00Z",
        None,
    );
    assert_eq!(extra["hn_id"], 42u64);
    assert_eq!(extra["hn_type"], "story");
    assert_eq!(extra["hn_author"], "testuser");
    assert_eq!(extra["hn_points"], 100u64);
    assert_eq!(extra["hn_comment_count"], 50u64);
    assert_eq!(extra["hn_created_at"], "2024-01-01T00:00:00Z");
    assert!(extra["hn_external_url"].is_null());
}

#[test]
fn build_extra_with_external_url() {
    let extra = build_extra(99, "story", "user", 5, 2, "", Some("https://example.com"));
    assert_eq!(extra["hn_external_url"], "https://example.com");
    assert!(extra.get("hn_created_at").is_none());
}

#[test]
fn build_extra_empty_created_at_omitted() {
    let extra = build_extra(1, "ask_hn", "foo", 0, 0, "", None);
    assert!(extra.get("hn_created_at").is_none());
}

#[test]
fn matches_ycombinator_item() {
    assert!(matches("https://news.ycombinator.com/item?id=12345"));
    assert!(matches("https://news.ycombinator.com/item?id=99999999"));
}

#[test]
fn rejects_ycombinator_other_paths() {
    assert!(!matches("https://news.ycombinator.com/"));
    assert!(!matches("https://news.ycombinator.com/news"));
    assert!(!matches("https://news.ycombinator.com/item"));
    assert!(!matches("https://news.ycombinator.com/item?foo=bar"));
}

#[test]
fn matches_algolia_items() {
    assert!(matches("https://hn.algolia.com/items/12345"));
}

#[test]
fn rejects_algolia_other_paths() {
    assert!(!matches("https://hn.algolia.com/"));
    assert!(!matches("https://hn.algolia.com/items/"));
    assert!(!matches("https://hn.algolia.com/search?q=rust"));
}

#[test]
fn extract_item_id_ycombinator() {
    let id = extract_item_id("https://news.ycombinator.com/item?id=42424242");
    assert_eq!(id, Some(42424242));
}

#[test]
fn extract_item_id_algolia() {
    let id = extract_item_id("https://hn.algolia.com/items/9876");
    assert_eq!(id, Some(9876));
}

#[test]
fn strip_html_basic() {
    let html = r#"<p>Hello <a href="/">world</a> &amp; friends</p>"#;
    assert_eq!(strip_html_tags(html), "Hello world & friends");
}

#[test]
fn count_comments_recursive() {
    let item = HnItem {
        id: Some(1),
        item_type: Some("story".into()),
        title: Some("Test".into()),
        url: None,
        author: Some("user".into()),
        points: Some(10),
        text: None,
        created_at: None,
        children: vec![HnItem {
            id: Some(2),
            item_type: Some("comment".into()),
            title: None,
            url: None,
            author: Some("commenter".into()),
            points: None,
            text: Some("reply".into()),
            created_at: None,
            children: vec![HnItem {
                id: Some(3),
                item_type: Some("comment".into()),
                title: None,
                url: None,
                author: Some("nested".into()),
                points: None,
                text: Some("nested reply".into()),
                created_at: None,
                children: vec![],
            }],
        }],
    };
    assert_eq!(count_comments(&item), 2);
}
