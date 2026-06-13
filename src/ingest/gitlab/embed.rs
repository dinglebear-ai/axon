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
use crate::vector::ops::{PreparedDoc, prepare_plain_text_source};

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
    kind: ContentKind,
    kind_extra: serde_json::Value,
) -> serde_json::Value {
    let owner: Option<String> = {
        let path = &target.namespace_path;
        path.rfind('/').map(|i| path[..i].to_string())
    };
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
    let extra = gitlab_payload(
        target,
        project,
        ContentKind::RepoMetadata,
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
    let doc = prepare_plain_text_source(
        project.web_url.clone(),
        target.host.clone(),
        content,
        "gitlab",
        Some(project.path_with_namespace.clone()),
        Some(extra),
    )
    .map_err(|err| anyhow::anyhow!("prepare gitlab metadata source failed: {err}"))?;
    if doc.chunks.is_empty() {
        return Ok(0);
    }
    embed_docs(cfg, vec![doc]).await
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
        .map(|issue| issue_doc(target, project, issue))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    embed_docs(cfg, docs).await
}

fn issue_doc(
    target: &GitLabTarget,
    project: &GitLabProject,
    issue: GitLabIssue,
) -> Result<Option<PreparedDoc>> {
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
    let url = issue
        .web_url
        .clone()
        .unwrap_or_else(|| format!("{}/-/issues/{}", target.web_url, issue.iid));
    let extra = gitlab_payload(
        target,
        project,
        ContentKind::Issue,
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
    let doc = prepare_plain_text_source(
        url,
        target.host.clone(),
        content,
        "gitlab",
        Some(format!("Issue #{}: {}", issue.iid, issue.title)),
        Some(extra),
    )
    .map_err(|err| anyhow::anyhow!("prepare gitlab issue source failed: {err}"))?;
    Ok((!doc.chunks.is_empty()).then_some(doc))
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
        .map(|mr| merge_request_doc(target, project, mr))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    embed_docs(cfg, docs).await
}

fn merge_request_doc(
    target: &GitLabTarget,
    project: &GitLabProject,
    mr: GitLabMergeRequest,
) -> Result<Option<PreparedDoc>> {
    let body = mr.description.as_deref().unwrap_or("");
    let content = format!("# MR !{}: {}\n\n{}", mr.iid, mr.title, body);
    let url = mr
        .web_url
        .clone()
        .unwrap_or_else(|| format!("{}/-/merge_requests/{}", target.web_url, mr.iid));
    let labels = mr.labels.unwrap_or_default();
    let extra = gitlab_payload(
        target,
        project,
        ContentKind::Pr,
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
    let doc = prepare_plain_text_source(
        url,
        target.host.clone(),
        content,
        "gitlab",
        Some(format!("MR !{}: {}", mr.iid, mr.title)),
        Some(extra),
    )
    .map_err(|err| anyhow::anyhow!("prepare gitlab merge request source failed: {err}"))?;
    Ok((!doc.chunks.is_empty()).then_some(doc))
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
        .map(|page| wiki_doc(target, project, page))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
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
) -> Result<Option<PreparedDoc>> {
    let Some(content) = page.content else {
        return Ok(None);
    };
    let extra = gitlab_payload(
        target,
        project,
        ContentKind::Wiki,
        serde_json::json!({
            "slug": page.slug,
            "format": page.format,
            "encoding": page.encoding,
        }),
    );
    let doc = prepare_plain_text_source(
        format!("{}/-/wikis/{}", target.web_url, page.slug),
        target.host.clone(),
        format!("# {}\n\n{}", page.title, content),
        "gitlab",
        Some(format!("Wiki: {}", page.title)),
        Some(extra),
    )
    .map_err(|err| anyhow::anyhow!("prepare gitlab wiki source failed: {err}"))?;
    Ok((!doc.chunks.is_empty()).then_some(doc))
}
