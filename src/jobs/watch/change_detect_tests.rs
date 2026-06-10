use super::*;
use crate::services::types::{DiffResult, DiffStatus, LinkEntry, ScrapeResult};
use std::sync::{Arc, Mutex};

fn diff(status: DiffStatus, links: usize, word_delta: i64) -> DiffResult {
    let text_diff = if matches!(status, DiffStatus::Changed) {
        Some("d".into())
    } else {
        None
    };
    DiffResult {
        url_a: "a".into(),
        url_b: "b".into(),
        status,
        text_diff,
        metadata_changes: vec![],
        links_added: (0..links)
            .map(|i| LinkEntry {
                href: format!("h{i}"),
                text: "".into(),
            })
            .collect(),
        links_removed: vec![],
        word_count_delta: word_delta,
    }
}

#[test]
fn same_is_not_meaningful() {
    assert!(!is_meaningful(&diff(DiffStatus::Same, 0, 0), 0));
}
#[test]
fn any_text_change_meaningful_at_threshold_zero() {
    assert!(is_meaningful(&diff(DiffStatus::Changed, 0, 1), 0));
}
#[test]
fn sub_threshold_text_change_not_meaningful() {
    assert!(!is_meaningful(&diff(DiffStatus::Changed, 0, 2), 5));
}
#[test]
fn link_change_always_meaningful() {
    assert!(is_meaningful(&diff(DiffStatus::Changed, 1, 0), 100));
}

#[test]
fn snapshot_hash_detects_link_only_change() {
    // Identical visible markdown but a different links snapshot must produce a
    // different hash, so the fast-equal shortcut does not skip a link-only
    // change before compute_diff can apply the "links always count" rule.
    let md = "same visible markdown";
    let links_a = r#"[{"href":"https://a.example/x","text":""}]"#;
    let links_b = r#"[{"href":"https://a.example/y","text":""}]"#;
    assert_ne!(snapshot_hash(md, links_a), snapshot_hash(md, links_b));
    // Sanity: stable under identical inputs.
    assert_eq!(snapshot_hash(md, links_a), snapshot_hash(md, links_a));
}

// ── Offline integration tests (stub fetcher, in-memory SQLite) ──────────────
//
// These tests exercise the four key branches of `detect_url_change_with`:
//   1. 304 short-circuit → Unchanged (no scrape called)
//   2. Probe failure fallback → scrape also fails → Failed
//   3. Fast-equal hash skip → Unchanged (no diff computed)
//   4. First-seen → Changed (seed path, no prior snapshot)
//
// Each test spins up an isolated in-memory SQLite pool so there are no
// cross-test ordering dependencies and no filesystem state.

/// Stub `WatchFetcher` that returns pre-configured canned responses.
///
/// The probe result is `Clone` so it can be returned directly.
/// The scrape result is wrapped in `Arc<Mutex<Option<...>>>` so ownership can
/// transfer out of the (shared-ref) trait method without requiring `Clone`.
struct StubFetcher {
    probe_result: Probe,
    scrape_result: Arc<Mutex<Option<Result<ScrapeResult, String>>>>,
}

impl StubFetcher {
    /// Probe-only stub; calling `scrape_url` returns an error.
    fn probe_only(probe_result: Probe) -> Self {
        Self {
            probe_result,
            scrape_result: Arc::new(Mutex::new(None)),
        }
    }

    /// Full stub with both a probe and a scrape response.
    fn new(probe_result: Probe, scrape: Result<ScrapeResult, String>) -> Self {
        Self {
            probe_result,
            scrape_result: Arc::new(Mutex::new(Some(scrape))),
        }
    }
}

#[async_trait::async_trait]
impl WatchFetcher for StubFetcher {
    async fn probe(&self, _url: &str, _etag: Option<&str>, _lm: Option<&str>) -> Probe {
        self.probe_result.clone()
    }

    async fn scrape_url(&self, _cfg: &Config, _url: &str) -> Result<ScrapeResult, String> {
        self.scrape_result
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Err("stub: scrape not configured".into()))
    }
}

/// Build a minimal `ScrapeResult` for a given URL and markdown body.
fn make_scrape_result(url: &str, markdown: &str) -> ScrapeResult {
    crate::services::scrape::map_scrape_payload(
        serde_json::json!({ "url": url, "markdown": markdown, "links": [] }),
    )
    .expect("map_scrape_payload should succeed for well-formed input")
}

/// Compute the content hash that `detect_url_change_with` will derive from a
/// given markdown string (no ignore patterns, no links in the payload).
/// Used to pre-seed the DB so the hash-equal skip fires as expected.
fn expected_hash_for(markdown: &str) -> String {
    let filtered = apply_ignore(&normalize_markdown(markdown), &[]);
    // A payload with no "links" key yields an empty Vec from
    // `extract_links_from_payload`, serialised as "[]".
    snapshot_hash(&filtered, "[]")
}

