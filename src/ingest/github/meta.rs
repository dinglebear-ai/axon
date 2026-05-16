use octocrab::models;
use serde_json::{Value, json};

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

/// Build a Qdrant extra payload with all 32 `gh_*` keys.
///
/// Null `Option` fields become JSON `null`, ensuring every chunk has the same
/// schema regardless of content kind.
pub fn build_github_payload(params: &GitHubPayloadParams) -> Value {
    json!({
        // Common
        "gh_repo": params.repo,
        "gh_owner": params.owner,
        "gh_content_kind": params.content_kind,
        "gh_branch": params.branch,
        "gh_default_branch": params.default_branch,
        "gh_repo_description": params.repo_description,
        "gh_pushed_at": params.pushed_at,
        "gh_is_private": params.is_private,

        // Repo metadata
        "gh_stars": params.stars,
        "gh_forks": params.forks,
        "gh_open_issues": params.open_issues,
        "gh_language": params.language,
        "gh_topics": params.topics,
        "gh_is_fork": params.is_fork,
        "gh_is_archived": params.is_archived,

        // Issue / PR
        "gh_issue_number": params.issue_number,
        "gh_state": params.state,
        "gh_author": params.author,
        "gh_created_at": params.created_at,
        "gh_updated_at": params.updated_at,
        "gh_comment_count": params.comment_count,
        "gh_labels": params.labels,
        "gh_is_pr": params.is_pr,
        "gh_merged_at": params.merged_at,
        "gh_is_draft": params.is_draft,

        // File
        "gh_file_path": params.file_path,
        "gh_file_language": params.file_language,
        "gh_file_type": params.file_type,
        "gh_is_test": params.is_test,
        "gh_file_size_bytes": params.file_size_bytes,

        // Chunk line range (1-indexed, inclusive)
        "gh_line_start": params.gh_line_start,
        "gh_line_end": params.gh_line_end,
    })
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "meta_tests.rs"]
mod tests;
