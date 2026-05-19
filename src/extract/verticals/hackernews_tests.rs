use super::*;

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
