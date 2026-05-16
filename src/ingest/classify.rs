use crate::jobs::ingest::IngestSource;
use reqwest::Url;
use std::error::Error;

/// Auto-detect ingest source from a raw user input string.
///
/// Routing rules (checked in order):
///   1. Reddit: `r/` prefix or reddit.com host
///   2. YouTube: `@handle` (expanded to full URL), youtube.com/youtu.be host, or bare 11-char video ID
///   3. GitHub: github.com host or `owner/repo` slug (exactly one `/`)
pub fn classify_target(input: &str, include_source: bool) -> Result<IngestSource, Box<dyn Error>> {
    let s = input.trim();

    // 1. Reddit: r/ prefix or reddit.com host
    if s.starts_with("r/") || is_host(s, &["reddit.com", "www.reddit.com", "old.reddit.com"]) {
        return Ok(IngestSource::Reddit {
            target: s.to_string(),
        });
    }

    // 2. YouTube: @handle → expand to full channel URL
    if s.starts_with('@') {
        return Ok(IngestSource::Youtube {
            target: format!("https://www.youtube.com/{s}"),
        });
    }
    if is_host(
        s,
        &[
            "youtube.com",
            "www.youtube.com",
            "m.youtube.com",
            "youtu.be",
        ],
    ) || is_bare_video_id(s)
    {
        return Ok(IngestSource::Youtube {
            target: s.to_string(),
        });
    }

    // 3. GitHub: URL or owner/repo slug
    if is_host(s, &["github.com", "www.github.com"]) {
        let repo = extract_github_repo_from_url(s)?;
        return Ok(IngestSource::Github {
            repo,
            include_source,
        });
    }
    if is_github_slug(s) {
        return Ok(IngestSource::Github {
            repo: s.to_string(),
            include_source,
        });
    }

    Err(format!(
        "cannot determine ingest source from '{s}': \
         use a GitHub slug (owner/repo) or URL, \
         YouTube URL or @handle, \
         or Reddit subreddit (r/name) or URL"
    )
    .into())
}

fn is_host(input: &str, hosts: &[&str]) -> bool {
    Url::parse(input)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
        .map(|h| hosts.iter().any(|&target| h == target))
        .unwrap_or(false)
}

fn is_bare_video_id(s: &str) -> bool {
    s.len() == 11
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn is_github_slug(s: &str) -> bool {
    // Must be exactly "owner/repo" — two non-empty parts, no extra slashes
    let parts: Vec<&str> = s.splitn(3, '/').collect();
    if parts.len() != 2 {
        return false;
    }
    let owner_ok = !parts[0].is_empty()
        && parts[0]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    let repo_ok = !parts[1].is_empty()
        && parts[1]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    owner_ok && repo_ok
}

fn extract_github_repo_from_url(s: &str) -> Result<String, Box<dyn Error>> {
    let u = Url::parse(s)?;
    let path = u.path().trim_start_matches('/').trim_end_matches('/');
    let parts: Vec<&str> = path.splitn(3, '/').collect();
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(format!("invalid GitHub URL '{s}': expected github.com/owner/repo").into());
    }
    Ok(format!("{}/{}", parts[0], parts[1]))
}

#[cfg(test)]
#[path = "classify_tests.rs"]
mod tests;
