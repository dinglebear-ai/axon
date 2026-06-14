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
    // Repo slug containing a dot (e.g. "my.project") should be accepted as a GitHub slug.
    assert!(matches!(
        classify_target("owner/my.project", false),
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
fn gitlab_url() {
    let r = classify_target("https://gitlab.com/gitlab-org/gitlab-runner", false).unwrap();
    if let IngestSource::Gitlab { target, .. } = r {
        assert_eq!(target, "gitlab.com/gitlab-org/gitlab-runner");
    } else {
        panic!("expected Gitlab variant");
    }
}

#[test]
fn gitlab_nested_namespace_url() {
    let r = classify_target("https://gitlab.com/group/subgroup/project", true).unwrap();
    if let IngestSource::Gitlab {
        target,
        include_source,
    } = r
    {
        assert_eq!(target, "gitlab.com/group/subgroup/project");
        assert!(include_source);
    } else {
        panic!("expected Gitlab variant");
    }
}

#[test]
fn gitlab_explicit_target() {
    let r = classify_target("gitlab:gitlab.com/group/subgroup/project", false).unwrap();
    if let IngestSource::Gitlab { target, .. } = r {
        assert_eq!(target, "gitlab.com/group/subgroup/project");
    } else {
        panic!("expected Gitlab variant");
    }
}

#[test]
fn gitea_explicit_target() {
    let r = classify_target("gitea:gitea.example.com/org/repo", false).unwrap();
    if let IngestSource::Gitea { target, .. } = r {
        assert_eq!(target, "gitea.example.com/org/repo");
    } else {
        panic!("expected Gitea variant");
    }
}

#[test]
fn forgejo_codeberg_url() {
    let r = classify_target("https://codeberg.org/forgejo/forgejo", true).unwrap();
    if let IngestSource::Gitea {
        target,
        include_source,
    } = r
    {
        assert_eq!(target, "codeberg.org/forgejo/forgejo");
        assert!(include_source);
    } else {
        panic!("expected Gitea variant");
    }
}

#[test]
fn generic_git_explicit_target() {
    let r = classify_target("git:https://example.com/org/repo.git", false).unwrap();
    if let IngestSource::GenericGit { target, .. } = r {
        assert_eq!(target, "https://example.com/org/repo.git");
    } else {
        panic!("expected GenericGit variant");
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
fn rss_explicit_prefix_adds_scheme() {
    let r = classify_target("rss:example.com/feed", false).unwrap();
    if let IngestSource::Rss { target } = r {
        assert_eq!(target, "https://example.com/feed");
    } else {
        panic!("expected Rss variant");
    }
}

#[test]
fn rss_feed_prefix_with_scheme_preserved() {
    let r = classify_target("feed:https://blog.example.com/atom.xml", false).unwrap();
    if let IngestSource::Rss { target } = r {
        assert_eq!(target, "https://blog.example.com/atom.xml");
    } else {
        panic!("expected Rss variant");
    }
}

#[test]
fn rss_atom_extension_url() {
    assert!(matches!(
        classify_target("https://blog.rust-lang.org/feed.xml", false),
        Ok(IngestSource::Rss { .. })
    ));
    assert!(matches!(
        classify_target("https://example.com/releases.atom", false),
        Ok(IngestSource::Rss { .. })
    ));
}

#[test]
fn rss_feed_path_segment_and_query() {
    assert!(matches!(
        classify_target("https://example.com/blog/feed/", false),
        Ok(IngestSource::Rss { .. })
    ));
    assert!(matches!(
        classify_target("https://example.com/?feed=rss2", false),
        Ok(IngestSource::Rss { .. })
    ));
}

#[test]
fn non_feed_url_does_not_classify_as_rss() {
    // A plain content URL must not be misrouted to RSS.
    assert!(classify_target("https://example.com/about", false).is_err());
}

#[test]
fn feedback_query_is_not_a_feed() {
    // `?feedback=1` contains the substring "feed" but is not a feed parameter.
    assert!(classify_target("https://example.com/page?feedback=1", false).is_err());
}

#[test]
fn category_atom_value_is_not_a_feed() {
    // A feed-shaped value under a non-format key (e.g. `?category=atom`) must
    // not be misrouted to RSS.
    assert!(classify_target("https://example.com/posts?category=atom", false).is_err());
}

#[test]
fn format_atom_query_is_a_feed() {
    assert!(matches!(
        classify_target("https://example.com/posts?format=atom", false),
        Ok(IngestSource::Rss { .. })
    ));
}

#[test]
fn github_releases_atom_classifies_as_rss() {
    // A `.atom` feed under a github.com path is a feed, not a repo — the feed
    // check runs before the GitHub host/slug branches (classify.rs comment).
    assert!(matches!(
        classify_target(
            "https://github.com/anthropics/claude-code/releases.atom",
            false
        ),
        Ok(IngestSource::Rss { .. })
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
