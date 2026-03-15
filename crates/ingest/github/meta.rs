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
    pub file_path: Option<String>,
    pub file_language: Option<String>,
    pub file_type: Option<String>,
    pub is_test: Option<bool>,
    pub file_size_bytes: Option<usize>,
    pub chunking_method: Option<String>,

    // Chunk line range (1-indexed, inclusive)
    pub gh_line_start: Option<u32>,
    pub gh_line_end: Option<u32>,
}

/// Build a Qdrant extra payload with all 33 `gh_*` keys.
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
        "gh_chunking_method": params.chunking_method,

        // Chunk line range (1-indexed, inclusive)
        "gh_line_start": params.gh_line_start,
        "gh_line_end": params.gh_line_end,
    })
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_common_params() -> GitHubPayloadParams {
        GitHubPayloadParams {
            repo: "axon_rust".into(),
            owner: "jmagar".into(),
            content_kind: "file".into(),
            default_branch: Some("main".into()),
            ..Default::default()
        }
    }

    #[test]
    fn payload_has_31_keys() {
        let params = make_common_params();
        let payload = build_github_payload(&params);
        let obj = payload.as_object().expect("payload is an object");
        assert_eq!(obj.len(), 33, "expected 33 gh_* keys, got {}", obj.len());
    }

    #[test]
    fn payload_common_fields_always_present() {
        let params = make_common_params();
        let payload = build_github_payload(&params);
        assert_eq!(payload["gh_repo"], "axon_rust");
        assert_eq!(payload["gh_owner"], "jmagar");
        assert_eq!(payload["gh_content_kind"], "file");
        assert_eq!(payload["gh_default_branch"], "main");
    }

    #[test]
    fn payload_file_fields_populated() {
        let params = GitHubPayloadParams {
            repo: "axon_rust".into(),
            owner: "jmagar".into(),
            content_kind: "file".into(),
            file_path: Some("src/main.rs".into()),
            file_language: Some("rust".into()),
            file_type: Some("source".into()),
            is_test: Some(false),
            file_size_bytes: Some(1024),
            chunking_method: Some("tree-sitter".into()),
            ..Default::default()
        };
        let payload = build_github_payload(&params);
        assert_eq!(payload["gh_file_path"], "src/main.rs");
        assert_eq!(payload["gh_file_language"], "rust");
        assert_eq!(payload["gh_file_type"], "source");
        assert_eq!(payload["gh_is_test"], false);
        assert_eq!(payload["gh_file_size_bytes"], 1024);
        assert_eq!(payload["gh_chunking_method"], "tree-sitter");
    }

    #[test]
    fn payload_issue_fields_null_for_file_chunks() {
        let params = make_common_params();
        let payload = build_github_payload(&params);
        assert!(payload["gh_issue_number"].is_null());
        assert!(payload["gh_state"].is_null());
        assert!(payload["gh_author"].is_null());
        assert!(payload["gh_labels"].is_null());
        assert!(payload["gh_is_pr"].is_null());
        assert!(payload["gh_merged_at"].is_null());
        assert!(payload["gh_is_draft"].is_null());
    }

    #[test]
    fn payload_repo_metadata_null_for_file_chunks() {
        let params = make_common_params();
        let payload = build_github_payload(&params);
        assert!(payload["gh_stars"].is_null());
        assert!(payload["gh_forks"].is_null());
        assert!(payload["gh_open_issues"].is_null());
        assert!(payload["gh_language"].is_null());
        assert!(payload["gh_is_fork"].is_null());
        assert!(payload["gh_is_archived"].is_null());
    }

    #[test]
    fn payload_issue_params_produce_correct_values() {
        let params = GitHubPayloadParams {
            repo: "axon_rust".into(),
            owner: "jmagar".into(),
            content_kind: "issue".into(),
            issue_number: Some(42),
            state: Some("open".into()),
            author: Some("contributor".into()),
            created_at: Some("2026-01-15T12:00:00Z".into()),
            updated_at: Some("2026-01-16T08:30:00Z".into()),
            comment_count: Some(5),
            labels: Some(vec!["bug".into(), "urgent".into()]),
            is_pr: Some(false),
            ..Default::default()
        };
        let payload = build_github_payload(&params);
        assert_eq!(payload["gh_content_kind"], "issue");
        assert_eq!(payload["gh_issue_number"], 42);
        assert_eq!(payload["gh_state"], "open");
        assert_eq!(payload["gh_author"], "contributor");
        assert_eq!(payload["gh_comment_count"], 5);
        assert_eq!(payload["gh_labels"], json!(["bug", "urgent"]));
        assert_eq!(payload["gh_is_pr"], false);
        // File fields should be null for issue chunks
        assert!(payload["gh_file_path"].is_null());
        assert!(payload["gh_chunking_method"].is_null());
    }

    #[test]
    fn payload_all_keys_start_with_gh_prefix() {
        let params = make_common_params();
        let payload = build_github_payload(&params);
        let obj = payload.as_object().unwrap();
        for key in obj.keys() {
            assert!(key.starts_with("gh_"), "key {key} missing gh_ prefix");
        }
    }
}
