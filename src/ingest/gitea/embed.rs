use anyhow::Result;
use reqwest::Client;

use crate::core::config::Config;
use crate::ingest::git_payload::{
    ContentKind, GitPayload, build_git_payload, extract_git_item_fields,
};
use crate::vector::ops::{PreparedDoc, prepare_plain_text_source};

use super::GiteaTarget;
use super::client::{
    GiteaIssue, GiteaLabel, GiteaPullRequest, GiteaRepo, GiteaUser, fetch_paginated,
};
use crate::ingest::git_files::embed_docs;

pub(crate) fn payload(
    target: &GiteaTarget,
    repo: &GiteaRepo,
    kind: ContentKind,
    kind_extra: serde_json::Value,
) -> serde_json::Value {
    // Gitea uses "number" as the item number field (Q-H4: shared decoder)
    let (state, number, author, labels, is_draft, merged_at, created_at, updated_at) =
        if matches!(kind, ContentKind::Issue | ContentKind::Pr) {
            extract_git_item_fields(&kind_extra, "number")
        } else {
            (None, None, None, None, None, None, None, None)
        };

    build_git_payload(&GitPayload {
        provider: "gitea".to_string(),
        host: target.host.clone(),
        owner: Some(target.owner.clone()),
        repo: target.repo.clone(),
        content_kind: kind,
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
    let doc = prepare_plain_text_source(
        repo.html_url
            .clone()
            .unwrap_or_else(|| target.web_url.clone()),
        target.host.clone(),
        format!("# {title}\n\n{}", parts.join("\n")),
        "gitea",
        Some(title),
        Some(payload(
            target,
            repo,
            ContentKind::RepoMetadata,
            serde_json::json!({
                "private": repo.private,
                "stars": repo.stars_count,
                "forks": repo.forks_count,
                "open_issues": repo.open_issues_count,
            }),
        )),
    );
    if doc.is_empty() {
        return Ok(0);
    }
    embed_docs(cfg, vec![doc]).await
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
        .map(|i| issue_doc(target, repo, i))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    embed_docs(cfg, docs).await
}

pub(crate) fn issue_doc(
    target: &GiteaTarget,
    repo: &GiteaRepo,
    issue: GiteaIssue,
) -> Result<Option<PreparedDoc>> {
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
    let url = issue
        .html_url
        .unwrap_or_else(|| format!("{}/issues/{}", target.web_url, issue.number));
    let doc = prepare_plain_text_source(
        url,
        target.host.clone(),
        content,
        "gitea",
        Some(format!("Issue #{}: {}", issue.number, issue.title)),
        Some(payload(
            target,
            repo,
            ContentKind::Issue,
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
    );
    Ok((!doc.is_empty()).then_some(doc))
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
        .map(|p| pull_doc(target, repo, p))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    embed_docs(cfg, docs).await
}

pub(crate) fn pull_doc(
    target: &GiteaTarget,
    repo: &GiteaRepo,
    pull: GiteaPullRequest,
) -> Result<Option<PreparedDoc>> {
    let labels = label_names(pull.labels);
    let content = format!(
        "# PR #{}: {}\n\n{}",
        pull.number,
        pull.title,
        pull.body.as_deref().unwrap_or("")
    );
    let url = pull
        .html_url
        .unwrap_or_else(|| format!("{}/pulls/{}", target.web_url, pull.number));
    let doc = prepare_plain_text_source(
        url,
        target.host.clone(),
        content,
        "gitea",
        Some(format!("PR #{}: {}", pull.number, pull.title)),
        Some(payload(
            target,
            repo,
            ContentKind::Pr,
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
    );
    Ok((!doc.is_empty()).then_some(doc))
}
