use anyhow::{Result, anyhow};
use reqwest::Client;
use reqwest::StatusCode;

use crate::core::config::Config;
use crate::vector::ops::{PreparedDoc, chunk_text, embed_prepared_docs};

use super::client::fetch_paginated;
use super::types::{
    GitLabIssue, GitLabMergeRequest, GitLabProject, GitLabTarget, GitLabUser, GitLabWikiPage,
};

pub(crate) fn gitlab_payload(
    target: &GitLabTarget,
    project: &GitLabProject,
    content_kind: &str,
    extra: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "provider": "gitlab",
        "host": target.host,
        "namespace_path": target.namespace_path,
        "project": target.project,
        "content_kind": content_kind,
        "default_branch": project.default_branch,
        "visibility": project.visibility,
        "last_activity_at": project.last_activity_at,
        "gitlab": extra,
    })
}

pub(crate) async fn embed_docs(cfg: &Config, docs: Vec<PreparedDoc>) -> Result<usize> {
    let summary = embed_prepared_docs(cfg, docs, None)
        .await
        .map_err(|e| anyhow!("{e}"))?;
    Ok(summary.chunks_embedded)
}

pub(crate) async fn embed_metadata(
    cfg: &Config,
    target: &GitLabTarget,
    project: &GitLabProject,
) -> Result<usize> {
    let mut parts = Vec::new();
    if let Some(desc) = project
        .description
        .as_deref()
        .filter(|desc| !desc.is_empty())
    {
        parts.push(format!("Description: {desc}"));
    }
    if let Some(visibility) = &project.visibility {
        parts.push(format!("Visibility: {visibility}"));
    }
    if let Some(stars) = project.star_count {
        parts.push(format!("Stars: {stars}"));
    }
    if let Some(forks) = project.forks_count {
        parts.push(format!("Forks: {forks}"));
    }
    if parts.is_empty() {
        return Ok(0);
    }
    let content = format!("# {}\n\n{}", project.path_with_namespace, parts.join("\n"));
    let chunks = chunk_text(&content);
    if chunks.is_empty() {
        return Ok(0);
    }
    let extra = gitlab_payload(
        target,
        project,
        "repo_metadata",
        serde_json::json!({
            "name": project.name,
            "stars": project.star_count,
            "forks": project.forks_count,
            "open_issues": project.open_issues_count,
            "issues_enabled": project.issues_enabled,
            "merge_requests_enabled": project.merge_requests_enabled,
            "wiki_enabled": project.wiki_enabled,
        }),
    );
    embed_docs(
        cfg,
        vec![PreparedDoc {
            url: project.web_url.clone(),
            domain: target.host.clone(),
            chunks,
            source_type: "gitlab".to_string(),
            content_type: "text",
            title: Some(project.path_with_namespace.clone()),
            extra: Some(extra),
            extractor_name: None,
            structured: None,
        }],
    )
    .await
}

fn author_name(author: &Option<GitLabUser>) -> Option<String> {
    author
        .as_ref()
        .and_then(|user| user.username.clone().or_else(|| user.name.clone()))
}

pub(crate) async fn embed_issues(
    cfg: &Config,
    client: &Client,
    target: &GitLabTarget,
    project: &GitLabProject,
) -> Result<usize> {
    if project.issues_enabled == Some(false) {
        return Ok(0);
    }
    let issues: Vec<GitLabIssue> = match fetch_paginated(
        client,
        &target.project_api_url("/issues"),
        &[
            ("state", "all"),
            ("order_by", "updated_at"),
            ("sort", "desc"),
        ],
        cfg.github_max_issues,
    )
    .await
    {
        Ok(issues) => issues,
        Err(err) if is_missing_or_forbidden(&err) => return Ok(0),
        Err(err) => return Err(err),
    };
    let docs = issues
        .into_iter()
        .filter_map(|issue| issue_doc(target, project, issue))
        .collect();
    embed_docs(cfg, docs).await
}

fn issue_doc(
    target: &GitLabTarget,
    project: &GitLabProject,
    issue: GitLabIssue,
) -> Option<PreparedDoc> {
    let body = issue.description.as_deref().unwrap_or("");
    let labels = issue.labels.unwrap_or_default();
    let label_text = if labels.is_empty() {
        String::new()
    } else {
        format!("\nLabels: {}", labels.join(", "))
    };
    let content = format!(
        "# Issue #{}: {}\n\n{}{}",
        issue.iid, issue.title, body, label_text
    );
    let chunks = chunk_text(&content);
    if chunks.is_empty() {
        return None;
    }
    let url = issue
        .web_url
        .clone()
        .unwrap_or_else(|| format!("{}/-/issues/{}", target.web_url, issue.iid));
    let extra = gitlab_payload(
        target,
        project,
        "issue",
        serde_json::json!({
            "iid": issue.iid,
            "state": issue.state,
            "author": author_name(&issue.author),
            "labels": labels,
            "created_at": issue.created_at,
            "updated_at": issue.updated_at,
            "comment_count": issue.user_notes_count,
        }),
    );
    Some(PreparedDoc {
        url,
        domain: target.host.clone(),
        chunks,
        source_type: "gitlab".to_string(),
        content_type: "text",
        title: Some(format!("Issue #{}: {}", issue.iid, issue.title)),
        extra: Some(extra),
        extractor_name: None,
        structured: None,
    })
}

