use axon_core::config::Config;
use axon_ingest as ingest;
use axon_jobs::ingest::IngestSource;

pub fn source_from_mcp_request(
    req: &axon_api::mcp_schema::IngestRequest,
    cfg: &Config,
) -> Result<IngestSource, String> {
    use axon_api::mcp_schema::IngestSourceType;

    // No explicit source_type → auto-detect from the raw target via the canonical
    // shared classifier. This is the single source of truth across CLI/MCP/REST/
    // palette: clients send a bare target (`owner/repo`, `gitlab.com/g/p`,
    // `r/rust`, a YouTube URL, a feed) and never reimplement classification.
    let Some(source_type) = req.source_type.clone() else {
        let target = required_ingest_target(req, "target")?;
        let include_source = req.include_source.unwrap_or(cfg.github_include_source);
        return ingest::classify::classify_target(&target, include_source)
            .map_err(|err| format!("could not auto-detect ingest source for '{target}': {err}"));
    };
    match source_type {
        IngestSourceType::Github => {
            let repo = validate_github_ingest_target(&required_ingest_target(req, "target repo")?)?;
            Ok(IngestSource::Github {
                repo,
                include_source: req.include_source.unwrap_or(cfg.github_include_source),
            })
        }
        IngestSourceType::Gitlab => {
            let target =
                validate_gitlab_ingest_target(&required_ingest_target(req, "target project")?)?;
            Ok(IngestSource::Gitlab {
                target,
                include_source: req.include_source.unwrap_or(cfg.github_include_source),
            })
        }
        IngestSourceType::Gitea => {
            let target =
                validate_gitea_ingest_target(&required_ingest_target(req, "target repo")?)?;
            Ok(IngestSource::Gitea {
                target,
                include_source: req.include_source.unwrap_or(cfg.github_include_source),
            })
        }
        IngestSourceType::Git => {
            let target = validate_git_ingest_target(&required_ingest_target(req, "target repo")?)?;
            Ok(IngestSource::GenericGit {
                target,
                include_source: req.include_source.unwrap_or(cfg.github_include_source),
            })
        }
        IngestSourceType::Reddit => {
            let target = required_ingest_target(req, "target")?;
            validate_reddit_ingest_target(&target)?;
            Ok(IngestSource::Reddit { target })
        }
        IngestSourceType::Youtube => {
            let target = required_ingest_target(req, "target")?;
            validate_youtube_ingest_target(&target)?;
            Ok(IngestSource::Youtube { target })
        }
        IngestSourceType::Rss => {
            let target = required_ingest_target(req, "target")?;
            validate_rss_ingest_target(&target)?;
            Ok(IngestSource::Rss { target })
        }
        IngestSourceType::Sessions => Err(
            "remote sessions ingest must use /v1/ingest/sessions/prepared; server-local session scanning is disabled"
                .to_string(),
        ),
    }
}

pub fn validate_ingest_source(source: &IngestSource) -> Result<(), String> {
    match source {
        IngestSource::Github { repo, .. } => {
            validate_github_ingest_target(repo)?;
        }
        IngestSource::Gitlab { target, .. } => {
            validate_gitlab_ingest_target(target)?;
        }
        IngestSource::Gitea { target, .. } => {
            validate_gitea_ingest_target(target)?;
        }
        IngestSource::GenericGit { target, .. } => {
            validate_git_ingest_target(target)?;
        }
        IngestSource::Reddit { target } => {
            validate_reddit_ingest_target(target)?;
        }
        IngestSource::Youtube { target } => {
            validate_youtube_ingest_target(target)?;
        }
        IngestSource::Rss { target } => {
            validate_rss_ingest_target(target)?;
        }
        IngestSource::Sessions { .. } => {}
        IngestSource::PreparedSessions { .. } => {}
    }
    Ok(())
}

fn validate_github_ingest_target(target: &str) -> Result<String, String> {
    let (owner, repo) = ingest::target_parse::parse_github_repo(target).ok_or_else(|| {
        "invalid GitHub target; expected owner/repo or github.com/owner/repo".to_string()
    })?;
    Ok(format!("{owner}/{repo}"))
}

fn validate_gitlab_ingest_target(target: &str) -> Result<String, String> {
    ingest::target_parse::normalize_gitlab_target(target).map_err(|err| {
        format!(
            "invalid GitLab target; expected gitlab.com/group/project URL or gitlab:<host>/<group>/<project>: {err}"
        )
    })
}

fn validate_gitea_ingest_target(target: &str) -> Result<String, String> {
    ingest::target_parse::normalize_gitea_target(target).map_err(|err| {
        format!("invalid Gitea target; expected gitea:<host>/<owner>/<repo> or known Gitea/Forgejo URL: {err}")
    })
}

fn validate_git_ingest_target(target: &str) -> Result<String, String> {
    ingest::target_parse::normalize_generic_git_target(target).map_err(|err| {
        format!("invalid generic git target; expected git:https://host/path/repo.git: {err}")
    })
}

fn validate_reddit_ingest_target(target: &str) -> Result<(), String> {
    match ingest::target_parse::classify_reddit_target(target).map_err(|err| err.to_string())? {
        ingest::target_parse::RedditTarget::Subreddit(name) => {
            let len = name.len();
            let valid = (3..=21).contains(&len)
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
            if valid {
                Ok(())
            } else {
                Err(
                    "invalid subreddit target; expected 3-21 ASCII letters, digits, or '_'"
                        .to_string(),
                )
            }
        }
        ingest::target_parse::RedditTarget::Thread(url) => {
            if url.starts_with("/r/") && url.contains("/comments/") {
                Ok(())
            } else {
                Err(
                    "invalid Reddit thread target; expected reddit.com comments URL or canonical /r/... permalink"
                        .to_string(),
                )
            }
        }
    }
}

fn validate_youtube_ingest_target(target: &str) -> Result<(), String> {
    ingest::target_parse::classify_youtube_target(target)
        .map(|_| ())
        .map_err(|err| format!("invalid YouTube target: {err}"))
}

fn validate_rss_ingest_target(target: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(target)
        .map_err(|err| format!("invalid RSS feed target; expected a feed URL: {err}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("invalid RSS feed target; only http/https feed URLs are supported".to_string());
    }
    Ok(())
}

fn required_ingest_target(
    req: &axon_api::mcp_schema::IngestRequest,
    field: &'static str,
) -> Result<String, String> {
    let Some(value) = req
        .target
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return Err(format!("{field} is required"));
    };
    Ok(value.to_string())
}
