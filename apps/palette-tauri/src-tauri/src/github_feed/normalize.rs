//! Per-event-type normalizers for `github_feed.rs`, split out to keep the
//! parent module under the monolith line cap. Declared via `#[path]` in
//! `github_feed.rs` (not `github_feed/mod.rs` — this repo never uses
//! `mod.rs`), so these items are children of `github_feed`, not siblings —
//! `super::{FeedItem, FeedBadge}` below refers to the parent module's types.

use super::{FeedBadge, FeedItem};

/// Convert one raw GitHub event JSON object into a `FeedItem`, or `None` if
/// its `type` isn't sourced by this plan (push/PR/merge/review/issue/release/
/// deps — NOT comment/conflict, see the parent module's doc comment and the
/// plan's "Mock-verified taxonomy" note in Task 3).
pub(crate) fn normalize_event(event: &serde_json::Value) -> Option<FeedItem> {
    let event_type = event.get("type")?.as_str()?;
    let actor = event
        .get("actor")
        .and_then(|a| a.get("login"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let repo = event
        .get("repo")
        .and_then(|r| r.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let created_at = event.get("created_at").and_then(|v| v.as_str())?;
    let timestamp_unix = parse_iso8601_to_unix(created_at)?;
    let payload = event
        .get("payload")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    match event_type {
        "PushEvent" => normalize_push_event(&payload, &repo, &actor, timestamp_unix),
        "PullRequestEvent" => normalize_pull_request_event(&payload, &repo, &actor, timestamp_unix),
        "PullRequestReviewEvent" => normalize_review_event(&payload, &repo, &actor, timestamp_unix),
        "IssuesEvent" => normalize_issue_event(&payload, &repo, &actor, timestamp_unix),
        "ReleaseEvent" => normalize_release_event(&payload, &repo, &actor, timestamp_unix),
        _ => None,
    }
}

fn normalize_push_event(
    payload: &serde_json::Value,
    repo: &str,
    actor: &str,
    ts: i64,
) -> Option<FeedItem> {
    let commits = payload.get("commits").and_then(|c| c.as_array());
    let commit_count = commits.map(|list| list.len()).unwrap_or(0);
    let lead_message = commits
        .and_then(|list| list.last())
        .and_then(|c| c.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("(no commit message)")
        .to_string();
    let branch_ref = payload
        .get("ref")
        .and_then(|v| v.as_str())
        .and_then(|r| r.strip_prefix("refs/heads/"))
        .unwrap_or("main");

    // "deps" (mock label "Dependencies") — NOT "dependency-bump", which does
    // not exist in the real mock's FEED_KIND taxonomy.
    let is_deps =
        actor.starts_with("dependabot") || lead_message.to_lowercase().starts_with("bump ");

    let (kind, meta) = if is_deps {
        ("deps", "1 update · dependency".to_string())
    } else {
        ("push", format!("{commit_count} commits · {branch_ref}"))
    };

    Some(FeedItem {
        kind: kind.to_string(),
        repo: repo.to_string(),
        actor: actor.to_string(),
        title: lead_message.clone(),
        url: format!("https://github.com/{repo}/commits"),
        path: extract_backtick_path(&lead_message),
        num: None,
        meta,
        // The Events API's PushEvent payload has no line-diff stats; a
        // {add, del} badge would need a separate per-commit API call. See
        // this task's "open design question" note — left None rather than
        // guessed.
        badge: None,
        timestamp_unix: ts,
    })
}

fn normalize_pull_request_event(
    payload: &serde_json::Value,
    repo: &str,
    actor: &str,
    ts: i64,
) -> Option<FeedItem> {
    let pr = payload.get("pull_request")?;
    let title = pr
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("(untitled PR)")
        .to_string();
    let url = pr
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let num = pr.get("number").and_then(|v| v.as_u64());
    let merged = pr.get("merged").and_then(|v| v.as_bool()).unwrap_or(false);
    let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let base_ref = pr
        .get("base")
        .and_then(|b| b.get("ref"))
        .and_then(|v| v.as_str())
        .unwrap_or("main");
    let head_ref = pr
        .get("head")
        .and_then(|h| h.get("ref"))
        .and_then(|v| v.as_str());

    let (kind, meta, badge) = if action == "closed" && merged {
        // The Events API's PullRequestEvent payload does not include
        // additions/deletions counts (those require a direct
        // GET /repos/{o}/{r}/pulls/{n} call) — badge is left None rather
        // than guessed. See this task's "open design question" note.
        ("merge", format!("merged into {base_ref}"), None)
    } else {
        let meta = match head_ref {
            Some(head) => format!("{action} · {base_ref} ← {head}"),
            None => format!("{action} · {base_ref}"),
        };
        ("pr", meta, None)
    };

    Some(FeedItem {
        kind: kind.to_string(),
        repo: repo.to_string(),
        actor: actor.to_string(),
        title,
        url,
        path: None,
        num,
        meta,
        badge,
        timestamp_unix: ts,
    })
}

fn normalize_review_event(
    payload: &serde_json::Value,
    repo: &str,
    actor: &str,
    ts: i64,
) -> Option<FeedItem> {
    let pr = payload.get("pull_request")?;
    let title = pr
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("(untitled PR)")
        .to_string();
    let url = pr
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let num = pr.get("number").and_then(|v| v.as_u64());
    let review_state = payload
        .get("review")
        .and_then(|r| r.get("state"))
        .and_then(|v| v.as_str())
        .unwrap_or("submitted");
    Some(FeedItem {
        kind: "review".to_string(),
        repo: repo.to_string(),
        actor: actor.to_string(),
        title,
        url,
        path: None,
        num,
        meta: review_state.to_string(),
        // No reliable single-call source for a review's file count/badge
        // from the Events API — see this task's "open design question" note.
        badge: None,
        timestamp_unix: ts,
    })
}

fn normalize_issue_event(
    payload: &serde_json::Value,
    repo: &str,
    actor: &str,
    ts: i64,
) -> Option<FeedItem> {
    let issue = payload.get("issue")?;
    let title = issue
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("(untitled issue)")
        .to_string();
    let url = issue
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let num = issue.get("number").and_then(|v| v.as_u64());
    let action = payload
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("opened");
    let first_label = issue
        .get("labels")
        .and_then(|l| l.as_array())
        .and_then(|list| list.first())
        .and_then(|l| l.get("name"))
        .and_then(|v| v.as_str());
    let meta = match first_label {
        Some(label) => format!("{action} · {label}"),
        None => action.to_string(),
    };
    let badge = (action == "closed").then(|| FeedBadge::Label {
        value: "Closed".to_string(),
    });

    Some(FeedItem {
        kind: "issue".to_string(),
        repo: repo.to_string(),
        actor: actor.to_string(),
        title,
        url,
        path: None,
        num,
        meta,
        badge,
        timestamp_unix: ts,
    })
}

fn normalize_release_event(
    payload: &serde_json::Value,
    repo: &str,
    actor: &str,
    ts: i64,
) -> Option<FeedItem> {
    let release = payload.get("release")?;
    let title = release
        .get("name")
        .and_then(|v| v.as_str())
        .or_else(|| release.get("tag_name").and_then(|v| v.as_str()))
        .unwrap_or("(untitled release)")
        .to_string();
    let url = release
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let tag_name = release
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    // The Events API's ReleaseEvent payload has no "is this the latest
    // release" flag inline — that requires a separate
    // GET /repos/{o}/{r}/releases/latest call. Left None rather than guessed;
    // see this task's "open design question" note.
    Some(FeedItem {
        kind: "release".to_string(),
        repo: repo.to_string(),
        actor: actor.to_string(),
        title,
        url,
        path: None,
        num: None,
        meta: format!("tagged {tag_name}"),
        badge: None,
        timestamp_unix: ts,
    })
}

/// Extract the first backtick-quoted token from a commit message, e.g.
/// `` fix: tighten SSRF validation in `src/core/http/ssrf.rs` `` → `Some("src/core/http/ssrf.rs")`.
/// Returns `None` when no backtick-quoted span exists — see the parent
/// module's "Known limitation."
fn extract_backtick_path(message: &str) -> Option<String> {
    let start = message.find('`')? + 1;
    let rest = &message[start..];
    let end = rest.find('`')?;
    let candidate = &rest[..end];
    // Cheap sanity filter: only treat it as a path if it looks path-shaped
    // (contains a '/' or a recognizable extension) — avoids false positives
    // like `` `cargo test` `` being mistaken for a file path.
    let looks_path_shaped = candidate.contains('/') || candidate.contains('.');
    looks_path_shaped.then(|| candidate.to_string())
}

/// Parse an RFC 3339 / ISO 8601 UTC timestamp (`2024-01-15T10:00:00Z`, the
/// exact shape GitHub's API sends) into Unix seconds, without pulling in a
/// chrono dependency — uses the shared `crate::date_math::days_from_civil`
/// (the exact inverse of `civil_from_days`, which `github_bridge.rs` uses for
/// its rate-limit "retry at" formatting).
fn parse_iso8601_to_unix(input: &str) -> Option<i64> {
    let bytes = input.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let year: i64 = input.get(0..4)?.parse().ok()?;
    let month: i64 = input.get(5..7)?.parse().ok()?;
    let day: i64 = input.get(8..10)?.parse().ok()?;
    let hour: i64 = input.get(11..13)?.parse().ok()?;
    let minute: i64 = input.get(14..16)?.parse().ok()?;
    let second: i64 = input.get(17..19)?.parse().ok()?;

    let days = crate::date_math::days_from_civil(year, month, day);
    Some(days * 86_400 + hour * 3600 + minute * 60 + second)
}
