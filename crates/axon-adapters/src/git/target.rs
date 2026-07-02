//! Git target parsing — normalizes an `https` clone URL into provider / host /
//! owner / repo parts. Ported from the legacy `axon-ingest::generic_git`
//! parser, minus the network/config coupling.

use axon_api::source::ApiError;
use axon_error::ErrorStage;
use url::Url;

use crate::adapter::Result;

/// A parsed git repository target. `web_url` has any embedded credentials
/// stripped and the trailing `.git` removed, so it is safe to surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitTarget {
    pub clone_url: String,
    pub web_url: String,
    pub host: String,
    pub owner: Option<String>,
    pub repo: String,
    pub provider: String,
}

fn err(code: &str, message: impl Into<String>) -> ApiError {
    ApiError::new(format!("adapter.git.{code}"), ErrorStage::Planning, message)
}

/// Provider label derived from the host (github/gitlab/gitea/git).
fn provider_for_host(host: &str) -> String {
    if host.contains("github") {
        "github".to_string()
    } else if host.contains("gitlab") {
        "gitlab".to_string()
    } else if host.contains("gitea") || host.contains("forgejo") || host.contains("codeberg") {
        "gitea".to_string()
    } else {
        "git".to_string()
    }
}

/// Parse a `git:`-prefixed or bare `https://host/owner/repo(.git)` target.
pub fn parse_git_target(input: &str) -> Result<GitTarget> {
    let raw = input.trim();
    let raw = raw.strip_prefix("git:").unwrap_or(raw).trim();
    let url = Url::parse(raw).map_err(|e| err("target.invalid", e.to_string()))?;
    if url.scheme() != "https" {
        return Err(err(
            "target.scheme",
            "git adapter requires an https clone URL",
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| err("target.host", "git target is missing host"))?
        .to_ascii_lowercase();
    let path = url.path().trim_matches('/');
    if path.is_empty() {
        return Err(err("target.path", "git target is missing repository path"));
    }
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let repo = segments
        .last()
        .copied()
        .unwrap_or(path)
        .trim_end_matches(".git")
        .to_string();
    let owner = if segments.len() >= 2 {
        Some(segments[segments.len() - 2].to_string())
    } else {
        None
    };
    let mut web = url.clone();
    let _ = web.set_username("");
    let _ = web.set_password(None);
    let web_url = web.as_str().trim_end_matches(".git").to_string();
    Ok(GitTarget {
        clone_url: url.to_string(),
        web_url,
        provider: provider_for_host(&host),
        host,
        owner,
        repo,
    })
}

#[cfg(test)]
#[path = "target_tests.rs"]
mod tests;