pub(crate) async fn embed_merge_requests(
    cfg: &Config,
    client: &Client,
    target: &GitLabTarget,
    project: &GitLabProject,
) -> Result<usize> {
    if project.merge_requests_enabled == Some(false) {
        return Ok(0);
    }
    let mrs: Vec<GitLabMergeRequest> = match fetch_paginated(
        client,
        &target.project_api_url("/merge_requests"),
        &[
            ("state", "all"),
            ("order_by", "updated_at"),
            ("sort", "desc"),
        ],
        cfg.github_max_prs,
    )
    .await
    {
        Ok(mrs) => mrs,
        Err(err) if is_missing_or_forbidden(&err) => return Ok(0),
        Err(err) => return Err(err),
    };
    let docs = mrs
        .into_iter()
        .filter_map(|mr| merge_request_doc(target, project, mr))
        .collect();
    embed_docs(cfg, docs).await
}

fn merge_request_doc(
    target: &GitLabTarget,
    project: &GitLabProject,
    mr: GitLabMergeRequest,
) -> Option<PreparedDoc> {
    let body = mr.description.as_deref().unwrap_or("");
    let content = format!("# MR !{}: {}\n\n{}", mr.iid, mr.title, body);
    let chunks = chunk_text(&content);
    if chunks.is_empty() {
        return None;
    }
    let url = mr
        .web_url
        .clone()
        .unwrap_or_else(|| format!("{}/-/merge_requests/{}", target.web_url, mr.iid));
    let labels = mr.labels.unwrap_or_default();
    let extra = gitlab_payload(
        target,
        project,
        "merge_request",
        serde_json::json!({
            "iid": mr.iid,
            "state": mr.state,
            "author": author_name(&mr.author),
            "labels": labels,
            "created_at": mr.created_at,
            "updated_at": mr.updated_at,
            "comment_count": mr.user_notes_count,
            "merged_at": mr.merged_at,
            "is_draft": mr.draft,
        }),
    );
    Some(PreparedDoc {
        url,
        domain: target.host.clone(),
        chunks,
        source_type: "gitlab".to_string(),
        content_type: "text",
        title: Some(format!("MR !{}: {}", mr.iid, mr.title)),
        extra: Some(extra),
        extractor_name: None,
        structured: None,
    })
}

pub(crate) async fn embed_wiki(
    cfg: &Config,
    client: &Client,
    target: &GitLabTarget,
    project: &GitLabProject,
) -> Result<usize> {
    if project.wiki_enabled == Some(false) {
        return Ok(0);
    }
    let pages: Vec<GitLabWikiPage> = match fetch_paginated(
        client,
        &target.project_api_url("/wikis"),
        &[("with_content", "1")],
        0,
    )
    .await
    {
        Ok(pages) => pages,
        Err(err) if is_missing_or_forbidden(&err) => return Ok(0),
        Err(err) => return Err(err),
    };
    let docs = pages
        .into_iter()
        .filter_map(|page| wiki_doc(target, project, page))
        .collect();
    embed_docs(cfg, docs).await
}

fn is_missing_or_forbidden(err: &anyhow::Error) -> bool {
    err.downcast_ref::<reqwest::Error>()
        .and_then(reqwest::Error::status)
        .is_some_and(|status| matches!(status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND))
}

fn wiki_doc(
    target: &GitLabTarget,
    project: &GitLabProject,
    page: GitLabWikiPage,
) -> Option<PreparedDoc> {
    let content = page.content?;
    let chunks = chunk_text(&format!("# {}\n\n{}", page.title, content));
    if chunks.is_empty() {
        return None;
    }
    let extra = gitlab_payload(
        target,
        project,
        "wiki",
        serde_json::json!({
            "slug": page.slug,
            "format": page.format,
            "encoding": page.encoding,
        }),
    );
    Some(PreparedDoc {
        url: format!("{}/-/wikis/{}", target.web_url, page.slug),
        domain: target.host.clone(),
        chunks,
        source_type: "gitlab".to_string(),
        content_type: "text",
        title: Some(format!("Wiki: {}", page.title)),
        extra: Some(extra),
        extractor_name: None,
        structured: None,
    })
}
