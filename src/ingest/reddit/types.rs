use std::error::Error;
use tokio_util::sync::CancellationToken;

/// Validate a subreddit name to prevent path traversal and injection attacks.
/// Reddit subreddit names are 3-21 characters, alphanumeric and underscores only.
pub(crate) fn validate_subreddit(name: &str) -> Result<(), Box<dyn Error>> {
    let len = name.len();
    let valid =
        (3..=21).contains(&len) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !valid {
        return Err(format!(
            "invalid subreddit name '{name}': must be 3-21 chars, alphanumeric and underscore only"
        )
        .into());
    }
    Ok(())
}

/// Context for a single Reddit comment including optional parent text for threading.
pub(super) struct CommentWithContext {
    pub body: String,
    pub parent_text: Option<String>,
}

/// Source-level controls that can be wired by job/service layers without
/// changing Reddit parsing or embedding behavior.
#[derive(Clone, Default)]
pub struct RedditIngestOptions {
    cancel_token: Option<CancellationToken>,
}

impl RedditIngestOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_cancel_token(cancel_token: CancellationToken) -> Self {
        Self {
            cancel_token: Some(cancel_token),
        }
    }

    pub(super) fn cancel_token(&self) -> Option<&CancellationToken> {
        self.cancel_token.as_ref()
    }

    pub(super) fn check_cancelled(&self) -> Result<(), Box<dyn Error>> {
        if self
            .cancel_token
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
        {
            return Err("reddit ingest canceled".into());
        }
        Ok(())
    }
}

/// Reddit ingest result details available for later service/job surfacing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RedditIngestStats {
    pub posts_seen: usize,
    pub posts_prepared: usize,
    pub comment_fetch_attempts: usize,
    pub comment_fetch_failures: usize,
}

impl RedditIngestStats {
    pub fn has_partial_comment_failures(&self) -> bool {
        self.comment_fetch_failures > 0
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RedditIngestSummary {
    pub chunks_embedded: usize,
    pub stats: RedditIngestStats,
}

/// Discriminates between a subreddit name and a specific thread URL.
#[derive(Debug, PartialEq, Eq)]
pub enum RedditTarget {
    /// r/subreddit -- fetch hot posts
    Subreddit(String),
    /// Specific thread URL -- fetch that thread + comments
    Thread(String),
}

/// Classify a user-provided target string as a subreddit name or thread URL.
pub fn classify_target(target: &str) -> Result<RedditTarget, Box<dyn Error>> {
    let target = target.trim();
    if target.is_empty() {
        return Err("reddit target cannot be empty".into());
    }

    if let Ok(url) = reqwest::Url::parse(target) {
        let host = url.host_str().unwrap_or("").to_ascii_lowercase();
        if !is_allowed_reddit_host(&host) {
            if url.path().contains("/comments/") {
                return Err(format!("non-Reddit comments URL rejected: {target}").into());
            }
            return Err(format!("invalid Reddit URL host '{host}'").into());
        }
        return classify_reddit_path(url.path());
    }

    if target.contains("/comments/") {
        let path = if target.starts_with("/r/") {
            target.to_string()
        } else if target.starts_with("r/") {
            format!("/{target}")
        } else {
            return Err(format!("non-Reddit comments target rejected: {target}").into());
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

fn classify_reddit_path(path: &str) -> Result<RedditTarget, Box<dyn Error>> {
    if path.contains("/comments/") {
        return Ok(RedditTarget::Thread(canonical_thread_permalink(path)?));
    }

    let rest = path
        .strip_prefix("/r/")
        .ok_or_else(|| format!("invalid Reddit target path '{path}': expected /r/<subreddit>"))?;
    let name = rest.split('/').next().unwrap_or("").trim();
    validate_subreddit(name)?;
    Ok(RedditTarget::Subreddit(name.to_string()))
}

fn is_allowed_reddit_host(host: &str) -> bool {
    matches!(host, "reddit.com" | "www.reddit.com" | "old.reddit.com")
}

fn canonical_thread_permalink(path: &str) -> Result<String, Box<dyn Error>> {
    let path = path.split(['?', '#']).next().unwrap_or(path);
    let normalized = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let trimmed = normalized.trim_end_matches('/');
    let parts: Vec<&str> = trimmed.split('/').collect();

    if parts.len() < 5 || parts[1] != "r" || parts[3] != "comments" {
        return Err(format!(
            "invalid Reddit thread path '{path}': expected /r/<subreddit>/comments/<id>"
        )
        .into());
    }

    validate_subreddit(parts[2])?;
    let post_id = parts[4];
    if post_id.is_empty()
        || !post_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!("invalid Reddit thread id in path '{path}'").into());
    }

    let title = parts.get(5).copied().unwrap_or("");
    if !title.is_empty()
        && !title
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!("invalid Reddit thread slug in path '{path}'").into());
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
#[path = "types_tests.rs"]
mod tests;
