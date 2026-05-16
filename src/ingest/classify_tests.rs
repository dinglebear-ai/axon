use super::*;

#[test]
fn github_slug() {
    assert!(matches!(
        classify_target("jmagar/axon", false),
        Ok(IngestSource::Github { .. })
    ));
}

#[test]
fn github_slug_with_dots() {
    assert!(matches!(
        classify_target("rust-lang/rust", false),
        Ok(IngestSource::Github { .. })
    ));
}

#[test]
fn github_url() {
    assert!(matches!(
        classify_target("https://github.com/anthropics/claude-code", false),
        Ok(IngestSource::Github { .. })
    ));
}

#[test]
fn github_url_with_trailing_slash() {
    let r = classify_target("https://github.com/rust-lang/rust/", false).unwrap();
    if let IngestSource::Github { repo, .. } = r {
        assert_eq!(repo, "rust-lang/rust");
    } else {
        panic!("expected Github variant");
    }
}

#[test]
fn github_url_with_subpath() {
    // Deep URL — should extract just owner/repo
    let r = classify_target("https://github.com/rust-lang/rust/issues/123", false).unwrap();
    if let IngestSource::Github { repo, .. } = r {
        assert_eq!(repo, "rust-lang/rust");
    } else {
        panic!("expected Github variant");
    }
}

#[test]
fn github_include_source_propagated() {
    let r = classify_target("jmagar/axon", true).unwrap();
    if let IngestSource::Github { include_source, .. } = r {
        assert!(include_source);
    } else {
        panic!("expected Github variant");
    }
}

#[test]
fn youtube_full_url() {
    assert!(matches!(
        classify_target("https://www.youtube.com/watch?v=dQw4w9WgXcQ", false),
        Ok(IngestSource::Youtube { .. })
    ));
}

#[test]
fn youtube_short_url() {
    assert!(matches!(
        classify_target("https://youtu.be/dQw4w9WgXcQ", false),
        Ok(IngestSource::Youtube { .. })
    ));
}

#[test]
fn youtube_handle_expansion() {
    let r = classify_target("@SpaceinvaderOne", false).unwrap();
    if let IngestSource::Youtube { target } = r {
        assert_eq!(target, "https://www.youtube.com/@SpaceinvaderOne");
    } else {
        panic!("expected Youtube variant");
    }
}

#[test]
fn youtube_bare_video_id() {
    assert!(matches!(
        classify_target("dQw4w9WgXcQ", false),
        Ok(IngestSource::Youtube { .. })
    ));
}

#[test]
fn youtube_mobile_url() {
    assert!(matches!(
        classify_target("https://m.youtube.com/watch?v=dQw4w9WgXcQ", false),
        Ok(IngestSource::Youtube { .. })
    ));
}

#[test]
fn reddit_subreddit_prefix() {
    assert!(matches!(
        classify_target("r/self-hosted", false),
        Ok(IngestSource::Reddit { .. })
    ));
}

#[test]
fn reddit_full_url() {
    assert!(matches!(
        classify_target("https://www.reddit.com/r/rust/", false),
        Ok(IngestSource::Reddit { .. })
    ));
}

#[test]
fn reddit_old_subdomain() {
    assert!(matches!(
        classify_target("https://old.reddit.com/r/unraid", false),
        Ok(IngestSource::Reddit { .. })
    ));
}

#[test]
fn unknown_target_returns_error() {
    assert!(classify_target("not-a-target", false).is_err());
}

#[test]
fn empty_string_returns_error() {
    assert!(classify_target("", false).is_err());
}

#[test]
fn bare_word_not_slug_returns_error() {
    // Single word without slash that is not 11 chars — not a valid GitHub slug or video ID
    assert!(classify_target("abc", false).is_err());
    assert!(classify_target("toolongforvideoidsomethingrandom", false).is_err());
}
