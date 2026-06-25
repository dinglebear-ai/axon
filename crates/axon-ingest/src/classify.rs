use axon_api::ingest::IngestSource;
use reqwest::Url;
use std::error::Error;

/// Auto-detect ingest source from a raw user input string.
///
/// Routing rules (checked in order):
///   1. Reddit: `r/` prefix or reddit.com host
///   2. YouTube: `@handle` (expanded to full URL), youtube.com/youtu.be host, or bare 11-char video ID
///   3. GitLab: gitlab.com URL or explicit `gitlab:` target
///   4. Gitea/Forgejo: known public hosts or explicit `gitea:` / `forgejo:` target
///   5. Generic Git: explicit `git:` HTTPS clone URL
///   6. GitHub: github.com host or `owner/repo` slug (exactly one `/`)
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

    // 3. GitLab: URL or explicit prefix. Bare nested paths are ambiguous, so
    // keep `owner/repo` reserved for the existing GitHub shorthand.
    if is_host(s, &["gitlab.com", "www.gitlab.com"]) || s.starts_with("gitlab:") {
        let target = crate::gitlab::normalize_gitlab_target(s)?;
        return Ok(IngestSource::Gitlab {
            target,
            include_source,
        });
    }

    if is_host(
        s,
        &[
            "gitea.com",
            "www.gitea.com",
            "codeberg.org",
            "www.codeberg.org",
        ],
    ) || s.starts_with("gitea:")
        || s.starts_with("forgejo:")
    {
        let target = crate::gitea::normalize_gitea_target(s)?;
        return Ok(IngestSource::Gitea {
            target,
            include_source,
        });
    }

    if s.starts_with("git:") {
        let target = crate::generic_git::normalize_generic_git_target(s)?;
        return Ok(IngestSource::GenericGit {
            target,
            include_source,
        });
    }

    // 5b. RSS / Atom feed: explicit `rss:` / `feed:` / `atom:` prefix, or a
    // URL shaped like a feed (`.rss`/`.atom`/`.rdf` extension, a `feed`/`rss`/
    // `atom` path segment, or a `?feed=` query). Checked after the provider
    // hosts so github.com/reddit/youtube URLs are never misrouted, but before
    // the bare GitHub slug so `owner/repo/releases.atom` resolves to a feed.
    if let Some(target) = classify_feed_target(s) {
        return Ok(IngestSource::Rss { target });
    }

    // 6. GitHub: URL or owner/repo slug
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
         GitLab URL or gitlab:<host>/<namespace>/<project>, \
         Gitea/Forgejo URL or gitea:<host>/<owner>/<repo>, \
         git:https://host/path/repo.git, \
         YouTube URL or @handle, \
         Reddit subreddit (r/name) or URL, \
         or an RSS/Atom feed URL (or rss:/feed:/atom: prefix)"
    )
    .into())
}

/// Resolve a raw input to an RSS/Atom feed target, or `None` if it doesn't look
/// like a feed. Honors explicit `rss:` / `feed:` / `atom:` prefixes (scheme is
/// added when missing) and otherwise applies a conservative URL-shape heuristic.
fn classify_feed_target(s: &str) -> Option<String> {
    for prefix in ["rss:", "feed:", "atom:"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            let rest = rest.trim();
            if rest.is_empty() {
                return None;
            }
            return Some(if rest.contains("://") {
                rest.to_string()
            } else {
                format!("https://{rest}")
            });
        }
    }
    looks_like_feed_url(s).then(|| s.to_string())
}

/// Conservative heuristic: does this URL look like an RSS/Atom/RDF feed?
fn looks_like_feed_url(s: &str) -> bool {
    let Ok(u) = Url::parse(s) else {
        return false;
    };
    if !matches!(u.scheme(), "http" | "https") {
        return false;
    }
    let path = u.path().to_ascii_lowercase();
    let has_feed_ext = path.ends_with(".rss") || path.ends_with(".atom") || path.ends_with(".rdf");
    let last_segment = path.rsplit('/').next().unwrap_or("");
    let common_feed_file = matches!(
        last_segment,
        "feed.xml" | "rss.xml" | "atom.xml" | "index.xml" | "feed" | "rss" | "atom"
    );
    let feed_segment = path
        .split('/')
        .any(|seg| matches!(seg, "feed" | "feeds" | "rss" | "atom"));
    // Match a real feed query parameter (e.g. `?feed=rss2`, `?format=atom`),
    // not any query that merely mentions a feed word. A bare `feed` key counts;
    // a feed-shaped value (`rss`/`atom`/…) counts only under a format-selecting
    // key, so `?feedback=1` and `?category=atom` are NOT treated as feeds.
    let feed_query = u.query_pairs().any(|(k, v)| {
        let k = k.to_ascii_lowercase();
        k == "feed"
            || ((k == "format" || k == "type" || k == "output")
                && matches!(
                    v.to_ascii_lowercase().as_str(),
                    "rss" | "rss2" | "atom" | "rdf" | "feed"
                ))
    });
    has_feed_ext || common_feed_file || feed_segment || feed_query
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
