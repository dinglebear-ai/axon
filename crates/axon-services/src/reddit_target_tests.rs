use super::*;

#[test]
fn subreddit_prefix_is_reddit() {
    assert!(is_reddit_target("r/rust"));
    assert!(is_reddit_target("/r/rust"));
    assert!(is_reddit_target("r/rust/hot"));
}

#[test]
fn bare_valid_subreddit_name_is_reddit() {
    // A syntactically valid subreddit name (3-21 chars, alphanumeric/_) counts.
    assert!(is_reddit_target("rust"));
    assert!(is_reddit_target("learnprogramming"));
}

#[test]
fn reddit_com_thread_url_is_reddit() {
    assert!(is_reddit_target(
        "https://www.reddit.com/r/rust/comments/abc123/some_title/"
    ));
    assert!(is_reddit_target(
        "https://reddit.com/r/rust/comments/abc123/some_title/"
    ));
    assert!(is_reddit_target(
        "https://old.reddit.com/r/rust/comments/abc123/t/"
    ));
}

#[test]
fn non_reddit_web_url_is_not_reddit() {
    // Plain web URLs and non-reddit hosts must not be classified as reddit, so
    // they fall through to the web branch.
    assert!(!is_reddit_target("https://docs.example.com/guide"));
    assert!(!is_reddit_target("http://example.com"));
    assert!(!is_reddit_target(
        "https://example.com/r/rust/comments/abc123/"
    ));
}

#[test]
fn github_url_is_not_reddit() {
    // A git host carries no reddit signal (git is checked first anyway).
    assert!(!is_reddit_target("https://github.com/jmagar/axon"));
    assert!(!is_reddit_target("https://github.com/jmagar/axon.git"));
}

#[test]
fn invalid_subreddit_shapes_are_not_reddit() {
    // Too short, hyphenated, or otherwise invalid subreddit names are rejected
    // so ordinary CLI words don't accidentally route to reddit.
    assert!(!is_reddit_target("ab")); // too short
    assert!(!is_reddit_target("not-a-path-or-url")); // hyphens disallowed
    assert!(!is_reddit_target("")); // empty
}
