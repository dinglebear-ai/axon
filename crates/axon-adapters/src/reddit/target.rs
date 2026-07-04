//! Reddit target parsing — classifies a caller-provided string as a subreddit
//! name or a specific thread permalink. Ported from the legacy
//! `axon-ingest::reddit::types` classifier, minus the network/OAuth coupling.

use axon_api::source::ApiError;
use axon_error::ErrorStage;

use crate::adapter::Result;

/// A parsed Reddit acquisition target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedditTarget {
    /// `r/<subreddit>` — fetch a subreddit's post listing.
    Subreddit(String),
    /// A specific thread permalink (e.g. `/r/rust/comments/abc123/title/`).
    Thread(String),
}

fn err(code: &str, message: impl Into<String>) -> ApiError {
    ApiError::new(
        format!("adapter.reddit.{code}"),
        ErrorStage::Planning,
        message,
    )
}

/// Validate a subreddit name to prevent path traversal and injection attacks.
/// Reddit subreddit names are 3-21 characters, alphanumeric and underscores only.
pub fn validate_subreddit(name: &str) -> Result<()> {
    let len = name.len();
    let valid =
        (3..=21).contains(&len) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid {
        return Err(err(
            "target.subreddit_invalid",
            format!(
                "invalid subreddit name '{name}': must be 3-21 chars, alphanumeric and underscore only"
            ),
        ));
    }
    Ok(())
}

/// Classify a user-provided target string as a subreddit name or thread URL.
pub fn parse_reddit_target(input: &str) -> Result<RedditTarget> {
    let target = input.trim();
    if target.is_empty() {
        return Err(err("target.empty", "reddit target cannot be empty"));
    }

    if let Ok(url) = url::Url::parse(target) {
        let host = url.host_str().unwrap_or("").to_ascii_lowercase();
        if !is_allowed_reddit_host(&host) {
            if url.path().contains("/comments/") {
                return Err(err(
                    "target.host_invalid",
                    format!("non-Reddit comments URL rejected: {target}"),
                ));
            }
            return Err(err(
                "target.host_invalid",
                format!("invalid Reddit URL host '{host}'"),
            ));
        }
        return classify_reddit_path(url.path());
    }

    if target.contains("/comments/") {
        let path = if target.starts_with("/r/") {
            target.to_string()
        } else if target.starts_with("r/") {
            format!("/{target}")
        } else {
            return Err(err(
                "target.host_invalid",
                format!("non-Reddit comments target rejected: {target}"),
            ));
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

    let rest = path.strip_prefix("/r/").ok_or_else(|| {
        err(
            "target.path_invalid",
            format!("invalid Reddit target path '{path}': expected /r/<subreddit>"),
        )
    })?;
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
        return Err(err(
            "target.thread_path_invalid",
            format!("invalid Reddit thread path '{path}': expected /r/<subreddit>/comments/<id>"),
        ));
    }

    validate_subreddit(parts[2])?;
    let post_id = parts[4];
    if post_id.is_empty()
        || !post_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(err(
            "target.thread_id_invalid",
            format!("invalid Reddit thread id in path '{path}'"),
        ));
    }

    let title = parts.get(5).copied().unwrap_or("");
    if !title.is_empty()
        && !title
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(err(
            "target.thread_slug_invalid",
            format!("invalid Reddit thread slug in path '{path}'"),
        ));
    }

    let mut permalink = format!("/r/{}/comments/{post_id}", parts[2]);
    if !title.is_empty() {
        permalink.push('/');
        permalink.push_str(title);
    }
    permalink.push('/');

    if parts.len() > 6 {
        let comment_id = parts[6];
        if !comment_id.is_empty()
            && comment_id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            permalink.push_str(comment_id);
            permalink.push('/');
        }
    }

    Ok(permalink)
}

#[cfg(test)]
#[path = "target_tests.rs"]
mod tests;
