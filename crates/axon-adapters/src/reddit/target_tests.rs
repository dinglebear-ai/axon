use super::*;

#[test]
fn parses_bare_subreddit_name() {
    let t = parse_reddit_target("rust").unwrap();
    assert_eq!(t, RedditTarget::Subreddit("rust".to_string()));
}

#[test]
fn parses_r_prefixed_subreddit() {
    assert_eq!(
        parse_reddit_target("r/rust").unwrap(),
        RedditTarget::Subreddit("rust".to_string())
    );
    assert_eq!(
        parse_reddit_target("/r/rust").unwrap(),
        RedditTarget::Subreddit("rust".to_string())
    );
}

#[test]
fn parses_full_subreddit_url() {
    let t = parse_reddit_target("https://www.reddit.com/r/rust").unwrap();
    assert_eq!(t, RedditTarget::Subreddit("rust".to_string()));
}

#[test]
fn parses_thread_url_with_title_and_id() {
    let t = parse_reddit_target("https://www.reddit.com/r/rust/comments/abc123/some_title_here/")
        .unwrap();
    assert_eq!(
        t,
        RedditTarget::Thread("/r/rust/comments/abc123/some_title_here/".to_string())
    );
}

#[test]
fn parses_thread_path_without_scheme() {
    let t = parse_reddit_target("/r/rust/comments/abc123/some_title/").unwrap();
    assert_eq!(
        t,
        RedditTarget::Thread("/r/rust/comments/abc123/some_title/".to_string())
    );
}

#[test]
fn parses_bare_thread_path_with_r_prefix() {
    let t = parse_reddit_target("r/rust/comments/abc123/some_title/").unwrap();
    assert_eq!(
        t,
        RedditTarget::Thread("/r/rust/comments/abc123/some_title/".to_string())
    );
}

#[test]
fn thread_without_title_still_parses() {
    let t = parse_reddit_target("https://www.reddit.com/r/rust/comments/abc123/").unwrap();
    assert_eq!(
        t,
        RedditTarget::Thread("/r/rust/comments/abc123/".to_string())
    );
}

#[test]
fn thread_with_comment_id_preserves_it() {
    let t =
        parse_reddit_target("https://www.reddit.com/r/rust/comments/abc123/title/def456/").unwrap();
    assert_eq!(
        t,
        RedditTarget::Thread("/r/rust/comments/abc123/title/def456/".to_string())
    );
}

#[test]
fn rejects_non_reddit_host() {
    assert!(parse_reddit_target("https://evil.example.com/r/rust").is_err());
}

#[test]
fn rejects_non_reddit_comments_url() {
    let err = parse_reddit_target("https://evil.example.com/r/rust/comments/abc123/").unwrap_err();
    assert_eq!(err.code.to_string(), "adapter.reddit.target.host_invalid");
}

#[test]
fn rejects_non_reddit_bare_comments_target() {
    assert!(parse_reddit_target("evil.example.com/comments/abc123/").is_err());
}

#[test]
fn rejects_empty_target() {
    let err = parse_reddit_target("").unwrap_err();
    assert_eq!(err.code.to_string(), "adapter.reddit.target.empty");
}

#[test]
fn rejects_invalid_subreddit_length() {
    assert!(parse_reddit_target("ab").is_err());
    assert!(parse_reddit_target(&"a".repeat(22)).is_err());
}

#[test]
fn rejects_invalid_subreddit_characters() {
    assert!(parse_reddit_target("rust-lang").is_err());
    assert!(parse_reddit_target("rust lang").is_err());
}

#[test]
fn non_comments_path_under_r_prefix_classifies_as_subreddit() {
    // Any `/r/<name>/...` path without a `/comments/` segment is treated as a
    // reference to the subreddit itself (mirrors the legacy classifier).
    let t = parse_reddit_target("https://www.reddit.com/r/rust/wiki/").unwrap();
    assert_eq!(t, RedditTarget::Subreddit("rust".to_string()));
}

#[test]
fn rejects_path_missing_r_prefix() {
    let err = parse_reddit_target("https://www.reddit.com/wiki/rust/").unwrap_err();
    assert_eq!(err.code.to_string(), "adapter.reddit.target.path_invalid");
}

#[test]
fn rejects_thread_id_with_invalid_characters() {
    let err = canonical_thread_permalink("/r/rust/comments/abc$123/title/").unwrap_err();
    assert_eq!(
        err.code.to_string(),
        "adapter.reddit.target.thread_id_invalid"
    );
}

#[test]
fn validate_subreddit_accepts_boundary_lengths() {
    assert!(validate_subreddit("abc").is_ok());
    assert!(validate_subreddit(&"a".repeat(21)).is_ok());
}
