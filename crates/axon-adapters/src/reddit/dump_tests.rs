use super::*;

fn sample_item_json() -> serde_json::Value {
    serde_json::json!({
        "title": "Rust chunking",
        "selftext": "Post body",
        "permalink": "/r/rust/comments/abc123/rust_chunking/",
        "author": "alice",
        "score": 42,
        "subreddit": "rust",
        "domain": "self.rust",
        "num_comments": 2,
        "upvote_ratio": 0.97,
        "is_video": false,
        "distinguished": null,
        "gilded": 0,
        "link_flair_text": "Discussion",
        "created_utc": 1767225600,
        "comments": [
            {"body": "Great post!", "parent_text": null},
            {"body": "Agreed.", "parent_text": "Great post!"}
        ]
    })
}

#[test]
fn parses_valid_dump_array() {
    let dump = serde_json::to_vec(&vec![sample_item_json()]).unwrap();
    let items = parse_dump(&dump).unwrap();
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item.title.as_deref(), Some("Rust chunking"));
    assert_eq!(item.author.as_deref(), Some("alice"));
    assert_eq!(item.score, Some(42));
    assert_eq!(item.comments.len(), 2);
}

#[test]
fn rejects_malformed_json() {
    let err = parse_dump(b"{not valid json").unwrap_err();
    assert_eq!(err.code.to_string(), "adapter.reddit.dump_invalid");
}

#[test]
fn rejects_non_array_top_level() {
    let dump = serde_json::to_vec(&sample_item_json()).unwrap();
    let err = parse_dump(&dump).unwrap_err();
    assert_eq!(err.code.to_string(), "adapter.reddit.dump_invalid");
}

#[test]
fn parses_empty_array_dump() {
    let items = parse_dump(b"[]").unwrap();
    assert!(items.is_empty());
}

#[test]
fn missing_optional_fields_default_gracefully() {
    let minimal = serde_json::json!([{
        "title": "Minimal Post",
        "permalink": "/r/rust/comments/xyz/minimal/"
    }]);
    let dump = serde_json::to_vec(&minimal).unwrap();
    let items = parse_dump(&dump).unwrap();
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item.author_or_deleted(), "[deleted]");
    assert_eq!(item.score, None);
    assert!(item.comments.is_empty());
}

#[test]
fn author_or_deleted_falls_back_on_empty_string() {
    let mut item_json = sample_item_json();
    item_json["author"] = serde_json::json!("");
    let dump = serde_json::to_vec(&vec![item_json]).unwrap();
    let items = parse_dump(&dump).unwrap();
    assert_eq!(items[0].author_or_deleted(), "[deleted]");
}

#[test]
fn render_content_includes_title_selftext_and_comments() {
    let dump = serde_json::to_vec(&vec![sample_item_json()]).unwrap();
    let items = parse_dump(&dump).unwrap();
    let content = items[0].render_content();
    assert!(content.starts_with("# Rust chunking"));
    assert!(content.contains("Post body"));
    assert!(content.contains("Great post!"));
    assert!(content.contains("Replying to: Great post!"));
    assert!(content.contains("Agreed."));
}

#[test]
fn render_content_without_comments_or_selftext() {
    let minimal = serde_json::json!([{
        "title": "Link only",
        "permalink": "/r/rust/comments/xyz/link_only/"
    }]);
    let dump = serde_json::to_vec(&minimal).unwrap();
    let items = parse_dump(&dump).unwrap();
    let content = items[0].render_content();
    assert_eq!(content, "# Link only");
}

#[test]
fn canonical_url_prefixes_reddit_domain() {
    let dump = serde_json::to_vec(&vec![sample_item_json()]).unwrap();
    let items = parse_dump(&dump).unwrap();
    assert_eq!(
        items[0].canonical_url(),
        "https://www.reddit.com/r/rust/comments/abc123/rust_chunking/"
    );
}
