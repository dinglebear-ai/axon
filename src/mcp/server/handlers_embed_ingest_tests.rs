use super::*;

#[test]
fn parse_ingest_source_normalizes_github_url() {
    let cfg = Config::default();
    let source = parse_ingest_source(
        Some(IngestSourceType::Github),
        Some("https://github.com/owner/repo.git".to_string()),
        None,
        Some(false),
        &cfg,
    )
    .expect("valid github target");
    assert!(matches!(
        source,
        IngestSource::Github {
            repo,
            include_source: false,
        } if repo == "owner/repo"
    ));
}

#[test]
fn parse_ingest_source_rejects_invalid_github_target() {
    let cfg = Config::default();
    let err = parse_ingest_source(
        Some(IngestSourceType::Github),
        Some("owner/repo/extra".to_string()),
        None,
        None,
        &cfg,
    )
    .expect_err("invalid target should fail");
    assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
}

#[test]
fn parse_ingest_source_normalizes_gitlab_url() {
    let cfg = Config::default();
    let source = parse_ingest_source(
        Some(IngestSourceType::Gitlab),
        Some("https://gitlab.com/group/subgroup/project/-/issues/1".to_string()),
        None,
        Some(false),
        &cfg,
    )
    .expect("valid gitlab target");
    assert!(matches!(
        source,
        IngestSource::Gitlab {
            target,
            include_source: false,
        } if target == "gitlab.com/group/subgroup/project"
    ));
}

#[test]
fn parse_ingest_source_normalizes_gitea_target() {
    let cfg = Config::default();
    let source = parse_ingest_source(
        Some(IngestSourceType::Gitea),
        Some("gitea:gitea.example.com/org/repo.git".to_string()),
        None,
        Some(false),
        &cfg,
    )
    .expect("valid gitea target");
    assert!(matches!(
        source,
        IngestSource::Gitea {
            target,
            include_source: false,
        } if target == "gitea.example.com/org/repo"
    ));
}

#[test]
fn parse_ingest_source_normalizes_generic_git_target() {
    let cfg = Config::default();
    let source = parse_ingest_source(
        Some(IngestSourceType::Git),
        Some("git:https://example.com/org/repo.git".to_string()),
        None,
        Some(false),
        &cfg,
    )
    .expect("valid generic git target");
    assert!(matches!(
        source,
        IngestSource::GenericGit {
            target,
            include_source: false,
        } if target == "https://example.com/org/repo.git"
    ));
}

#[test]
fn parse_ingest_source_rejects_non_reddit_comments_url() {
    let cfg = Config::default();
    let err = parse_ingest_source(
        Some(IngestSourceType::Reddit),
        Some("https://example.com/r/rust/comments/abc/title".to_string()),
        None,
        None,
        &cfg,
    )
    .expect_err("non-reddit thread URL should fail");
    assert_eq!(err.code, rmcp::model::ErrorCode::INVALID_PARAMS);
}

#[test]
fn parse_ingest_source_accepts_youtube_handle() {
    let cfg = Config::default();
    let source = parse_ingest_source(
        Some(IngestSourceType::Youtube),
        Some("https://www.youtube.com/@SpaceinvaderOne".to_string()),
        None,
        None,
        &cfg,
    )
    .expect("valid youtube channel target");
    assert!(
        matches!(source, IngestSource::Youtube { target } if target.contains("@SpaceinvaderOne"))
    );
}
