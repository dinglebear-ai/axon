use octocrab::models;
use serde_json::{Value, json};

use crate::ingest::git_payload::{GitPayload, build_git_payload};

pub(crate) fn issue_state_str(state: &models::IssueState) -> &'static str {
    match state {
        models::IssueState::Open => "open",
        models::IssueState::Closed => "closed",
        _ => "unknown",
    }
}

// ── Unified payload builder ──────────────────────────────────────────────────

/// Parameters for the unified GitHub payload builder.
///
/// Required fields: `repo`, `owner`, `content_kind`.
/// All other fields are optional — absent values become JSON `null`.
#[derive(Debug, Default)]
pub struct GitHubPayloadParams {
    // Common (required)
    pub repo: String,
    pub owner: String,
    pub content_kind: String,

    // Common (optional)
    pub branch: Option<String>,
    pub default_branch: Option<String>,
    pub repo_description: Option<String>,
    pub pushed_at: Option<String>,
    pub is_private: Option<bool>,

    // Repo metadata
    pub stars: Option<u32>,
    pub forks: Option<u32>,
    pub open_issues: Option<u32>,
    pub language: Option<String>,
    pub topics: Option<Vec<String>>,
    pub is_fork: Option<bool>,
    pub is_archived: Option<bool>,

    // Issue / PR
    pub issue_number: Option<u64>,
    pub state: Option<String>,
    pub author: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub comment_count: Option<u32>,
    pub labels: Option<Vec<String>>,
    pub is_pr: Option<bool>,
    pub merged_at: Option<String>,
    pub is_draft: Option<bool>,

    // File
    // NOTE: gh_chunking_method is intentionally absent — chunking_method is set universally
    // by the TEI embed layer (pipeline.rs); no GitHub-specific field needed.
    pub file_path: Option<String>,
    pub file_language: Option<String>,
    pub file_type: Option<String>,
    pub is_test: Option<bool>,
    pub file_size_bytes: Option<usize>,

    // Chunk line range (1-indexed, inclusive)
    pub gh_line_start: Option<u32>,
    pub gh_line_end: Option<u32>,
}

/// Build a Qdrant extra payload with `gh_*` keys (backwards compat) and the
/// canonical `git_*` keys shared across all git providers.
///
/// Null `Option` fields become JSON `null`, ensuring every chunk has the same
/// schema regardless of content kind.
pub fn build_github_payload(params: &GitHubPayloadParams) -> Value {
    let content_kind: &'static str = match params.content_kind.as_str() {
        "issue" => "issue",
        "pr" => "pr",
        "wiki" => "wiki",
        "repo_metadata" => "repo_metadata",
        _ => "file",
    };
    let git = build_git_payload(&GitPayload {
        provider: "github".to_string(),
        host: "github.com".to_string(),
        owner: Some(params.owner.clone()),
        repo: params.repo.clone(),
        content_kind,
        branch: params.branch.clone(),
        state: params.state.clone(),
        number: params.issue_number,
        author: params.author.clone(),
        labels: params.labels.clone().unwrap_or_default(),
        is_draft: params.is_draft,
        merged_at: params.merged_at.clone(),
        created_at: params.created_at.clone(),
        updated_at: params.updated_at.clone(),
        file_path: params.file_path.clone(),
        file_language: params.file_language.clone(),
        meta: Some(json!({
            // Lower-priority extras kept in git_meta (not indexed in Qdrant).
            // Promoted fields (gh_stars, gh_forks, gh_language, gh_topics,
            // gh_is_fork, gh_is_archived, gh_file_type, gh_line_start, gh_line_end)
            // are emitted as flat top-level keys below for Qdrant indexing.
            "open_issues":        params.open_issues,
            "is_private":         params.is_private,
            "default_branch":     params.default_branch,
            "repo_description":   params.repo_description,
            "pushed_at":          params.pushed_at,
            "gh_is_test":         params.is_test,
            "gh_file_size_bytes": params.file_size_bytes,
            "gh_comment_count":   params.comment_count,
            "gh_is_pr":           params.is_pr,
        })),
    });
    let mut payload = git;
    // Backwards-compat: also emit flat gh_* fields so existing Qdrant
    // queries and filters continue to work on already-indexed points.
    let obj = payload
        .as_object_mut()
        .expect("git payload is always an object");
    obj.insert("gh_repo".into(), json!(params.repo));
    obj.insert("gh_owner".into(), json!(params.owner));
    obj.insert("gh_content_kind".into(), json!(params.content_kind));
    obj.insert("gh_branch".into(), json!(params.branch));
    obj.insert("gh_default_branch".into(), json!(params.default_branch));
    obj.insert("gh_repo_description".into(), json!(params.repo_description));
    obj.insert("gh_pushed_at".into(), json!(params.pushed_at));
    obj.insert("gh_is_private".into(), json!(params.is_private));
    obj.insert("gh_stars".into(), json!(params.stars));
    obj.insert("gh_forks".into(), json!(params.forks));
    obj.insert("gh_open_issues".into(), json!(params.open_issues));
    obj.insert("gh_language".into(), json!(params.language));
    obj.insert("gh_topics".into(), json!(params.topics));
    obj.insert("gh_is_fork".into(), json!(params.is_fork));
    obj.insert("gh_is_archived".into(), json!(params.is_archived));
    obj.insert("gh_issue_number".into(), json!(params.issue_number));
    obj.insert("gh_state".into(), json!(params.state));
    obj.insert("gh_author".into(), json!(params.author));
    obj.insert("gh_created_at".into(), json!(params.created_at));
    obj.insert("gh_updated_at".into(), json!(params.updated_at));
    obj.insert("gh_comment_count".into(), json!(params.comment_count));
    obj.insert("gh_labels".into(), json!(params.labels));
    obj.insert("gh_is_pr".into(), json!(params.is_pr));
    obj.insert("gh_merged_at".into(), json!(params.merged_at));
    obj.insert("gh_is_draft".into(), json!(params.is_draft));
    obj.insert("gh_file_path".into(), json!(params.file_path));
    obj.insert("gh_file_language".into(), json!(params.file_language));
    obj.insert("gh_file_type".into(), json!(params.file_type));
    obj.insert("gh_is_test".into(), json!(params.is_test));
    obj.insert("gh_file_size_bytes".into(), json!(params.file_size_bytes));
    obj.insert("gh_line_start".into(), json!(params.gh_line_start));
    obj.insert("gh_line_end".into(), json!(params.gh_line_end));
    payload
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "meta_tests.rs"]
mod tests;
