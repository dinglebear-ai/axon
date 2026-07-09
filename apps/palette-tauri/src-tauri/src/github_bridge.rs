//! Tauri bridge command(s) for the real GitHub REST API.
//!
//! The frontend CSP (`tauri.conf.json` → `connect-src`) locks network access to
//! `'self' ipc: http://ipc.localhost` — the renderer cannot `fetch()`
//! `api.github.com` directly. This module is the only place in the desktop
//! shell that talks to GitHub: it proxies a small, allow-listed set of GitHub
//! REST endpoints (repo listing, tree, file contents) through `reqwest`,
//! attaching a bearer token when `GITHUB_TOKEN` is configured in the user's
//! `~/.axon/.env` (read via `persistence::read_default_env_entries`, the same
//! source the palette settings screen already surfaces) and falling back to
//! unauthenticated requests otherwise (60 req/hr is fine for a browse-only
//! MVP — never hard-require a token).
//!
//! Kept intentionally separate from `axon_bridge.rs`: that module proxies the
//! user's own Axon server (arbitrary configured base URL, allow-listed
//! `/v1/*` routes); this one always targets the fixed `https://api.github.com`
//! origin, so the two validators and allow-lists must not be conflated.

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};

use crate::persistence::{read_default_env_entries, value_for};

/// Percent-encode set for a single GitHub path segment: encode everything
/// `NON_ALPHANUMERIC` encodes, except `.`, `-`, `_`, and `~`, which are safe in
/// URL paths and must survive untouched (a `.` is load-bearing in file
/// extensions like `main.rs`).
const PATH_SEGMENT_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'.')
    .remove(b'-')
    .remove(b'_')
    .remove(b'~');

pub(crate) const GITHUB_API_BASE: &str = "https://api.github.com";
const GITHUB_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);
const GITHUB_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// GitHub file-contents API rejects anything much larger than this anyway
/// (returns a `too_large` payload without `content`); we additionally cap the
/// decoded preview so a huge base64 blob can't balloon renderer memory.
const MAX_FILE_PREVIEW_BYTES: usize = 2 * 1024 * 1024;

/// A shared `reqwest::Client` for GitHub REST calls, held in Tauri `AppState`.
pub(crate) struct GitHubClient(reqwest::Client);

