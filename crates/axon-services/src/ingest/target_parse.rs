//! Pure target-string parsing/normalization for git-provider origins.
//!
//! Relocated from the (Phase 12 clean break) `gitea.rs`/`gitlab/types.rs`/
//! `generic_git.rs` modules, whose real client/embed logic was deleted —
//! `classify_target` (and, transitively, `axon refresh`'s origin
//! reclassification) still needs to turn a raw target string into a
//! canonical `IngestSource` even though nothing executes non-Sessions
//! ingest sources anymore (see `crates/axon-jobs/src/workers/runners/
//! ingest.rs`).

use std::error::Error;

use anyhow::{Result, anyhow, bail};
use reqwest::Url;

use axon_core::http::validate_url;

// ── Gitea/Forgejo ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GiteaTarget {
    pub host: String,
    pub owner: String,
    pub repo: String,
}

impl GiteaTarget {
    pub(crate) fn as_normalized_target(&self) -> String {
        format!("{}/{}/{}", self.host, self.owner, self.repo)
    }
}

pub fn normalize_gitea_target(input: &str) -> Result<String> {
    Ok(parse_gitea_target(input)?.as_normalized_target())
}

pub fn parse_gitea_target(input: &str) -> Result<GiteaTarget> {
    let raw = input
        .trim()
        .strip_prefix("gitea:")
        .or_else(|| input.trim().strip_prefix("forgejo:"))
        .unwrap_or(input.trim());
    let parsed = if raw.starts_with("http://") || raw.starts_with("https://") {
        Url::parse(raw)?
    } else {
        Url::parse(&format!("https://{raw}"))?
    };
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        bail!("invalid Gitea target '{input}': expected http(s) URL");
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("invalid Gitea target '{input}': missing host"))?
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    let segments: Vec<&str> = parsed
        .path()
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    if segments.len() < 2 {
        bail!("invalid Gitea target '{input}': expected host/owner/repo");
    }
    let owner = segments[0].to_string();
    let repo = segments[1].trim_end_matches(".git").to_string();
    let web_url = format!("{}://{host}/{owner}/{repo}", parsed.scheme());
    validate_url(&web_url)?;
    Ok(GiteaTarget { host, owner, repo })
}

// ── GitLab ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitLabTarget {
    pub host: String,
    pub namespace_path: String,
}

impl GitLabTarget {
    pub(crate) fn as_normalized_target(&self) -> String {
        format!("{}/{}", self.host, self.namespace_path)
    }
}

pub fn normalize_gitlab_target(input: &str) -> std::result::Result<String, Box<dyn Error>> {
    Ok(parse_gitlab_target(input)?.as_normalized_target())
}

pub fn parse_gitlab_target(input: &str) -> Result<GitLabTarget> {
    let raw = input.trim();
    let raw = raw.strip_prefix("gitlab:").unwrap_or(raw).trim();
    let parsed = if raw.starts_with("http://") || raw.starts_with("https://") {
        Url::parse(raw)?
    } else {
        Url::parse(&format!("https://{raw}"))?
    };

    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        bail!("invalid GitLab target '{input}': expected http:// or https:// URL");
    }
    let scheme = parsed.scheme();
    let host = parsed
        .host_str()
        .ok_or_else(|| anyhow!("invalid GitLab target '{input}': missing host"))?
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    let mut segments: Vec<&str> = parsed
        .path()
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    if let Some(marker) = segments.iter().position(|part| *part == "-") {
        segments.truncate(marker);
    }
    if segments.len() < 2 {
        bail!("invalid GitLab target '{input}': expected host/group/project");
    }
    let mut parts: Vec<String> = segments.into_iter().map(str::to_string).collect();
    if let Some(last) = parts.last_mut() {
        *last = last.trim_end_matches(".git").to_string();
    }
    let namespace_path = parts.join("/");
    let web_url = format!("{scheme}://{host}/{namespace_path}");
    validate_url(&web_url)?;
    Ok(GitLabTarget {
        host,
        namespace_path,
    })
}

// ── Generic git (bare https clone URL) ──────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericGitTarget {
    pub clone_url: String,
}

