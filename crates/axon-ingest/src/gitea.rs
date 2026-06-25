use anyhow::{Result, anyhow, bail};
use reqwest::Url;

use crate::generic_git::ingest_git_repository;
use crate::progress::PhaseReporter;
use axon_core::config::Config;
use axon_core::http::validate_url;
use axon_core::logging::{log_done, log_info, log_warn};

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

    reporter
        .report(serde_json::json!({
            "phase": "ingesting",
            "tasks_total": 4,
            "tasks_done": 0,
        }))
        .await;

    let mut total = 0usize;
    let mut tasks_done = 0usize;
    const TASKS_TOTAL: usize = 4;

    macro_rules! run_phase {
        ($label:expr, $fut:expr) => {{
            reporter
                .report(serde_json::json!({
                    "phase": $label,
                    "tasks_done": tasks_done,
                    "tasks_total": TASKS_TOTAL,
                }))
                .await;
            match $fut.await {
                Ok(chunks) => {
                    total += chunks;
                    log_info(&format!(
                        "gitea task_done task={} target={} chunks={chunks}",
                        $label,
                        target.web_url
                    ));
                }
                Err(err) => log_warn(&format!(
                    "gitea task_failed task={} target={} err={err}",
                    $label,
                    target.web_url
                )),
            }
            tasks_done += 1;
        }};
    }

    run_phase!("metadata", embed_metadata(cfg, &target, &repo));

    let clone_target = repo.clone_url.as_deref().unwrap_or(target.web_url.as_str());
    let clone_url = format!("git:{clone_target}");
    run_phase!(
        "files",
        ingest_git_repository(
            cfg,
            &clone_url,
            include_source,
            reporter.clone(),
            "gitea",
            "gitea"
        )
    );

    run_phase!("issues", embed_issues(cfg, &client, &target, &repo));
    run_phase!("pulls", embed_pulls(cfg, &client, &target, &repo));

    reporter
        .report(serde_json::json!({
            "tasks_done": tasks_done,
            "tasks_total": TASKS_TOTAL,
            "chunks_embedded": total,
            "phase": "completed",
        }))
        .await;
    log_done(&format!(
        "command=ingest source=gitea target={} chunk_count={total}",
        target.web_url
    ));
    Ok(total)
}

#[cfg(test)]
#[path = "gitea_tests.rs"]
mod tests;
