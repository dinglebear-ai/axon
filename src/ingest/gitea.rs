use anyhow::{Result, anyhow, bail};
use reqwest::Url;

use crate::core::config::Config;
use crate::core::http::validate_url;
use crate::core::logging::{log_done, log_info, log_warn};
use crate::ingest::generic_git::ingest_git_repository;
use crate::ingest::progress::PhaseReporter;

mod client;
mod embed;

use client::{build_client, fetch_repo};
use embed::{embed_issues, embed_metadata, embed_pulls};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiteaTarget {
    pub host: String,
    pub owner: String,
    pub repo: String,
    pub web_url: String,
    pub api_base: String,
}

impl GiteaTarget {
    pub(crate) fn as_normalized_target(&self) -> String {
        format!("{}/{}/{}", self.host, self.owner, self.repo)
    }

    pub(crate) fn repo_api_url(&self, suffix: &str) -> String {
        format!(
            "{}/repos/{}/{}{}",
            self.api_base, self.owner, self.repo, suffix
        )
    }
}

pub fn normalize_gitea_target(input: &str) -> Result<String> {
    Ok(parse_gitea_target(input)?.as_normalized_target())
}

pub fn parse_gitea_target(input: &str) -> Result<GiteaTarget> {
    let raw = input
        .trim()
        .strip_prefix("gitea:")
        .or_else(|| input.trim().strip_prefix("forgejo:"))
        .unwrap_or(input.trim());
    let parsed = if raw.starts_with("http://") || raw.starts_with("https://") {
        Url::parse(raw)?
    } else {
        Url::parse(&format!("https://{raw}"))?
    };
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        bail!("invalid Gitea target '{input}': expected http(s) URL");
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("invalid Gitea target '{input}': missing host"))?
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    let segments: Vec<&str> = parsed
        .path()
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    if segments.len() < 2 {
        bail!("invalid Gitea target '{input}': expected host/owner/repo");
    }
    let owner = segments[0].to_string();
    let repo = segments[1].trim_end_matches(".git").to_string();
    let web_url = format!("{}://{host}/{owner}/{repo}", parsed.scheme());
    validate_url(&web_url)?;
    Ok(GiteaTarget {
        host: host.clone(),
        owner,
        repo,
        web_url,
        api_base: format!("{}://{host}/api/v1", parsed.scheme()),
    })
}

pub async fn ingest_gitea(
    cfg: &Config,
    target: &str,
    include_source: bool,
    reporter: PhaseReporter,
) -> Result<usize> {
    log_info(&format!("command=ingest source=gitea target={target}"));
    let target = parse_gitea_target(target)?;
    let client = build_client(cfg)?;
    let repo = fetch_repo(&client, &target).await?;
    let mut total = embed_metadata(cfg, &target, &repo)
        .await
        .unwrap_or_else(|err| {
            log_warn(&format!(
                "gitea metadata_failed target={} err={err}",
                target.web_url
            ));
            0
        });
    let clone_target = repo.clone_url.as_deref().unwrap_or(target.web_url.as_str());
    total += ingest_git_repository(
        cfg,
        &format!("git:{clone_target}"),
        include_source,
        reporter,
        "gitea",
        "gitea",
    )
    .await
    .unwrap_or_else(|err| {
        log_warn(&format!(
            "gitea files_failed target={} err={err}",
            target.web_url
        ));
        0
    });
    total += embed_issues(cfg, &client, &target, &repo)
        .await
        .unwrap_or_else(|err| {
            log_warn(&format!(
                "gitea issues_failed target={} err={err}",
                target.web_url
            ));
            0
        });
    total += embed_pulls(cfg, &client, &target, &repo)
        .await
        .unwrap_or_else(|err| {
            log_warn(&format!(
                "gitea pulls_failed target={} err={err}",
                target.web_url
            ));
            0
        });
    log_done(&format!(
        "command=ingest source=gitea target={} chunk_count={total}",
        target.web_url
    ));
    Ok(total)
}

#[cfg(test)]
#[path = "gitea_tests.rs"]
mod tests;
