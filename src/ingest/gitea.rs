use anyhow::{Result, anyhow, bail};
use reqwest::header::{HeaderMap, HeaderValue};
use reqwest::{Client, Url};
use serde::Deserialize;

use crate::core::config::Config;
use crate::core::http::validate_url;
use crate::core::logging::{log_done, log_info, log_warn};
use crate::ingest::generic_git::ingest_git_repository;
use crate::ingest::progress::PhaseReporter;
use crate::vector::ops::{PreparedDoc, chunk_text, embed_prepared_docs};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiteaTarget {
    pub host: String,
    pub owner: String,
    pub repo: String,
    pub web_url: String,
    pub api_base: String,
}

#[derive(Debug, Deserialize)]
struct GiteaRepo {
    full_name: Option<String>,
    description: Option<String>,
    html_url: Option<String>,
    clone_url: Option<String>,
    default_branch: Option<String>,
    private: Option<bool>,
    stars_count: Option<u64>,
    forks_count: Option<u64>,
    open_issues_count: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct GiteaUser {
    login: Option<String>,
    full_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GiteaIssue {
    number: u64,
    title: String,
    body: Option<String>,
    state: Option<String>,
    html_url: Option<String>,
    user: Option<GiteaUser>,
    labels: Option<Vec<GiteaLabel>>,
    created_at: Option<String>,
    updated_at: Option<String>,
    comments: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct GiteaLabel {
    name: String,
}

#[derive(Debug, Deserialize)]
struct GiteaPullRequest {
    number: u64,
    title: String,
    body: Option<String>,
    state: Option<String>,
    html_url: Option<String>,
    user: Option<GiteaUser>,
    labels: Option<Vec<GiteaLabel>>,
    created_at: Option<String>,
    updated_at: Option<String>,
    comments: Option<u64>,
    merged: Option<bool>,
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

impl GiteaTarget {
    fn as_normalized_target(&self) -> String {
        format!("{}/{}/{}", self.host, self.owner, self.repo)
    }

    fn repo_api_url(&self, suffix: &str) -> String {
        format!(
            "{}/repos/{}/{}{}",
            self.api_base, self.owner, self.repo, suffix
        )
    }
}

fn build_client(cfg: &Config) -> Result<Client> {
    let mut headers = HeaderMap::new();
    if let Some(token) = cfg.gitea_token.as_deref().filter(|token| !token.is_empty()) {
        headers.insert(
            "Authorization",
            HeaderValue::from_str(&format!("token {token}"))?,
        );
    }
    Ok(Client::builder()
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(60))
        .build()?)
}

async fn fetch_repo(client: &Client, target: &GiteaTarget) -> Result<GiteaRepo> {
    Ok(client
        .get(target.repo_api_url(""))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

async fn fetch_paginated<T: for<'de> Deserialize<'de>>(
    client: &Client,
    url: &str,
    query: &[(&str, &str)],
    max_items: usize,
) -> Result<Vec<T>> {
    let mut out = Vec::new();
    let mut page = 1usize;
    loop {
        let mut request_url = Url::parse(url)?;
        {
            let mut pairs = request_url.query_pairs_mut();
            for (key, value) in query {
                pairs.append_pair(key, value);
            }
            pairs.append_pair("limit", "100");
            pairs.append_pair("page", &page.to_string());
        }
        let mut page_items: Vec<T> = client
            .get(request_url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let page_len = page_items.len();
        if max_items > 0 {
            let remaining = max_items.saturating_sub(out.len());
            page_items.truncate(remaining);
        }
        out.append(&mut page_items);
        if page_len < 100 || (max_items > 0 && out.len() >= max_items) {
            break;
        }
        page += 1;
    }
    Ok(out)
}

async fn embed_docs(cfg: &Config, docs: Vec<PreparedDoc>) -> Result<usize> {
    let summary = embed_prepared_docs(cfg, docs, None)
        .await
        .map_err(|e| anyhow!("{e}"))?;
    Ok(summary.chunks_embedded)
}

fn payload(
    target: &GiteaTarget,
    repo: &GiteaRepo,
    kind: &str,
    extra: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "provider": "gitea",
        "host": target.host,
        "owner": target.owner,
        "repo": target.repo,
        "content_kind": kind,
        "default_branch": repo.default_branch,
        "gitea": extra,
    })
}

async fn embed_metadata(cfg: &Config, target: &GiteaTarget, repo: &GiteaRepo) -> Result<usize> {
    let mut parts = Vec::new();
    if let Some(desc) = repo.description.as_deref().filter(|desc| !desc.is_empty()) {
        parts.push(format!("Description: {desc}"));
    }
    if let Some(stars) = repo.stars_count {
        parts.push(format!("Stars: {stars}"));
    }
    if let Some(forks) = repo.forks_count {
        parts.push(format!("Forks: {forks}"));
    }
    if parts.is_empty() {
        return Ok(0);
    }
    let title = repo
        .full_name
        .clone()
        .unwrap_or_else(|| format!("{}/{}", target.owner, target.repo));
    let chunks = chunk_text(&format!("# {title}\n\n{}", parts.join("\n")));
    if chunks.is_empty() {
        return Ok(0);
    }
    embed_docs(
        cfg,
        vec![PreparedDoc {
            url: repo
                .html_url
                .clone()
                .unwrap_or_else(|| target.web_url.clone()),
            domain: target.host.clone(),
            chunks,
            source_type: "gitea".to_string(),
            content_type: "text",
            title: Some(title),
            extra: Some(payload(
                target,
                repo,
                "repo_metadata",
                serde_json::json!({
                    "private": repo.private,
                    "stars": repo.stars_count,
                    "forks": repo.forks_count,
                    "open_issues": repo.open_issues_count,
                }),
            )),
            extractor_name: None,
            structured: None,
        }],
    )
    .await
}

fn author_name(user: &Option<GiteaUser>) -> Option<String> {
    user.as_ref()
        .and_then(|u| u.login.clone().or_else(|| u.full_name.clone()))
}

fn label_names(labels: Option<Vec<GiteaLabel>>) -> Vec<String> {
    labels
        .unwrap_or_default()
        .into_iter()
        .map(|l| l.name)
        .collect()
}

async fn embed_issues(
    cfg: &Config,
    client: &Client,
    target: &GiteaTarget,
    repo: &GiteaRepo,
) -> Result<usize> {
    let issues: Vec<GiteaIssue> = fetch_paginated(
        client,
        &target.repo_api_url("/issues"),
        &[("state", "all"), ("type", "issues")],
        cfg.github_max_issues,
    )
    .await?;
    let docs = issues
        .into_iter()
        .filter_map(|i| issue_doc(target, repo, i))
        .collect();
    embed_docs(cfg, docs).await
}

fn issue_doc(target: &GiteaTarget, repo: &GiteaRepo, issue: GiteaIssue) -> Option<PreparedDoc> {
    let labels = label_names(issue.labels);
    let label_text = if labels.is_empty() {
        String::new()
    } else {
        format!("\nLabels: {}", labels.join(", "))
    };
    let content = format!(
        "# Issue #{}: {}\n\n{}{}",
        issue.number,
        issue.title,
        issue.body.as_deref().unwrap_or(""),
        label_text
    );
    let chunks = chunk_text(&content);
    if chunks.is_empty() {
        return None;
    }
    Some(PreparedDoc {
        url: issue
            .html_url
            .unwrap_or_else(|| format!("{}/issues/{}", target.web_url, issue.number)),
        domain: target.host.clone(),
        chunks,
        source_type: "gitea".to_string(),
        content_type: "text",
        title: Some(format!("Issue #{}: {}", issue.number, issue.title)),
        extra: Some(payload(
            target,
            repo,
            "issue",
            serde_json::json!({
                "number": issue.number,
                "state": issue.state,
                "author": author_name(&issue.user),
                "labels": labels,
                "created_at": issue.created_at,
                "updated_at": issue.updated_at,
                "comment_count": issue.comments,
            }),
        )),
        extractor_name: None,
        structured: None,
    })
}

async fn embed_pulls(
    cfg: &Config,
    client: &Client,
    target: &GiteaTarget,
    repo: &GiteaRepo,
) -> Result<usize> {
    let pulls: Vec<GiteaPullRequest> = fetch_paginated(
        client,
        &target.repo_api_url("/pulls"),
        &[("state", "all")],
        cfg.github_max_prs,
    )
    .await?;
    let docs = pulls
        .into_iter()
        .filter_map(|p| pull_doc(target, repo, p))
        .collect();
    embed_docs(cfg, docs).await
}

fn pull_doc(target: &GiteaTarget, repo: &GiteaRepo, pull: GiteaPullRequest) -> Option<PreparedDoc> {
    let labels = label_names(pull.labels);
    let content = format!(
        "# PR #{}: {}\n\n{}",
        pull.number,
        pull.title,
        pull.body.as_deref().unwrap_or("")
    );
    let chunks = chunk_text(&content);
    if chunks.is_empty() {
        return None;
    }
    Some(PreparedDoc {
        url: pull
            .html_url
            .unwrap_or_else(|| format!("{}/pulls/{}", target.web_url, pull.number)),
        domain: target.host.clone(),
        chunks,
        source_type: "gitea".to_string(),
        content_type: "text",
        title: Some(format!("PR #{}: {}", pull.number, pull.title)),
        extra: Some(payload(
            target,
            repo,
            "pull_request",
            serde_json::json!({
                "number": pull.number,
                "state": pull.state,
                "author": author_name(&pull.user),
                "labels": labels,
                "created_at": pull.created_at,
                "updated_at": pull.updated_at,
                "comment_count": pull.comments,
                "merged": pull.merged,
            }),
        )),
        extractor_name: None,
        structured: None,
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