impl GitHubClient {
    pub(crate) fn new() -> Result<Self, reqwest::Error> {
        let client = reqwest::Client::builder()
            .timeout(GITHUB_REQUEST_TIMEOUT)
            .connect_timeout(GITHUB_CONNECT_TIMEOUT)
            .user_agent(concat!("Axon Palette/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self(client))
    }

    pub(crate) fn client(&self) -> &reqwest::Client {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GitHubRequestKind {
    /// `GET /users/{owner}/repos` — repos owned by a user or org.
    ListRepos,
    /// `GET /repos/{owner}/{repo}` — repo metadata (default branch, description, …).
    RepoInfo,
    /// `GET /repos/{owner}/{repo}/git/trees/{branch}?recursive=1` — full file tree.
    Tree,
    /// `GET /repos/{owner}/{repo}/contents/{path}` — a single file (base64 content).
    FileContents,
    /// `GET /repos/{owner}/{repo}/events` — one repo's public event timeline, the
    /// building block for the cross-repo Feed. Unlike the other three variants,
    /// a single `Feed` browse request fans this out across every repo the owner
    /// has (see `github_feed.rs::fetch_feed`) rather than hitting one URL — so
    /// `build_request_url` below returns the single-repo URL shape used by that
    /// fan-out helper, not a URL `github_browse` calls directly for this kind.
    Feed,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitHubBrowseRequest {
    /// One of "repos", "repo", "tree", "file".
    pub(crate) kind: String,
    pub(crate) owner: String,
    #[serde(default)]
    pub(crate) repo: Option<String>,
    #[serde(default)]
    pub(crate) branch: Option<String>,
    #[serde(default)]
    pub(crate) path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitHubBrowseResult {
    pub(crate) ok: bool,
    pub(crate) status: u16,
    pub(crate) kind: String,
    /// Echoed request identity so the frontend can reconstruct navigation
    /// state (which owner/repo/branch/path this response belongs to) without
    /// re-deriving it from the GitHub JSON shape, which varies by `kind`.
    pub(crate) owner: String,
    pub(crate) repo: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) path: Option<String>,
    /// Raw GitHub JSON payload (array or object depending on `kind`), present
    /// only when `ok` is true.
    pub(crate) payload: serde_json::Value,
    /// Human-readable error surfaced to the palette UI when `ok` is false —
    /// rate-limit responses get a specific "retry at <time>" message.
    pub(crate) error: Option<String>,
    /// Requests remaining in the current rate-limit window, when GitHub sent
    /// the `x-ratelimit-remaining` header.
    pub(crate) rate_limit_remaining: Option<u32>,
    /// Unix timestamp (seconds) the current rate-limit window resets, when
    /// GitHub sent the `x-ratelimit-reset` header.
    pub(crate) rate_limit_reset: Option<i64>,
    /// True when the request carried a `GITHUB_TOKEN` bearer credential.
    pub(crate) authenticated: bool,
}

fn github_token() -> Option<String> {
    let entries = read_default_env_entries();
    value_for("GITHUB_TOKEN", &entries)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_kind(raw: &str) -> Result<GitHubRequestKind, String> {
    match raw {
        "repos" => Ok(GitHubRequestKind::ListRepos),
        "repo" => Ok(GitHubRequestKind::RepoInfo),
        "tree" => Ok(GitHubRequestKind::Tree),
        "file" => Ok(GitHubRequestKind::FileContents),
        "feed" => Ok(GitHubRequestKind::Feed),
        other => Err(format!("unknown GitHub browse kind: {other}")),
    }
}

/// Validate a single path segment used to build a GitHub API URL: non-empty,
/// no path traversal, no scheme/host injection, no control characters. This
/// guards `owner`/`repo`/`branch` — free-form file `path` values are validated
/// separately by `validate_file_path` since they legitimately contain `/`.
pub(crate) fn validate_segment<'a>(value: &'a str, field: &str) -> Result<&'a str, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    if trimmed != value
        || trimmed.contains('/')
        || trimmed.contains("..")
        || trimmed.contains(['\\', '?', '#', '@', ':'])
        || trimmed.chars().any(char::is_control)
    {
        return Err(format!("{field} contains invalid characters"));
    }
    Ok(trimmed)
}

/// Validate a repo-relative file/tree path: no leading slash, no `..`
/// traversal segments, no control characters. Internal slashes are allowed
/// (it's a path).
fn validate_file_path(value: &str) -> Result<&str, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("path must not be empty".to_string());
    }
    if trimmed.starts_with('/')
        || trimmed.contains(['\\', '?', '#'])
        || trimmed.chars().any(char::is_control)
        || trimmed
            .split('/')
            .any(|segment| segment == "." || segment == "..")
    {
        return Err("path contains invalid characters".to_string());
    }
    Ok(trimmed)
}

pub(crate) fn build_request_url(
    request: &GitHubBrowseRequest,
    kind: GitHubRequestKind,
) -> Result<String, String> {
    let owner = validate_segment(&request.owner, "owner")?;
    match kind {
        GitHubRequestKind::ListRepos => Ok(format!(
            "{GITHUB_API_BASE}/users/{owner}/repos?sort=updated&per_page=50"
        )),
        GitHubRequestKind::RepoInfo => {
            let repo = validate_segment(request.repo.as_deref().unwrap_or_default(), "repo")?;
            Ok(format!("{GITHUB_API_BASE}/repos/{owner}/{repo}"))
        }
        GitHubRequestKind::Tree => {
            let repo = validate_segment(request.repo.as_deref().unwrap_or_default(), "repo")?;
            let branch = validate_segment(request.branch.as_deref().unwrap_or("main"), "branch")?;
            let encoded_branch =
                percent_encoding::utf8_percent_encode(branch, PATH_SEGMENT_ENCODE_SET);
            Ok(format!(
                "{GITHUB_API_BASE}/repos/{owner}/{repo}/git/trees/{encoded_branch}?recursive=1"
            ))
        }
        GitHubRequestKind::FileContents => {
            let repo = validate_segment(request.repo.as_deref().unwrap_or_default(), "repo")?;
            let path = validate_file_path(request.path.as_deref().unwrap_or_default())?;
            let encoded_path = path
                .split('/')
                .map(|segment| {
                    percent_encoding::utf8_percent_encode(segment, PATH_SEGMENT_ENCODE_SET)
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("/");
            let mut url = format!("{GITHUB_API_BASE}/repos/{owner}/{repo}/contents/{encoded_path}");
            if let Some(branch) = request.branch.as_deref().filter(|b| !b.trim().is_empty()) {
                let branch = validate_segment(branch, "branch")?;
                let encoded_branch =
                    percent_encoding::utf8_percent_encode(branch, PATH_SEGMENT_ENCODE_SET);
                url = format!("{url}?ref={encoded_branch}");
            }
            Ok(url)
        }
        GitHubRequestKind::Feed => {
            // Unreachable in practice: `github_browse` special-cases
            // `GitHubRequestKind::Feed` and routes to
            // `github_feed::github_browse_feed` before this function is ever
            // called for that kind (see `GitHubRequestKind::Feed`'s doc
            // comment above and `github_browse` below). Kept as an explicit
            // arm — rather than a `_ =>` catch-all — so this match stays
            // exhaustive and self-documenting if `GitHubRequestKind` grows a
            // new variant later.
            unreachable!(
                "GitHubRequestKind::Feed is dispatched directly to github_feed::github_browse_feed \
                 and never reaches build_request_url"
            )
        }
    }
}

/// Result of a single unauthenticated-or-bearer GitHub GET request: the raw
/// (already status/JSON-parsed) response plus the bits every call site needs
/// to build a `GitHubBrowseResult` — status code, rate-limit headers, and the
/// parsed JSON body. Shared by `github_browse`'s main HTTP path and every
/// `github_feed.rs` call site (`github_browse_feed`'s repo-list fetch,
/// `fetch_repo_events`) so header parsing and error/JSON handling live in
/// exactly one place.
pub(crate) struct GitHubFetch {
    pub(crate) status: reqwest::StatusCode,
    pub(crate) rate_limit_remaining: Option<u32>,
    pub(crate) rate_limit_reset: Option<i64>,
    /// `Retry-After` header (seconds), when GitHub's secondary/abuse-detection
    /// rate limiter sent one — see `describe_error_with_retry_after`.
    pub(crate) retry_after_secs: Option<u64>,
    /// Parsed JSON body. `Value::Null` for an empty body; a `Value::String`
    /// fallback when the body isn't valid JSON (mirrors the previous
    /// `github_browse` inline handling).
    pub(crate) payload: serde_json::Value,
}

/// Issue a single GET against `url` with the standard GitHub REST headers,
/// optionally attaching a bearer `token`, and collect status + rate-limit
/// headers + parsed JSON body. Does not itself branch on success/failure —
/// callers use `status.is_success()` (or `describe_error`) to decide.
pub(crate) async fn fetch_github_json(
    client: &reqwest::Client,
    url: &str,
    token: Option<&str>,
) -> Result<GitHubFetch, String> {
    let mut builder = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(token) = token {
        builder = builder.bearer_auth(token);
    }

    let response = builder.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    let rate_limit_remaining = header_u32(&response, "x-ratelimit-remaining");
    let rate_limit_reset = header_i64(&response, "x-ratelimit-reset");
    let retry_after = retry_after_secs(&response);

    let text = response.text().await.map_err(|err| err.to_string())?;
    let payload: serde_json::Value = if text.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text))
    };

    Ok(GitHubFetch {
        status,
        rate_limit_remaining,
        rate_limit_reset,
        retry_after_secs: retry_after,
        payload,
    })
}

#[tauri::command]
pub(crate) async fn github_browse(
    client: tauri::State<'_, GitHubClient>,
    request: GitHubBrowseRequest,
) -> Result<GitHubBrowseResult, String> {
    let kind = parse_kind(&request.kind)?;
    let token = github_token();
    let authenticated = token.is_some();
    let owner = request.owner.clone();
    let repo = request.repo.clone();
    let branch = request.branch.clone();
    let path = request.path.clone();

    if kind == GitHubRequestKind::Feed {
        return crate::github_feed::github_browse_feed(
            &client,
            &request,
            token.as_deref(),
            authenticated,
        )
        .await;
    }

    let url = build_request_url(&request, kind)?;
    let fetch = fetch_github_json(client.client(), &url, token.as_deref()).await?;
    let GitHubFetch {
        status,
        rate_limit_remaining,
        rate_limit_reset,
        retry_after_secs: retry_after,
        payload,
    } = fetch;

    if status.is_success() {
        let payload = if kind == GitHubRequestKind::FileContents {
            truncate_file_payload(payload)
        } else {
            payload
        };
        return Ok(GitHubBrowseResult {
            ok: true,
            status: status.as_u16(),
            kind: request.kind,
            owner,
            repo,
            branch,
            path,
            payload,
            error: None,
            rate_limit_remaining,
            rate_limit_reset,
            authenticated,
        });
    }

    let error = describe_error_with_retry_after(
        status,
        rate_limit_remaining,
        rate_limit_reset,
        retry_after,
        &payload,
    );
    Ok(GitHubBrowseResult {
        ok: false,
        status: status.as_u16(),
        kind: request.kind,
        owner,
        repo,
        branch,
        path,
        payload: serde_json::Value::Null,
        error: Some(error),
        rate_limit_remaining,
        rate_limit_reset,
        authenticated,
    })
}

/// Cap the decoded preview size for large files. GitHub's contents API
/// base64-encodes `content`, so we bound on the raw field length rather than
/// decoding — good enough to prevent pathological payloads from reaching the
/// renderer while leaving normal source files untouched.
fn truncate_file_payload(mut payload: serde_json::Value) -> serde_json::Value {
    if let Some(obj) = payload.as_object_mut()
        && let Some(serde_json::Value::String(content)) = obj.get("content")
        && content.len() > MAX_FILE_PREVIEW_BYTES
    {
        obj.insert(
            "content".to_string(),
            serde_json::Value::String(String::new()),
        );
        obj.insert("truncated".to_string(), serde_json::Value::Bool(true));
    }
    payload
}

pub(crate) fn header_u32(response: &reqwest::Response, name: &str) -> Option<u32> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u32>().ok())
}

