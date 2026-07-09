use super::*;

fn sample_push_event() -> serde_json::Value {
    serde_json::json!({
        "id": "1",
        "type": "PushEvent",
        "actor": { "login": "jmagar" },
        "repo": { "name": "jmagar/axon" },
        "created_at": "2024-01-15T10:00:00Z",
        "payload": {
            "commits": [
                { "message": "fix: tighten SSRF validation in `src/core/http/ssrf.rs`" }
            ]
        }
    })
}

fn sample_dependabot_push_event() -> serde_json::Value {
    serde_json::json!({
        "id": "2",
        "type": "PushEvent",
        "actor": { "login": "dependabot[bot]" },
        "repo": { "name": "jmagar/axon" },
        "created_at": "2024-01-15T09:00:00Z",
        "payload": {
            "commits": [
                { "message": "Bump serde from 1.0.190 to 1.0.195" }
            ]
        }
    })
}

fn sample_merged_pr_event() -> serde_json::Value {
    serde_json::json!({
        "id": "3",
        "type": "PullRequestEvent",
        "actor": { "login": "jmagar" },
        "repo": { "name": "jmagar/axon" },
        "created_at": "2024-01-15T08:00:00Z",
        "payload": {
            "action": "closed",
            "pull_request": { "number": 42, "title": "Add feed view", "html_url": "https://github.com/jmagar/axon/pull/42", "merged": true, "base": { "ref": "main" } }
        }
    })
}

fn sample_opened_pr_event() -> serde_json::Value {
    serde_json::json!({
        "id": "4",
        "type": "PullRequestEvent",
        "actor": { "login": "jmagar" },
        "repo": { "name": "jmagar/axon" },
        "created_at": "2024-01-15T07:00:00Z",
        "payload": {
            "action": "opened",
            "pull_request": { "number": 43, "title": "WIP: feed", "html_url": "https://github.com/jmagar/axon/pull/43", "merged": false, "base": { "ref": "main" }, "head": { "ref": "feat/feed" } }
        }
    })
}

fn sample_unhandled_event() -> serde_json::Value {
    serde_json::json!({
        "id": "5",
        "type": "WatchEvent",
        "actor": { "login": "someone" },
        "repo": { "name": "jmagar/axon" },
        "created_at": "2024-01-15T06:00:00Z",
        "payload": {}
    })
}

#[test]
fn normalizes_plain_push_event_and_extracts_backtick_path() {
    let item = normalize_event(&sample_push_event()).expect("should normalize");
    assert_eq!(item.kind, "push");
    assert_eq!(item.repo, "jmagar/axon");
    assert_eq!(item.actor, "jmagar");
    assert_eq!(item.path.as_deref(), Some("src/core/http/ssrf.rs"));
    assert_eq!(item.num, None);
}

#[test]
fn reclassifies_dependabot_push_as_deps() {
    let item = normalize_event(&sample_dependabot_push_event()).expect("should normalize");
    // "deps" (mock label "Dependencies") — NOT "dependency-bump"; that kind
    // name does not exist in the real mock's FEED_KIND taxonomy.
    assert_eq!(item.kind, "deps");
}

#[test]
fn merged_pull_request_event_is_classified_as_merge() {
    let item = normalize_event(&sample_merged_pr_event()).expect("should normalize");
    assert_eq!(item.kind, "merge");
    assert_eq!(item.title, "Add feed view");
    assert_eq!(item.num, Some(42));
}

#[test]
fn opened_pull_request_event_is_classified_as_pr() {
    let item = normalize_event(&sample_opened_pr_event()).expect("should normalize");
    assert_eq!(item.kind, "pr");
    assert_eq!(item.num, Some(43));
}

#[test]
fn unhandled_event_types_are_skipped() {
    assert!(normalize_event(&sample_unhandled_event()).is_none());
}

#[test]
fn merge_feed_items_sorts_by_timestamp_descending() {
    let items = vec![
        FeedItem {
            kind: "push".into(),
            repo: "a".into(),
            actor: "x".into(),
            title: "older".into(),
            url: "".into(),
            path: None,
            num: None,
            meta: "".into(),
            badge: None,
            timestamp_unix: 100,
        },
        FeedItem {
            kind: "push".into(),
            repo: "a".into(),
            actor: "x".into(),
            title: "newer".into(),
            url: "".into(),
            path: None,
            num: None,
            meta: "".into(),
            badge: None,
            timestamp_unix: 200,
        },
    ];
    let sorted = sort_feed_items_desc(items);
    assert_eq!(sorted[0].title, "newer");
    assert_eq!(sorted[1].title, "older");
}

#[test]
fn caps_repo_fanout_at_ten() {
    let repos: Vec<String> = (0..25).map(|i| format!("repo-{i}")).collect();
    let capped = cap_repos_for_feed(&repos);
    assert_eq!(capped.len(), 10);
    assert_eq!(capped[0], "repo-0");
}

#[test]
fn feed_payload_serializes_items_and_partial_flag() {
    let items = vec![FeedItem {
        kind: "push".to_string(),
        repo: "jmagar/axon".to_string(),
        actor: "jmagar".to_string(),
        title: "fix: bug".to_string(),
        url: "https://github.com/jmagar/axon/commits".to_string(),
        path: Some("src/main.rs".to_string()),
        num: None,
        meta: "1 commits · main".to_string(),
        badge: None,
        timestamp_unix: 1_700_000_000,
    }];
    let fetch_result = FeedFetchResult {
        items,
        rate_limit_remaining: Some(55),
        rate_limit_reset: Some(1_700_003_600),
        partial: false,
        errors: vec![],
    };
    let payload = build_feed_payload(fetch_result);
    assert_eq!(payload["items"][0]["repo"], "jmagar/axon");
    assert_eq!(payload["partial"], false);
}
