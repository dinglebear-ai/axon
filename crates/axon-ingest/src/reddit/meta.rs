use serde_json::{Value, json};

/// Build Qdrant extra payload fields for a Reddit post chunk.
///
/// Fields: `reddit_author`, `reddit_created_utc`, `reddit_score`, `reddit_num_comments`,
/// `reddit_upvote_ratio`, `reddit_subreddit`, `reddit_domain`, `reddit_is_video`,
/// `reddit_distinguished`, `reddit_gilded`, `reddit_flair`.
///
/// `data` is the `post["data"]` object from the Reddit API JSON response.
pub fn build_reddit_post_extra_payload(data: &Value) -> Value {
    json!({
        "reddit_author": data["author"].as_str().unwrap_or("[deleted]"),
        // Reddit API returns `created_utc` as a float (e.g. 1710000000.0).
        // `as_u64()` returns `None` for JSON floats, so parse as f64 then cast.
        "reddit_created_utc": data["created_utc"].as_f64().map(|f| f as u64).unwrap_or(0),
        "reddit_score": data["score"].as_i64().unwrap_or(0),
        "reddit_num_comments": data["num_comments"].as_u64().unwrap_or(0),
        "reddit_upvote_ratio": data["upvote_ratio"].as_f64().unwrap_or(0.0),
        "reddit_subreddit": data["subreddit"].as_str().unwrap_or(""),
        "reddit_domain": data["domain"].as_str().unwrap_or(""),
        "reddit_is_video": data["is_video"].as_bool().unwrap_or(false),
        "reddit_distinguished": data["distinguished"].as_str(),
        "reddit_gilded": data["gilded"].as_u64().unwrap_or(0),
        "reddit_flair": data["link_flair_text"].as_str(),
    })
}