pub(crate) fn header_i64(response: &reqwest::Response, name: &str) -> Option<i64> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i64>().ok())
}

/// GitHub's secondary/abuse-detection rate limiter returns 403 with a
/// `Retry-After` header (seconds) and, critically, does NOT necessarily zero
/// out the primary `x-ratelimit-remaining` count the way the primary limiter
/// does. `retry_after_secs` reads that header when present.
pub(crate) fn retry_after_secs(response: &reqwest::Response) -> Option<u64> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
}

/// Describe a non-success GitHub API response as a human-readable error,
/// including a `retry_after` (seconds) reading of the `Retry-After` header —
/// GitHub's secondary/abuse-detection limiter returns 403 with `Retry-After`
/// set but the primary `x-ratelimit-remaining` count UNCHANGED, so without
/// this it would otherwise fall through to a generic "GitHub API error (403)"
/// message that doesn't tell the caller it's a rate limit at all, let alone
/// the secondary one (which is what the Feed tab's concurrent multi-repo
/// fan-out is most likely to trigger — see
/// `github_feed.rs::FEED_FANOUT_CHUNK_SIZE`'s doc comment).
pub(crate) fn describe_error_with_retry_after(
    status: reqwest::StatusCode,
    remaining: Option<u32>,
    reset: Option<i64>,
    retry_after: Option<u64>,
    payload: &serde_json::Value,
) -> String {
    let github_message = payload
        .get("message")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();

    if (status == reqwest::StatusCode::FORBIDDEN
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS)
        && remaining == Some(0)
    {
        if let Some(reset) = reset {
            let retry_at = format_unix_time(reset);
            return format!("GitHub API rate limited — retry at {retry_at}");
        }
        return "GitHub API rate limited — retry later".to_string();
    }
    if status == reqwest::StatusCode::FORBIDDEN
        && let Some(seconds) = retry_after
    {
        return format!("GitHub secondary rate limit hit — retry after {seconds}s");
    }
    if status == reqwest::StatusCode::NOT_FOUND {
        return "not found on GitHub".to_string();
    }
    if !github_message.is_empty() {
        return format!("GitHub API error ({status}): {github_message}");
    }
    format!("GitHub API error: {status}")
}

/// Minimal, dependency-free UTC timestamp formatter for rate-limit messages
/// (`YYYY-MM-DD HH:MM:SS UTC`). Good enough for a user-facing error string —
/// not used for anything that needs calendar correctness beyond that.
fn format_unix_time(epoch_seconds: i64) -> String {
    const SECONDS_PER_DAY: i64 = 86_400;
    let days_since_epoch = epoch_seconds.div_euclid(SECONDS_PER_DAY);
    let seconds_of_day = epoch_seconds.rem_euclid(SECONDS_PER_DAY);
    let (year, month, day) = crate::date_math::civil_from_days(days_since_epoch);
    let hour = seconds_of_day / 3600;
    let minute = (seconds_of_day % 3600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC")
}

#[cfg(test)]
#[path = "github_bridge_tests.rs"]
mod tests;
