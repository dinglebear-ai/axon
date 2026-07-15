use super::*;

#[tokio::test]
async fn classify_source_input_detects_reddit_targets() {
    assert_eq!(
        classify_source_input("r/rust").await,
        SourceInputKind::Reddit
    );
    assert_eq!(
        classify_source_input("https://www.reddit.com/r/rust/comments/abc123/some_title/").await,
        SourceInputKind::Reddit
    );
    assert_eq!(
        classify_source_input("https://old.reddit.com/r/rust/comments/abc123/t/").await,
        SourceInputKind::Reddit
    );
}

#[tokio::test]
async fn classify_source_input_reddit_thread_beats_web_url() {
    assert_eq!(
        classify_source_input("https://reddit.com/r/rust/comments/abc123/some_title/").await,
        SourceInputKind::Reddit
    );
}

#[tokio::test]
async fn classify_source_input_github_url_still_git_not_reddit() {
    assert_eq!(
        classify_source_input("https://github.com/jmagar/axon").await,
        SourceInputKind::Git
    );
}

#[tokio::test]
async fn classify_source_input_plain_web_url_is_not_reddit() {
    assert_eq!(
        classify_source_input("https://docs.example.com/guide").await,
        SourceInputKind::Web
    );
    assert_eq!(
        classify_source_input("http://example.com").await,
        SourceInputKind::Web
    );
}

#[tokio::test]
async fn classify_source_input_feed_url_still_feed_not_reddit() {
    assert_eq!(
        classify_source_input("https://example.com/feed.rss").await,
        SourceInputKind::Feed
    );
}

#[tokio::test]
async fn classify_source_input_prefers_existing_local_path() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let path = dir.path().to_string_lossy().to_string();
    assert_eq!(classify_source_input(&path).await, SourceInputKind::Local);
}

#[tokio::test]
async fn classify_source_input_detects_lexical_local_path_before_it_exists() {
    assert_eq!(
        classify_source_input("/tmp/axon-missing-local-source").await,
        SourceInputKind::Local
    );
    assert_eq!(
        classify_source_input("./missing-local-source").await,
        SourceInputKind::Local
    );
    assert_eq!(
        classify_source_input("../missing-local-source").await,
        SourceInputKind::Local
    );
    assert_eq!(
        classify_source_input("~/missing-local-source").await,
        SourceInputKind::Local
    );
}

#[tokio::test]
async fn classify_source_input_detects_git_url() {
    assert_eq!(
        classify_source_input("https://github.com/jmagar/axon.git").await,
        SourceInputKind::Git
    );
    assert_eq!(
        classify_source_input("https://gitlab.com/owner/repo").await,
        SourceInputKind::Git
    );
}

#[tokio::test]
async fn classify_source_input_detects_web_url() {
    assert_eq!(
        classify_source_input("https://docs.example.com/guide").await,
        SourceInputKind::Web
    );
    assert_eq!(
        classify_source_input("http://example.com").await,
        SourceInputKind::Web
    );
}

#[tokio::test]
async fn classify_source_input_git_url_beats_web_url() {
    assert_eq!(
        classify_source_input("https://github.com/jmagar/axon").await,
        SourceInputKind::Git
    );
}

#[tokio::test]
async fn classify_source_input_dot_git_on_unknown_host_is_git() {
    assert_eq!(
        classify_source_input("https://example.com/team/repo.git").await,
        SourceInputKind::Git
    );
}

#[tokio::test]
async fn classify_source_input_plain_https_path_is_web_not_git() {
    assert_eq!(
        classify_source_input("https://docs.example.com/team/guide").await,
        SourceInputKind::Web
    );
}

#[tokio::test]
async fn classify_source_input_rejects_plain_word() {
    assert_eq!(
        classify_source_input("not-a-path-or-url").await,
        SourceInputKind::Unsupported
    );
    assert_eq!(
        classify_source_input("ftp://example.com/file").await,
        SourceInputKind::Unsupported
    );
}