pub fn normalize_generic_git_target(input: &str) -> Result<String> {
    Ok(parse_generic_git_target(input)?.clone_url)
}

pub fn parse_generic_git_target(input: &str) -> Result<GenericGitTarget> {
    let raw = input.trim();
    let raw = raw.strip_prefix("git:").unwrap_or(raw).trim();
    let url = Url::parse(raw)?;
    if url.scheme() != "https" {
        bail!("generic git ingest requires an https clone URL");
    }
    validate_url(url.as_str())?;
    let path = url.path().trim_matches('/');
    if path.is_empty() {
        bail!("generic git target is missing repository path");
    }
    Ok(GenericGitTarget {
        clone_url: url.to_string(),
    })
}

// ── GitHub (bare owner/repo slug parsing only) ──────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubTarget {
    pub owner: String,
    pub repo: String,
    pub repo_slug: String,
}

/// Parse an "owner/repo" string into (owner, repo) parts.
/// Accepts both "owner/repo" and "https://github.com/owner/repo" forms.
pub fn parse_github_repo(input: &str) -> Option<(String, String)> {
    parse_github_target(input).map(|target| (target.owner, target.repo))
}

/// Parse an "owner/repo" string into a normalized GitHub target.
/// Accepts both "owner/repo" and "https://github.com/owner/repo" forms.
pub fn parse_github_target(input: &str) -> Option<GitHubTarget> {
    let (slug, is_url) = match input.strip_prefix("https://github.com/") {
        Some(rest) => (rest.trim_end_matches('/'), true),
        None => (input, false),
    };

    let mut parts = slug.split('/');
    let owner = parts.next().filter(|s| !s.is_empty())?;
    let repo = parts.next().filter(|s| !s.is_empty())?;
    // URL form accepts extra path segments (e.g. pasted /tree/main); slug form does not.
    if !is_url && parts.next().is_some() {
        return None;
    }

    // Strip .git suffix commonly found in clone URLs
    let repo = repo.strip_suffix(".git").unwrap_or(repo);

    if repo.is_empty() {
        return None;
    }

    let owner = owner.to_string();
    let repo = repo.to_string();
    let repo_slug = format!("{owner}/{repo}");

    Some(GitHubTarget {
        owner,
        repo,
        repo_slug,
    })
}

// ── Reddit (target string parsing only) ─────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedditTarget {
    /// r/subreddit -- fetch hot posts
    Subreddit(String),
    /// Specific thread URL -- fetch that thread + comments
    Thread(String),
}

/// Validate a subreddit name to prevent path traversal and injection attacks.
/// Reddit subreddit names are 3-21 characters, alphanumeric and underscores only.
pub fn validate_subreddit(name: &str) -> Result<()> {
    let len = name.len();
    let valid =
        (3..=21).contains(&len) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid {
        bail!(
            "invalid subreddit name '{name}': must be 3-21 chars, alphanumeric and underscore only"
        );
    }
    Ok(())
}

/// Classify a user-provided target string as a subreddit name or thread URL.
pub fn classify_reddit_target(target: &str) -> Result<RedditTarget> {
    let target = target.trim();
    if target.is_empty() {
        bail!("reddit target cannot be empty");
    }

    if let Ok(url) = Url::parse(target) {
        let host = url.host_str().unwrap_or("").to_ascii_lowercase();
        if !is_allowed_reddit_host(&host) {
            if url.path().contains("/comments/") {
                bail!("non-Reddit comments URL rejected: {target}");
            }
            bail!("invalid Reddit URL host '{host}'");
        }
        return classify_reddit_path(url.path());
    }

    if target.contains("/comments/") {
        let path = if target.starts_with("/r/") {
            target.to_string()
        } else if let Some(rest) = target.strip_prefix("r/") {
            format!("/r/{rest}")
        } else {
            format!("/{target}")
        };
        return Ok(RedditTarget::Thread(canonical_thread_permalink(&path)?));
    }

    if let Some(path) = target
        .strip_prefix("/r/")
        .or_else(|| target.strip_prefix("r/"))
    {
        let name = path.split('/').next().unwrap_or("").trim();
        validate_subreddit(name)?;
        return Ok(RedditTarget::Subreddit(name.to_string()));
    }

    validate_subreddit(target)?;
    Ok(RedditTarget::Subreddit(target.to_string()))
}

