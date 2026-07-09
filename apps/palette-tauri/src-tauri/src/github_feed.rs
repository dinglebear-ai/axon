//! Cross-repo activity Feed: fans `GET /repos/{owner}/{repo}/events` out across
//! a bounded set of repos, normalizes each event into a `FeedItem`, merges and
//! sorts by recency. Called from `github_bridge.rs`'s `Feed` branch, which
//! resolves the repo list (via `ListRepos`) before calling `fetch_feed` here.
//!
//! Data source: the GitHub Events API, not the Notifications API — see the
//! "Data source decision" section of `docs/plans/palette-github-enhancements.md`
//! for the full comparison. Short version: Events API scopes to "activity on
//! repos I'm already browsing" (matches how every other `github` browse kind
//! works), works unauthenticated, and needs no extra OAuth scope beyond the
//! `GITHUB_TOKEN` PAT the palette already reads.
//!
//! Known limitation: the Events API's `PushEvent` payload does not include
//! per-commit changed-file lists, so `FeedItem::path` is extracted heuristically
//! from the first backtick-quoted token in the lead commit message (a common
//! but not universal commit-message convention). When no backtick token is
//! found, `path` is `None` and clicking the feed item opens the repo's file
//! tree unscoped rather than jumping to a specific file — a graceful degrade,
//! not an error.

use serde::Serialize;

use crate::github_bridge::GITHUB_API_BASE;

#[path = "github_feed/normalize.rs"]
mod normalize;

pub(crate) use normalize::normalize_event;

/// Repos beyond this count (by the caller's ordering — `ListRepos` already
/// sorts by `updated`, so this means "10 most recently updated repos") are not
/// included in the fan-out. Bounds worst-case unauthenticated rate-limit burn
/// to 10 of the 60 req/hr budget, and keeps authenticated latency reasonable.
const MAX_FEED_REPOS: usize = 10;

/// Repos are fetched in chunks of this size (not all-at-once) to avoid
/// GitHub's secondary/abuse-detection rate limiting, which throttles bursts of
/// concurrent requests independent of the primary `x-ratelimit-*` budget.
const FEED_FANOUT_CHUNK_SIZE: usize = 3;

const MAX_ITEMS_PER_REPO: usize = 30;
const MAX_TOTAL_FEED_ITEMS: usize = 100;

/// One of `"pr"`, `"merge"`, `"review"`, `"comment"`, `"conflict"`, `"deps"`,
/// `"issue"`, `"push"`, `"release"` — the real mock's `FEED_KIND` taxonomy
/// (verified against `palette-mock.html`'s `var FEED_KIND = {...}` object;
/// mock labels are "Pull Request"/"Merged"/"Review"/"Comment"/"Conflict"/
/// "Dependencies"/"Issue"/"Push"/"Release" respectively). `normalize_event`
/// below never emits `"comment"` or `"conflict"` — this plan's Events API
/// source doesn't cover them (see this task's "Mock-verified taxonomy" note
/// in the plan doc); Task 5's TS-side label/icon maps still register both
/// kinds so the taxonomy stays forward-compatible.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FeedItem {
    pub(crate) kind: String,
    /// `owner/repo`.
    pub(crate) repo: String,
    /// GitHub login of the actor who triggered the event.
    pub(crate) actor: String,
    /// Human-readable title (PR/issue title, or the push's lead commit message).
    pub(crate) title: String,
    /// Best-effort link to view the event on github.com (PR/issue HTML URL, or
    /// the repo's commits page for pushes).
    pub(crate) url: String,
    /// Best-effort file path this event touched, when extractable — see the
    /// module doc's "Known limitation" for how/when this is populated.
    pub(crate) path: Option<String>,
    /// PR/issue number, when the event names one (`None` for pushes/releases,
    /// matching the mock's `num:null` on those rows).
    pub(crate) num: Option<u64>,
    /// Short freeform descriptive line (mock examples: `"opened · main ←
    /// feat/research"`, `"1 update · security advisory"`, `"tagged v5.19.0 ·
    /// 41 commits"`). Populated per event type in the `normalize_*` functions
    /// below — see this task's "open design question" note on how reliably
    /// the real Events API can back this field per kind.
    pub(crate) meta: String,
    /// Either a `{add, del}` line-diff or a short status label (mock
    /// examples: `"Approved"`, `"Closed"`, `"Bug"`, `"Latest"`, `"Patch"`).
    /// `None` when this event type has no reliable single-call source (see
    /// this task's "open design question" note).
    pub(crate) badge: Option<FeedBadge>,
    /// Unix seconds, parsed from the event's `created_at`.
    pub(crate) timestamp_unix: i64,
}

