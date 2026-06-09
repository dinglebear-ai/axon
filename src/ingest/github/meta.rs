use octocrab::models;
use serde_json::Value;

use crate::ingest::git_payload::{ContentKind, GitPayload, build_git_payload};

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
    pub content_kind: ContentKind,

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
    pub file_path: Option<String>,
    pub file_language: Option<String>,
    pub file_type: Option<String>,
    pub is_test: Option<bool>,
    pub file_size_bytes: Option<usize>,
    pub chunking_method: Option<String>,
    pub symbol_name: Option<String>,
    pub symbol_kind: Option<String>,
    pub symbol_extraction_status: Option<String>,

    // Chunk line range (1-indexed, inclusive)
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
}

/// Build a Qdrant extra payload with canonical `git_*` and `code_*` keys shared
/// across git-backed providers.
///
/// Null `Option` fields become JSON `null`, ensuring every chunk has the same
/// schema regardless of content kind.
pub fn build_github_payload(params: &GitHubPayloadParams) -> Value {
    build_git_payload(&GitPayload {
        provider: "github".to_string(),
        host: "github.com".to_string(),
        owner: Some(params.owner.clone()),
        repo: params.repo.clone(),
        content_kind: params.content_kind,
        branch: params.branch.clone(),
        default_branch: params.default_branch.clone(),
        repo_description: params.repo_description.clone(),
        repo_pushed_at: params.pushed_at.clone(),
        repo_is_private: params.is_private,
        repo_stars: params.stars,
        repo_forks: params.forks,
        repo_open_issues: params.open_issues,
        repo_language: params.language.clone(),
        repo_topics: params.topics.clone(),
        repo_is_fork: params.is_fork,
        repo_is_archived: params.is_archived,
        state: params.state.clone(),
        number: params.issue_number,
        author: params.author.clone(),
        labels: params.labels.clone().unwrap_or_default(),
        comment_count: params.comment_count,
        is_pr: params.is_pr,
        is_draft: params.is_draft,
        merged_at: params.merged_at.clone(),
        created_at: params.created_at.clone(),
        updated_at: params.updated_at.clone(),
        file_path: params.file_path.clone(),
        file_language: params.file_language.clone(),
        file_type: params.file_type.clone(),
        file_is_test: params.is_test,
        file_size_bytes: params.file_size_bytes,
        line_start: params.line_start,
        line_end: params.line_end,
        chunking_method: params.chunking_method.clone(),
        symbol_name: params.symbol_name.clone(),
        symbol_kind: params.symbol_kind.clone(),
        symbol_extraction_status: params.symbol_extraction_status.clone(),
        meta: None,
    })
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "meta_tests.rs"]
mod tests;