fn classify_reddit_path(path: &str) -> Result<RedditTarget> {
    if path.contains("/comments/") {
        return Ok(RedditTarget::Thread(canonical_thread_permalink(path)?));
    }

    let rest = path
        .strip_prefix("/r/")
        .ok_or_else(|| anyhow!("invalid Reddit target path '{path}': expected /r/<subreddit>"))?;
    let name = rest.split('/').next().unwrap_or("").trim();
    validate_subreddit(name)?;
    Ok(RedditTarget::Subreddit(name.to_string()))
}

fn is_allowed_reddit_host(host: &str) -> bool {
    matches!(host, "reddit.com" | "www.reddit.com" | "old.reddit.com")
}

fn canonical_thread_permalink(path: &str) -> Result<String> {
    let path = path.split(['?', '#']).next().unwrap_or(path);
    let normalized = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let trimmed = normalized.trim_end_matches('/');
    let parts: Vec<&str> = trimmed.split('/').collect();

    if parts.len() < 5 || parts[1] != "r" || parts[3] != "comments" {
        bail!("invalid Reddit thread path '{path}': expected /r/<subreddit>/comments/<id>");
    }
    Ok(trimmed.to_string())
}

// ── YouTube (target string parsing only) ────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YoutubeTargetKind {
    SingleVideo,
    PlaylistOrChannel,
}

pub fn extract_video_id(input: &str) -> Option<String> {
    if let Ok(url) = Url::parse(input) {
        let host = url.host_str().unwrap_or("");

        if host == "www.youtube.com" || host == "youtube.com" || host == "m.youtube.com" {
            for (key, value) in url.query_pairs() {
                if key == "v" {
                    return Some(value.into_owned());
                }
            }
            if let Some(id) = url.path_segments().and_then(|mut segs| {
                let first = segs.next()?;
                if matches!(first, "embed" | "shorts" | "v") {
                    segs.next().map(|s| s.to_string())
                } else {
                    None
                }
            }) && !id.is_empty()
            {
                return Some(id);
            }
            return None;
        }

        if host == "youtu.be" {
            let path = url.path().trim_start_matches('/');
            if !path.is_empty() {
                return Some(path.to_string());
            }
            return None;
        }

        return None;
    }

    let trimmed = input.trim();
    if trimmed.len() == 11
        && trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Some(trimmed.to_string());
    }

    None
}

pub fn is_playlist_or_channel_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or("");
    if !matches!(host, "www.youtube.com" | "youtube.com" | "m.youtube.com") {
        return false;
    }
    if parsed.query_pairs().any(|(k, _)| k == "list")
        && !parsed.query_pairs().any(|(k, _)| k == "v")
    {
        return true;
    }
    if let Some(first_seg) = parsed.path_segments().and_then(|mut s| s.next()) {
        if matches!(first_seg, "c" | "channel" | "user") {
            return true;
        }
        if first_seg.starts_with('@') {
            return true;
        }
    }
    false
}

fn normalize_youtube_target(target: &str) -> String {
    let trimmed = target.trim();
    if trimmed.starts_with('@') {
        format!("https://www.youtube.com/{trimmed}")
    } else {
        trimmed.to_string()
    }
}

pub fn classify_youtube_target(
    target: &str,
) -> std::result::Result<YoutubeTargetKind, &'static str> {
    let normalized = normalize_youtube_target(target);
    if is_playlist_or_channel_url(&normalized) {
        return Ok(YoutubeTargetKind::PlaylistOrChannel);
    }
    if extract_video_id(&normalized).is_some() {
        return Ok(YoutubeTargetKind::SingleVideo);
    }
    Err("target does not appear to be a YouTube video, playlist, or channel")
}

#[cfg(test)]
#[path = "target_parse_tests.rs"]
mod tests;
