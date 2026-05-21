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
