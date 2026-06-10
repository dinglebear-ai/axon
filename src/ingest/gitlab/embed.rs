use anyhow::Result;
use reqwest::Client;
use reqwest::StatusCode;

use crate::core::config::Config;
use crate::ingest::git_payload::{
    ContentKind, GitPayload, build_git_payload, extract_git_item_fields,
};
use crate::vector::ops::input::classify::{
    classify_file_type, is_test_path, language_name, path_extension,
};
use crate::vector::ops::{PreparedDoc, chunk_text};

use super::client::fetch_paginated;
use super::types::{
    GitLabIssue, GitLabMergeRequest, GitLabProject, GitLabTarget, GitLabUser, GitLabWikiPage,
};
use crate::ingest::git_files::embed_docs;

/// Build the canonical `git_*` payload for a GitLab chunk, plus GitLab-specific fields.
///
/// ## Owner convention for GitLab
/// `git_owner` = `namespace_path` minus the final project segment
/// (e.g. `"group/subgroup"` for `group/subgroup/project`).
/// `git_repo` = `target.project` (the final segment only).
pub(crate) fn gitlab_payload(
    target: &GitLabTarget,
    project: &GitLabProject,
    content_kind: &'static str,
    kind_extra: serde_json::Value,
) -> serde_json::Value {
    let owner: Option<String> = {
        let path = &target.namespace_path;
        path.rfind('/').map(|i| path[..i].to_string())
    };
    // Canonical content_kind: GitLab uses "merge_request"; from_wire normalises.
    let kind = ContentKind::from_wire(content_kind);

    // GitLab uses "iid" as the item number field (Q-H4: shared decoder)
    let (state, number, author, labels, is_draft, merged_at, created_at, updated_at) =
        extract_git_item_fields(&kind_extra, "iid");

    let file_path = kind_extra
        .get("path")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let (file_language, file_type, file_is_test) = if kind == ContentKind::File {
        file_path.as_deref().map_or((None, None, None), |path| {
            (
                Some(language_name(path_extension(path)).to_string()),
                Some(classify_file_type(path).to_string()),
                Some(is_test_path(path)),
            )
        })
    } else {
        (None, None, None)
    };
    let branch = kind_extra
        .get("branch")
        .and_then(|v| v.as_str())
        .or(project.default_branch.as_deref())
        .map(str::to_string);

    build_git_payload(&GitPayload {
        provider: "gitlab".to_string(),
        host: target.host.clone(),
        owner,
        repo: target.project.clone(),
        content_kind: kind,
        branch,
        state,
        number,
        author,
        labels: labels.unwrap_or_default(),
        is_draft,
        merged_at,
        created_at,
        updated_at,
        file_path,
        file_language,
        file_type,
        file_is_test,
        meta: Some(serde_json::json!({
            "namespace_path": target.namespace_path,
            "visibility": project.visibility,
            "last_activity_at": project.last_activity_at,
            "default_branch": project.default_branch,
            "gitlab": kind_extra,
        })),
        ..GitPayload::default()
    })
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
        vec![PreparedDoc::ingest(
            project.web_url.clone(),
            target.host.clone(),
            chunks,
            "gitlab",
            Some(project.path_with_namespace.clone()),
            Some(extra),
        )],
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
    Some(PreparedDoc::ingest(
        url,
        target.host.clone(),
        chunks,
        "gitlab",
        Some(format!("Issue #{}: {}", issue.iid, issue.title)),
        Some(extra),
    ))
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
    Some(PreparedDoc::ingest(
        url,
        target.host.clone(),
        chunks,
        "gitlab",
        Some(format!("MR !{}: {}", mr.iid, mr.title)),
        Some(extra),
    ))
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
    Some(PreparedDoc::ingest(
        format!("{}/-/wikis/{}", target.web_url, page.slug),
        target.host.clone(),
        chunks,
        "gitlab",
        Some(format!("Wiki: {}", page.title)),
        Some(extra),
    ))
}