#[tokio::test]
async fn classify_source_input_detects_feed_urls() {
    assert_eq!(
        classify_source_input("https://example.com/blog/feed.rss").await,
        SourceInputKind::Feed
    );
    assert_eq!(
        classify_source_input("https://example.com/releases.atom").await,
        SourceInputKind::Feed
    );
    assert_eq!(
        classify_source_input("rss:https://example.com/blog").await,
        SourceInputKind::Feed
    );
}

#[tokio::test]
async fn classify_source_input_feed_url_beats_web_url() {
    assert_eq!(
        classify_source_input("https://example.com/feed").await,
        SourceInputKind::Feed
    );
}

#[tokio::test]
async fn classify_source_input_plain_web_url_is_not_feed() {
    assert_eq!(
        classify_source_input("https://docs.example.com/guide").await,
        SourceInputKind::Web
    );
}

#[tokio::test]
async fn classify_source_input_github_url_still_git_not_feed() {
    assert_eq!(
        classify_source_input("https://github.com/jmagar/axon").await,
        SourceInputKind::Git
    );
}

#[tokio::test]
async fn classify_source_input_detects_youtube_targets() {
    assert_eq!(
        classify_source_input("https://www.youtube.com/watch?v=dQw4w9WgXcQ").await,
        SourceInputKind::Youtube
    );
    assert_eq!(
        classify_source_input("https://youtu.be/dQw4w9WgXcQ").await,
        SourceInputKind::Youtube
    );
    assert_eq!(
        classify_source_input("@RickAstleyYT").await,
        SourceInputKind::Youtube
    );
    assert_eq!(
        classify_source_input("https://www.youtube.com/playlist?list=PL123").await,
        SourceInputKind::Youtube
    );
}

#[tokio::test]
async fn classify_source_input_bare_11_char_id_is_youtube() {
    // The #1 precedence risk: a bare 11-char video id must classify as Youtube,
    // NOT be mis-claimed as a subreddit by reddit's bare-name rule.
    assert_eq!(
        classify_source_input("dQw4w9WgXcQ").await,
        SourceInputKind::Youtube
    );
    assert_eq!(
        classify_source_input("abcdefghijk").await,
        SourceInputKind::Youtube
    );
}

#[tokio::test]
async fn classify_source_input_youtube_url_beats_web_url() {
    assert_eq!(
        classify_source_input("https://www.youtube.com/watch?v=dQw4w9WgXcQ").await,
        SourceInputKind::Youtube
    );
}

#[tokio::test]
async fn classify_source_input_github_url_still_git_not_youtube() {
    assert_eq!(
        classify_source_input("https://github.com/jmagar/axon").await,
        SourceInputKind::Git
    );
}

#[tokio::test]
async fn classify_source_input_reddit_url_still_reddit_not_youtube() {
    assert_eq!(
        classify_source_input("https://www.reddit.com/r/rust/comments/abc123/t/").await,
        SourceInputKind::Reddit
    );
    assert_eq!(
        classify_source_input("r/rust").await,
        SourceInputKind::Reddit
    );
    assert_eq!(
        classify_source_input("learnprogramming").await,
        SourceInputKind::Reddit
    );
}

#[tokio::test]
async fn classify_source_input_plain_web_url_is_not_youtube() {
    assert_eq!(
        classify_source_input("https://docs.example.com/guide").await,
        SourceInputKind::Web
    );
}

#[tokio::test]
async fn classify_source_input_detects_session_selector() {
    assert_eq!(
        classify_source_input("session:claude:/tmp/does-not-exist.jsonl").await,
        SourceInputKind::Session
    );
}

#[test]
fn session_source_requires_local_filesystem_safety() {
    assert_eq!(
        safety_class_for(SourceInputKind::Session),
        axon_api::source::SafetyClass::LocalFilesystem
    );
}

#[tokio::test]
async fn classify_source_input_detects_registry_target() {
    assert_eq!(
        classify_source_input("pkg:npm/left-pad").await,
        SourceInputKind::Registry
    );
}