/// Mirrors the mock's `feedBadge()` renderer for the status-label case (the
/// mock's other case — a `{add, del}` line-diff badge — is never constructed
/// by `normalize_event`: the Events API doesn't expose a single-call
/// additions/deletions source for any event type this plan sources, see
/// Task 3's "open design question" note in
/// docs/plans/palette-github-enhancements.md — so that variant was removed
/// rather than kept as dead, unconstructed code. If a future normalizer gains
/// a reliable diff-stat source, reintroduce a tagged enum with a `Diff`
/// variant at that point).
///
/// Struct-shaped (`{ value: String }`, not a newtype `Label(String)`) because
/// serde's internally-tagged representation (`#[serde(tag = "type")]`) cannot
/// serialize a newtype variant wrapping a primitive (it panics at runtime
/// with "cannot serialize tagged newtype variant ... containing a string").
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "type")]
pub(crate) enum FeedBadge {
    Label { value: String },
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FeedFetchResult {
    pub(crate) items: Vec<FeedItem>,
    pub(crate) rate_limit_remaining: Option<u32>,
    pub(crate) rate_limit_reset: Option<i64>,
    /// True when at least one repo's events call failed (e.g. individually
    /// rate-limited) and was dropped rather than failing the whole feed.
    pub(crate) partial: bool,
    /// Human-readable per-repo error messages, present only when `partial`.
    pub(crate) errors: Vec<String>,
}

/// Cap and return the leading `MAX_FEED_REPOS` entries of `repos`, preserving
/// order (callers pass already-sorted-by-recency repo names from `ListRepos`).
pub(crate) fn cap_repos_for_feed(repos: &[String]) -> Vec<String> {
    repos.iter().take(MAX_FEED_REPOS).cloned().collect()
}

/// Fetch and merge events across `repos` (already capped by the caller via
/// `cap_repos_for_feed` — this function does not re-cap, so tests can exercise
/// arbitrary repo counts directly).
pub(crate) async fn fetch_feed(
    client: &reqwest::Client,
    owner: &str,
    repos: &[String],
    token: Option<&str>,
) -> FeedFetchResult {
    let mut all_items = Vec::new();
    let mut errors = Vec::new();
    let mut last_remaining = None;
    let mut last_reset = None;

    for chunk in repos.chunks(FEED_FANOUT_CHUNK_SIZE) {
        let futures = chunk
            .iter()
            .map(|repo| fetch_repo_events(client, owner, repo, token));
        let results = futures::future::join_all(futures).await;
        for (repo, result) in chunk.iter().zip(results) {
            match result {
                Ok((items, remaining, reset)) => {
                    all_items.extend(items);
                    last_remaining = remaining.or(last_remaining);
                    last_reset = reset.or(last_reset);
                }
                Err(message) => {
                    errors.push(format!("{owner}/{repo}: {message}"));
                }
            }
        }
    }

    let mut items = sort_feed_items_desc(all_items);
    items.truncate(MAX_TOTAL_FEED_ITEMS);

    FeedFetchResult {
        items,
        rate_limit_remaining: last_remaining,
        rate_limit_reset: last_reset,
        partial: !errors.is_empty(),
        errors,
    }
}

async fn fetch_repo_events(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    token: Option<&str>,
) -> Result<(Vec<FeedItem>, Option<u32>, Option<i64>), String> {
    let url =
        format!("{GITHUB_API_BASE}/repos/{owner}/{repo}/events?per_page={MAX_ITEMS_PER_REPO}");
    let fetch = crate::github_bridge::fetch_github_json(client, &url, token).await?;

    if !fetch.status.is_success() {
        // Give the per-repo error a distinct message when this looks like
        // GitHub's secondary/abuse-detection limiter (`Retry-After` set) —
        // the Feed fan-out's concurrent multi-repo bursts are exactly what
        // triggers it (see `FEED_FANOUT_CHUNK_SIZE`'s doc comment above).
        if fetch.status == reqwest::StatusCode::FORBIDDEN
            && let Some(seconds) = fetch.retry_after_secs
        {
            return Err(format!(
                "GitHub secondary rate limit hit — retry after {seconds}s"
            ));
        }
        return Err(format!("HTTP {}", fetch.status));
    }

    let events = fetch.payload.as_array().cloned().unwrap_or_default();
    let items = events.iter().filter_map(normalize_event).collect();
    Ok((items, fetch.rate_limit_remaining, fetch.rate_limit_reset))
}

pub(crate) fn sort_feed_items_desc(mut items: Vec<FeedItem>) -> Vec<FeedItem> {
    items.sort_by_key(|item| std::cmp::Reverse(item.timestamp_unix));
    items
}

pub(crate) fn build_feed_payload(fetch_result: FeedFetchResult) -> serde_json::Value {
    serde_json::json!({
        "items": fetch_result.items,
        "partial": fetch_result.partial,
        "errors": fetch_result.errors,
    })
}

/// `Feed` branch of `github_browse`: resolves the repo list for `request.owner`
/// (reusing the `ListRepos` URL/shape) unless the caller already supplied one
/// via `request.repo` as a comma-separated list (not currently exercised by the
/// frontend — see Task 7 — but supported here so a future caller can skip the
/// extra `ListRepos` round trip when it already knows the repos), then fans
/// events out across up to `MAX_FEED_REPOS` of them via `fetch_feed`.
///
/// Moved here (from `github_bridge.rs`) in Task 9's verification pass to keep
/// `github_bridge.rs` under the 500-line monolith cap — this is Feed-specific
/// dispatch logic, not shared with the other four `GitHubRequestKind`s.
pub(crate) async fn github_browse_feed(
    client: &crate::github_bridge::GitHubClient,
    request: &crate::github_bridge::GitHubBrowseRequest,
    token: Option<&str>,
    authenticated: bool,
) -> Result<crate::github_bridge::GitHubBrowseResult, String> {
    use crate::github_bridge::{
        GitHubBrowseResult, GitHubRequestKind, build_request_url, describe_error_with_retry_after,
        fetch_github_json, validate_segment,
    };

    let owner = validate_segment(&request.owner, "owner")?.to_string();

    let repos: Vec<String> =
        if let Some(explicit) = request.repo.as_deref().filter(|r| !r.trim().is_empty()) {
            explicit
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .map(|s| validate_segment(&s, "repo").map(str::to_string))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            let list_url = build_request_url(request, GitHubRequestKind::ListRepos)?;
            let fetch = fetch_github_json(client.client(), &list_url, token).await?;
            if !fetch.status.is_success() {
                let error = describe_error_with_retry_after(
                    fetch.status,
                    fetch.rate_limit_remaining,
                    fetch.rate_limit_reset,
                    fetch.retry_after_secs,
                    &fetch.payload,
                );
                return Ok(GitHubBrowseResult {
                    ok: false,
                    status: fetch.status.as_u16(),
                    kind: "feed".to_string(),
                    owner,
                    repo: None,
                    branch: None,
                    path: None,
                    payload: serde_json::Value::Null,
                    error: Some(error),
                    rate_limit_remaining: fetch.rate_limit_remaining,
                    rate_limit_reset: fetch.rate_limit_reset,
                    authenticated,
                });
            }
            fetch
                .payload
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|r| r.get("name").and_then(|n| n.as_str()).map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };

    let capped = cap_repos_for_feed(&repos);
    let fetch_result = fetch_feed(client.client(), &owner, &capped, token).await;
    let rate_limit_remaining = fetch_result.rate_limit_remaining;
    let rate_limit_reset = fetch_result.rate_limit_reset;
    let payload = build_feed_payload(fetch_result);

    Ok(GitHubBrowseResult {
        ok: true,
        status: 200,
        kind: "feed".to_string(),
        owner,
        repo: None,
        branch: None,
        path: None,
        payload,
        error: None,
        rate_limit_remaining,
        rate_limit_reset,
        authenticated,
    })
}

#[cfg(test)]
#[path = "github_feed_tests.rs"]
mod tests;
