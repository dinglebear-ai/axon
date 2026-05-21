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
    pub content_kind: &'static str,
    pub branch: Option<String>,
    pub state: Option<String>,
    pub number: Option<u64>,
    pub author: Option<String>,
    pub labels: Vec<String>,
    pub is_draft: Option<bool>,
    pub merged_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub file_path: Option<String>,
    pub file_language: Option<String>,
    /// Provider-specific extras stored as an opaque blob.
    /// Use for fields that don't generalise (stars, visibility, clone_url, …).
    pub meta: Option<Value>,
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
        "git_content_kind":  p.content_kind,
        "git_branch":        p.branch,
        "git_state":         p.state,
        "git_number":        p.number,
        "git_author":        p.author,
        "git_labels":        if p.labels.is_empty() { Value::Null } else { json!(p.labels) },
        "git_is_draft":      p.is_draft,
        "git_merged_at":     p.merged_at,
        "git_created_at":    p.created_at,
        "git_updated_at":    p.updated_at,
        "git_file_path":     p.file_path,
        "git_file_language": p.file_language,
        "git_meta":          p.meta,
    })
}
