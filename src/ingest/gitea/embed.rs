use anyhow::{Result, anyhow};
use reqwest::Client;

use crate::core::config::Config;
use crate::ingest::git_payload::{ContentKind, GitPayload, build_git_payload};
use crate::vector::ops::{PreparedDoc, chunk_text, embed_prepared_docs};

use super::GiteaTarget;
use super::client::{
    GiteaIssue, GiteaLabel, GiteaPullRequest, GiteaRepo, GiteaUser, fetch_paginated,
};

pub(crate) async fn embed_docs(cfg: &Config, docs: Vec<PreparedDoc>) -> Result<usize> {
    let summary = embed_prepared_docs(cfg, docs, None)
        .await
        .map_err(|e| anyhow!("{}", e))?;
    Ok(summary.chunks_embedded)
}

pub(crate) fn payload(
    target: &GiteaTarget,
    repo: &GiteaRepo,
    kind: &'static str,
    kind_extra: serde_json::Value,
) -> serde_json::Value {
    let (state, number, author, labels, is_draft, merged_at, created_at, updated_at) = match kind {
        "issue" | "pull_request" => (
            kind_extra
                .get("state")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            kind_extra.get("number").and_then(|v| v.as_u64()),
            kind_extra
                .get("author")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            kind_extra
                .get("labels")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(str::to_string))
                        .collect()
                }),
            None::<bool>,
            None::<String>,
            kind_extra
                .get("created_at")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            kind_extra
                .get("updated_at")
                .and_then(|v| v.as_str())
                .map(str::to_string),
        ),
        _ => (None, None, None, None, None, None, None, None),
    };

    build_git_payload(&GitPayload {
        provider: "gitea".to_string(),
        host: target.host.clone(),
        owner: Some(target.owner.clone()),
        repo: target.repo.clone(),
        content_kind: ContentKind::from_wire(kind),
        branch: repo.default_branch.clone(),
        state,
        number,
        author,
        labels: labels.unwrap_or_default(),
        is_draft,
        merged_at,
        created_at,
        updated_at,
        file_path: None,
        file_language: None,
        meta: Some(serde_json::json!({
            "default_branch": repo.default_branch,
            "private": repo.private,
            "gitea": kind_extra,
        })),
        ..GitPayload::default()
    })
}

pub(crate) async fn embed_metadata(
    cfg: &Config,
    target: &GiteaTarget,
    repo: &GiteaRepo,
) -> Result<usize> {
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

pub(crate) fn author_name(user: &Option<GiteaUser>) -> Option<String> {
    user.as_ref()
        .and_then(|u| u.login.clone().or_else(|| u.full_name.clone()))
}

pub(crate) fn label_names(labels: Option<Vec<GiteaLabel>>) -> Vec<String> {
    labels
        .unwrap_or_default()
        .into_iter()
        .map(|l| l.name)
        .collect()
}

pub(crate) async fn embed_issues(
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

pub(crate) fn issue_doc(
    target: &GiteaTarget,
    repo: &GiteaRepo,
    issue: GiteaIssue,
) -> Option<PreparedDoc> {
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

pub(crate) async fn embed_pulls(
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

pub(crate) fn pull_doc(
    target: &GiteaTarget,
    repo: &GiteaRepo,
    pull: GiteaPullRequest,
) -> Option<PreparedDoc> {
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