/// Create a minimal in-memory SQLite pool with just the `axon_watch_url_state`
/// table. The FK to `axon_watch_defs` is omitted here; SQLite does not enforce
/// foreign keys by default so the unit tests work without the parent table.
async fn make_test_pool() -> SqlitePool {
    let pool = SqlitePool::connect(":memory:")
        .await
        .expect("in-memory SQLite pool");
    sqlx::query(
        "CREATE TABLE axon_watch_url_state (
            watch_id          TEXT NOT NULL,
            url               TEXT NOT NULL,
            etag              TEXT,
            last_modified     TEXT,
            content_hash      TEXT,
            last_markdown     TEXT,
            last_links_json   TEXT,
            last_checked_at   INTEGER,
            last_changed_at   INTEGER,
            last_crawl_job_id TEXT,
            PRIMARY KEY (watch_id, url)
        )",
    )
    .execute(&pool)
    .await
    .expect("create axon_watch_url_state");
    pool
}

// ── Test 1: 304 short-circuit → Unchanged ───────────────────────────────────

#[tokio::test]
async fn probe_not_modified_returns_unchanged_without_scrape() {
    let pool = make_test_pool().await;
    let cfg = Config::default_minimal();
    let watch_id = Uuid::new_v4();
    let url = "https://example.test/page";

    // Probe says 304 — scrape should never be called.
    let fetcher = StubFetcher::probe_only(Probe::NotModified);
    let outcome = detect_url_change_with(&fetcher, &cfg, &pool, watch_id, url, &[], 0).await;

    assert!(
        matches!(outcome, UrlOutcome::Unchanged),
        "expected Unchanged for 304; got {outcome:?}"
    );
    // The 304 path calls `persist_unchanged`, which must stamp `last_checked_at`
    // on the URL state row even when there is no prior snapshot.
    let state = get_url_state(&pool, watch_id, url)
        .await
        .expect("get_url_state should not error");
    assert!(
        state.is_some(),
        "NotModified path should have written a url-state row"
    );
    assert!(
        state.unwrap().last_checked_at.is_some(),
        "NotModified path must stamp last_checked_at"
    );
}

// ── Test 2: probe fails + scrape fails → Failed ──────────────────────────────

#[tokio::test]
async fn probe_failure_then_scrape_failure_returns_failed() {
    let pool = make_test_pool().await;
    let cfg = Config::default_minimal();
    let watch_id = Uuid::new_v4();
    let url = "https://example.test/page";

    // Probe fails; scrape also fails.
    let fetcher = StubFetcher::new(
        Probe::Failed("timeout".into()),
        Err("scrape failed: connection refused".into()),
    );
    let outcome = detect_url_change_with(&fetcher, &cfg, &pool, watch_id, url, &[], 0).await;

    assert!(
        matches!(outcome, UrlOutcome::Failed { .. }),
        "expected Failed when both probe and scrape fail; got {outcome:?}"
    );
}

// ── Test 3: hash-equal skip → Unchanged ─────────────────────────────────────

#[tokio::test]
async fn hash_equal_prior_returns_unchanged() {
    let pool = make_test_pool().await;
    let cfg = Config::default_minimal();
    let watch_id = Uuid::new_v4();
    let url = "https://example.test/page";
    let markdown = "# Stable\n\nThis page has not changed.";

    // Seed the DB with the hash that the scrape will produce, simulating a
    // previously indexed snapshot that matches the fresh content exactly.
    let prior_hash = expected_hash_for(markdown);
    upsert_url_state(
        &pool,
        watch_id,
        url,
        &UrlState {
            content_hash: Some(prior_hash),
            last_markdown: Some(markdown.to_string()),
            last_links_json: Some("[]".into()),
            last_checked_at: Some(1_000_000),
            last_changed_at: Some(1_000_000),
            ..Default::default()
        },
    )
    .await
    .expect("seed prior state");

    // Probe signals modified; scrape returns the same content as the snapshot.
    let fetcher = StubFetcher::new(
        Probe::Modified {
            etag: None,
            last_modified: None,
        },
        Ok(make_scrape_result(url, markdown)),
    );
    let outcome = detect_url_change_with(&fetcher, &cfg, &pool, watch_id, url, &[], 0).await;

    assert!(
        matches!(outcome, UrlOutcome::Unchanged),
        "expected Unchanged when hashes match; got {outcome:?}"
    );
}

// ── Test 4: first-seen (no prior) → Changed ──────────────────────────────────

#[tokio::test]
async fn first_seen_url_returns_changed() {
    let pool = make_test_pool().await;
    let cfg = Config::default_minimal();
    let watch_id = Uuid::new_v4();
    let url = "https://example.test/new-page";
    let markdown = "# New content\n\nThis is the first time we have seen this page.";

    // No prior snapshot exists; probe signals modified; scrape succeeds.
    let fetcher = StubFetcher::new(
        Probe::Modified {
            etag: Some("\"v1\"".into()),
            last_modified: None,
        },
        Ok(make_scrape_result(url, markdown)),
    );
    let outcome = detect_url_change_with(&fetcher, &cfg, &pool, watch_id, url, &[], 0).await;

    assert!(
        matches!(outcome, UrlOutcome::Changed { .. }),
        "expected Changed on first-seen URL; got {outcome:?}"
    );
}
