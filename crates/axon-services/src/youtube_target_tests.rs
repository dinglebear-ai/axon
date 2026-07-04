use super::*;

#[test]
fn watch_url_is_youtube() {
    assert!(is_youtube_target(
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
    ));
    assert!(is_youtube_target("https://youtube.com/watch?v=dQw4w9WgXcQ"));
    assert!(is_youtube_target(
        "https://m.youtube.com/watch?v=dQw4w9WgXcQ"
    ));
}

#[test]
fn short_url_is_youtube() {
    assert!(is_youtube_target("https://youtu.be/dQw4w9WgXcQ"));
}

#[test]
fn handle_is_youtube() {
    // A bare @handle expands to a channel URL and counts as youtube.
    assert!(is_youtube_target("@RickAstleyYT"));
}

#[test]
fn playlist_and_channel_urls_are_youtube() {
    assert!(is_youtube_target(
        "https://www.youtube.com/playlist?list=PL1234567890"
    ));
    assert!(is_youtube_target("https://www.youtube.com/channel/UC1234"));
    assert!(is_youtube_target("https://www.youtube.com/@RickAstleyYT"));
}

#[test]
fn bare_11_char_video_id_is_youtube() {
    // The canonical bare-id case (mixes `-`/`_`) — the highest-risk precedence
    // case against reddit's bare-subreddit rule.
    assert!(is_youtube_target("dQw4w9WgXcQ"));
    // An 11-char id that is all alphanumeric/`_` (no hyphen) ALSO satisfies
    // reddit's bare-name rule; youtube's more specific 11-char check must win,
    // which is enforced by classification order (tested in `source_tests`).
    assert!(is_youtube_target("abcdefghijk"));
}

#[test]
fn non_11_char_bare_word_is_not_youtube() {
    // 4, 10, and 16 chars: none is exactly 11, so none is a bare video id.
    assert!(!is_youtube_target("rust"));
    assert!(!is_youtube_target("just-a-wor")); // 10 chars
    assert!(!is_youtube_target("learnprogramming")); // 16 chars
}

#[test]
fn non_youtube_web_url_is_not_youtube() {
    assert!(!is_youtube_target("https://docs.example.com/guide"));
    assert!(!is_youtube_target("http://example.com"));
}

#[test]
fn github_url_is_not_youtube() {
    assert!(!is_youtube_target("https://github.com/jmagar/axon"));
    assert!(!is_youtube_target("https://github.com/jmagar/axon.git"));
}

#[test]
fn reddit_url_is_not_youtube() {
    // A reddit thread URL carries no youtube signal.
    assert!(!is_youtube_target(
        "https://www.reddit.com/r/rust/comments/abc123/t/"
    ));
    assert!(!is_youtube_target("r/rust"));
}
