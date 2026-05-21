use serde_json::json;

use super::build_extra;

#[test]
fn build_extra_all_fields_present() {
    let post_data = json!({
        "author": "testuser",
        "created_utc": 1_710_000_000.0_f64,
        "score": 42,
        "num_comments": 7,
        "upvote_ratio": 0.95,
        "subreddit": "rust",
        "domain": "self.rust",
        "is_video": false,
        "distinguished": null,
        "gilded": 0,
        "link_flair_text": "Discussion"
    });
    let extra = build_extra(&post_data);
    assert_eq!(extra["reddit_author"], json!("testuser"));
    assert_eq!(extra["reddit_created_utc"], json!(1_710_000_000_u64));
    assert_eq!(extra["reddit_score"], json!(42_i64));
    assert_eq!(extra["reddit_num_comments"], json!(7_u64));
    assert_eq!(extra["reddit_upvote_ratio"], json!(0.95_f64));
    assert_eq!(extra["reddit_subreddit"], json!("rust"));
    assert_eq!(extra["reddit_domain"], json!("self.rust"));
    assert_eq!(extra["reddit_is_video"], json!(false));
    assert_eq!(extra["reddit_gilded"], json!(0_u64));
    assert_eq!(extra["reddit_flair"], json!("Discussion"));
}

#[test]
fn build_extra_missing_fields_use_defaults() {
    let post_data = json!({});
    let extra = build_extra(&post_data);
    assert_eq!(extra["reddit_author"], json!("[deleted]"));
    assert_eq!(extra["reddit_created_utc"], json!(0_u64));
    assert_eq!(extra["reddit_score"], json!(0_i64));
    assert_eq!(extra["reddit_num_comments"], json!(0_u64));
    assert_eq!(extra["reddit_upvote_ratio"], json!(0.0_f64));
    assert_eq!(extra["reddit_subreddit"], json!(""));
    assert_eq!(extra["reddit_domain"], json!(""));
    assert_eq!(extra["reddit_is_video"], json!(false));
    assert_eq!(extra["reddit_gilded"], json!(0_u64));
    // nullable fields should be JSON null when absent
    assert!(extra["reddit_distinguished"].is_null());
    assert!(extra["reddit_flair"].is_null());
}

#[test]
fn build_extra_distinguished_field() {
    let post_data = json!({
        "distinguished": "moderator"
    });
    let extra = build_extra(&post_data);
    assert_eq!(extra["reddit_distinguished"], json!("moderator"));
}

#[test]
fn build_extra_returns_object() {
    let extra = build_extra(&json!({}));
    assert!(extra.is_object(), "build_extra must return a JSON object");
}
