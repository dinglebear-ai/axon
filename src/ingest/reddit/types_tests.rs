use super::{RedditTarget, classify_target, validate_subreddit};

#[test]
fn classify_bare_subreddit_name() {
    assert_eq!(
        classify_target("rust").unwrap(),
        RedditTarget::Subreddit("rust".to_string())
    );
}

#[test]
fn classify_subreddit_name_with_r_prefix() {
    assert_eq!(
        classify_target("r/rust").unwrap(),
        RedditTarget::Subreddit("rust".to_string())
    );
}

#[test]
fn classify_subreddit_name_with_leading_slash() {
    assert_eq!(
        classify_target("/r/rust").unwrap(),
        RedditTarget::Subreddit("rust".to_string())
    );
}

#[test]
fn classify_thread_url() {
    let url = "https://www.reddit.com/r/rust/comments/abc123/some_title/";
    assert_eq!(
        classify_target(url).unwrap(),
        RedditTarget::Thread("/r/rust/comments/abc123/some_title/".to_string())
    );
}

#[test]
fn classify_old_reddit_thread_url() {
    let url = "https://old.reddit.com/r/rust/comments/abc123/some_title/";
    assert_eq!(
        classify_target(url).unwrap(),
        RedditTarget::Thread("/r/rust/comments/abc123/some_title/".to_string())
    );
}

#[test]
fn classify_reddit_thread_strips_query_and_fragment() {
    let url = "https://reddit.com/r/rust/comments/abc123/some_title/?utm_source=share#thing";
    assert_eq!(
        classify_target(url).unwrap(),
        RedditTarget::Thread("/r/rust/comments/abc123/some_title/".to_string())
    );
}

#[test]
fn classify_permalink_like_thread() {
    assert_eq!(
        classify_target("/r/rust/comments/abc123/some_title/").unwrap(),
        RedditTarget::Thread("/r/rust/comments/abc123/some_title/".to_string())
    );
    assert_eq!(
        classify_target("r/rust/comments/abc123/some_title/").unwrap(),
        RedditTarget::Thread("/r/rust/comments/abc123/some_title/".to_string())
    );
}

#[test]
fn classify_permalink_like_thread_strips_query() {
    assert_eq!(
        classify_target("/r/rust/comments/abc123/some_title/?context=3").unwrap(),
        RedditTarget::Thread("/r/rust/comments/abc123/some_title/".to_string())
    );
}

#[test]
fn reject_non_reddit_comments_url() {
    assert!(classify_target("https://example.com/r/rust/comments/abc123/title/").is_err());
    assert!(classify_target("https://notreddit.com/comments/abc123/title/").is_err());
}

#[test]
fn classify_subreddit_name_with_underscores() {
    assert_eq!(
        classify_target("rust_gamedev").unwrap(),
        RedditTarget::Subreddit("rust_gamedev".to_string())
    );
}

#[test]
fn classify_full_subreddit_url() {
    assert_eq!(
        classify_target("https://www.reddit.com/r/rust/").unwrap(),
        RedditTarget::Subreddit("rust".to_string())
    );
}

#[test]
fn classify_full_subreddit_url_no_trailing_slash() {
    assert_eq!(
        classify_target("https://www.reddit.com/r/rust").unwrap(),
        RedditTarget::Subreddit("rust".to_string())
    );
}

#[test]
fn classify_full_subreddit_url_no_www() {
    assert_eq!(
        classify_target("https://reddit.com/r/programming/").unwrap(),
        RedditTarget::Subreddit("programming".to_string())
    );
}

#[test]
fn classify_old_reddit_subreddit_url() {
    assert_eq!(
        classify_target("https://old.reddit.com/r/rust/").unwrap(),
        RedditTarget::Subreddit("rust".to_string())
    );
}

#[test]
fn validate_subreddit_accepts_valid_names() {
    assert!(validate_subreddit("rust").is_ok());
    assert!(validate_subreddit("rust_gamedev").is_ok());
    assert!(validate_subreddit("AskReddit").is_ok());
    assert!(validate_subreddit("abc").is_ok());
}

#[test]
fn validate_subreddit_rejects_path_traversal() {
    assert!(validate_subreddit("../../../etc/passwd").is_err());
    assert!(validate_subreddit("rust/../../admin").is_err());
}

#[test]
fn validate_subreddit_rejects_too_short() {
    assert!(validate_subreddit("ab").is_err());
    assert!(validate_subreddit("a").is_err());
    assert!(validate_subreddit("").is_err());
}

#[test]
fn validate_subreddit_rejects_too_long() {
    assert!(validate_subreddit("abcdefghijklmnopqrstuv").is_err());
}

#[test]
fn validate_subreddit_rejects_special_chars() {
    assert!(validate_subreddit("rust-lang").is_err());
    assert!(validate_subreddit("rust.lang").is_err());
    assert!(validate_subreddit("rust lang").is_err());
}

#[test]
fn min_length_boundary() {
    assert!(validate_subreddit("ab").is_err());
    assert!(validate_subreddit("abc").is_ok());
}

#[test]
fn max_length_boundary() {
    assert!(validate_subreddit(&"a".repeat(21)).is_ok());
    assert!(validate_subreddit(&"a".repeat(22)).is_err());
}

#[test]
fn rejects_null_byte() {
    assert!(validate_subreddit("rust\0hack").is_err());
}

#[test]
fn rejects_unicode() {
    assert!(validate_subreddit("r\u{fc}st").is_err());
    assert!(validate_subreddit("caf\u{e9}").is_err());
}
