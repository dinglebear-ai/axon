//! Shared git provider payload builder.
//!
//! All git-backed ingest sources (GitHub, GitLab, Gitea, generic HTTPS git)
//! emit the canonical `git_*` fields defined here alongside any
//! provider-specific extras, so Qdrant filters like
//! `git_content_kind = "issue" AND git_state = "open"` work uniformly
//! across providers.
//!
//! ## Content kinds (canonical values for `git_content_kind`)
//! - `"file"`          — source or doc file from the repository tree
//! - `"issue"`         — issue thread
//! - `"pr"`            — pull request or merge request (normalised from
//!   "pull_request" / "merge_request")
//! - `"release"`       — tagged release (GitHub only for now)
//! - `"wiki"`          — wiki page
//! - `"repo_metadata"` — top-level repository summary

use serde_json::{Value, json};

/// Canonical value for the `git_content_kind` payload field. One variant per
/// kind so a typo can't silently degrade to `"file"` (the previous behavior of
/// the GitHub builder's stringly-typed match).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentKind {
    #[default]
    File,
    Issue,
    Pr,
    Release,
    #[allow(dead_code)]
    Wiki,
    RepoMetadata,
}

impl ContentKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Issue => "issue",
            Self::Pr => "pr",
            Self::Release => "release",
            Self::Wiki => "wiki",
            Self::RepoMetadata => "repo_metadata",
        }
    }
}

/// Parameters for the shared git provider payload.
///
/// Required fields: `provider`, `host`, `repo`, `content_kind`.
/// All other fields are optional; absent values become JSON `null`.
///
/// ## Owner convention
/// - GitHub / Gitea: `owner` = organisation or user login
/// - GitLab: `owner` = namespace path *without* the final project segment
///   (e.g. `"group/subgroup"` for `gitlab.com/group/subgroup/project`)
/// - Generic git: `owner = None` (no API to determine it)
#[derive(Default)]
pub struct GitPayload {
    pub provider: String,
    pub host: String,
    pub owner: Option<String>,
    pub repo: String,
    pub content_kind: ContentKind,
    pub branch: Option<String>,
    pub default_branch: Option<String>,
    pub repo_description: Option<String>,
    pub repo_pushed_at: Option<String>,
    pub repo_is_private: Option<bool>,
    pub repo_stars: Option<u32>,
    pub repo_forks: Option<u32>,
    pub repo_open_issues: Option<u32>,
    pub repo_language: Option<String>,
    pub repo_topics: Option<Vec<String>>,
    pub repo_is_fork: Option<bool>,
    pub repo_is_archived: Option<bool>,
    pub state: Option<String>,
    pub number: Option<u64>,
    pub author: Option<String>,
    pub labels: Vec<String>,
    pub comment_count: Option<u32>,
    pub is_pr: Option<bool>,
    pub is_draft: Option<bool>,
    pub merged_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub file_path: Option<String>,
    pub file_language: Option<String>,
    pub file_type: Option<String>,
    pub file_is_test: Option<bool>,
    pub file_size_bytes: Option<usize>,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    pub chunking_method: Option<String>,
    pub symbol_name: Option<String>,
    pub symbol_kind: Option<String>,
    pub symbol_extraction_status: Option<String>,
    /// Provider-specific extras stored as an opaque blob.
    /// Use for fields that don't generalise (stars, visibility, clone_url, …).
    pub meta: Option<Value>,
}

/// Decoded fields extracted from a git item (issue or PR) extra payload.
///
/// Tuple order: (state, number, author, labels, is_draft, merged_at,
///               created_at, updated_at)
///
/// `number_field` is the JSON key that holds the item's number:
/// - GitLab uses `"iid"` for both issues and merge requests
/// - Gitea/GitHub use `"number"`
#[allow(dead_code)]
pub type GitItemFields = (
    Option<String>,
    Option<u64>,
    Option<String>,
    Option<Vec<String>>,
    Option<bool>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// Extract the common issue/PR fields from an extra-payload JSON blob.
/// Parameterised by `number_field` so GitLab (`"iid"`) and Gitea/GitHub
/// (`"number"`) can both use this function without branching at the call site.
#[allow(dead_code)]
pub fn extract_git_item_fields(extra: &Value, number_field: &str) -> GitItemFields {
    let state = extra
        .get("state")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let number = extra.get(number_field).and_then(|v| v.as_u64());
    let author = extra
        .get("author")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let labels = extra.get("labels").and_then(|v| v.as_array()).map(|a| {
        a.iter()
            .filter_map(|s| s.as_str().map(str::to_string))
            .collect()
    });
    let is_draft = extra.get("is_draft").and_then(|v| v.as_bool());
    let merged_at = extra
        .get("merged_at")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let created_at = extra
        .get("created_at")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    let updated_at = extra
        .get("updated_at")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    (
        state, number, author, labels, is_draft, merged_at, created_at, updated_at,
    )
}

/// Build the canonical `git_*` payload object.
///
/// Returns a flat JSON object. Callers may extend it with additional
/// provider-specific top-level keys by calling `as_object_mut().unwrap().extend(…)`
/// before embedding — but must not overwrite any `git_*` key.
pub fn build_git_payload(p: &GitPayload) -> Value {
    json!({
        "provider":          p.provider,
        "git_host":          p.host,
        "git_owner":         p.owner,
        "git_repo":          p.repo,
        "git_content_kind":  p.content_kind.as_str(),
        "git_branch":        p.branch,
        "git_default_branch": p.default_branch,
        "git_repo_description": p.repo_description,
        "git_repo_pushed_at": p.repo_pushed_at,
        "git_repo_is_private": p.repo_is_private,
        "git_repo_stars":    p.repo_stars,
        "git_repo_forks":    p.repo_forks,
        "git_repo_open_issues": p.repo_open_issues,
        "git_repo_language": p.repo_language,
        "git_repo_topics":   p.repo_topics,
        "git_repo_is_fork":  p.repo_is_fork,
        "git_repo_is_archived": p.repo_is_archived,
        "git_state":         p.state,
        "git_number":        p.number,
        "git_author":        p.author,
        "git_labels":        if p.labels.is_empty() { Value::Null } else { json!(p.labels) },
        "git_comment_count": p.comment_count,
        "git_is_pr":         p.is_pr,
        "git_is_draft":      p.is_draft,
        "git_merged_at":     p.merged_at,
        "git_created_at":    p.created_at,
        "git_updated_at":    p.updated_at,
        "code_file_path":    p.file_path,
        "code_language":     p.file_language,
        "code_file_type":    p.file_type,
        "code_is_test":      p.file_is_test,
        "code_file_size_bytes": p.file_size_bytes,
        "code_line_start":   p.line_start,
        "code_line_end":     p.line_end,
        "code_chunking_method": p.chunking_method,
        "symbol_name":       p.symbol_name,
        "symbol_kind":       p.symbol_kind,
        "symbol_extraction_status": p.symbol_extraction_status,
        "git_meta":          p.meta,
    })
}
