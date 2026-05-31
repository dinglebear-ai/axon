# URL Change-Detection Watch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the `watch` action into a URL change detector that crawls only the changed subtrees of each watched URL, instead of unconditionally re-scraping every tick.

**Architecture:** Each scheduler tick, for every URL in a watch: a cheap conditional HTTP probe (ETag/Last-Modified) decides "definitely unchanged" (304); otherwise the page is scraped and a SHA-256 of its normalized markdown is compared to the stored hash. Changed URLs are grouped by common path prefix and one crawl job is enqueued per cluster (seeded at the common ancestor), unless that cluster's previous crawl is still in flight. Per-URL detection state lives in a new `axon_watch_url_state` table.

**Tech Stack:** Rust, SQLite (sqlx + `sqlx::migrate!`), `reqwest` 0.13, `sha2` 0.11, the existing in-process job runtime (`enqueue_job` + crawl worker), the existing scheduler loop from v4.15.0.

---

## Context the implementer needs

Read these before starting:
- `src/jobs/watch.rs` — the watch runtime. `run_watch_now_with_pool` creates a run, calls `run_watch_task`, finalizes once. `lease_due_watches` + the scheduler in `src/jobs/workers/watch_scheduler.rs` already fire due watches. `validate_task_type` / `SUPPORTED_TASK_TYPES` gate create paths. `dispatched_job_id` and `result_json` columns exist on `axon_watch_runs`.
- `src/jobs/CLAUDE.md` — module conventions, `JobStatus`, the SQLite store rules.
- `docs/superpowers/specs/2026-05-31-url-watch-change-detection-design.md` — the approved design.

Key facts (verified):
- `crate::services::scrape::scrape(cfg: &Config, url: &str, None) -> Result<ScrapeResult, Box<dyn Error>>`; `ScrapeResult.markdown: String`. ScrapeResult does **not** expose ETag/Last-Modified, which is why we add a separate probe.
- `crate::core::http::http_client() -> anyhow::Result<&'static reqwest::Client>`.
- `crate::core::http::ssrf::validate_url(url: &str) -> Result<(), HttpError>` (sync SSRF guard).
- `crate::jobs::ops::enqueue_job(pool: &SqlitePool, payload: &JobPayload, cfg: &Config) -> Result<Uuid, JobError>`.
- `crate::jobs::backend::JobPayload::Crawl { url: String, config_json: String }` (single-seed).
- `crate::jobs::config_snapshot::config_snapshot_json(cfg: &Config) -> Result<String, serde_json::Error>` (pub(crate)).
- `crate::jobs::query::job_status_row(pool, kind: JobKind, id: Uuid) -> Result<Option<JobStatusRow>, sqlx::Error>`; `JobStatusRow.status: JobStatus`; `JobStatus::is_active()` is true for Pending|Running.
- `crate::jobs::store::{now_ms, open_sqlite_pool}`; migrations run automatically on pool open via `sqlx::migrate!("src/jobs/migrations")`.
- `cfg.max_depth: usize`. `sha2 = "0.11"` is already a dependency.

Repo rules:
- **Module layout:** never `mod.rs`. `src/jobs/watch.rs` is the module root; new submodules go in `src/jobs/watch/<name>.rs` declared `mod <name>;` inside `watch.rs`.
- **Test sidecars:** tests live in `<file>_tests.rs` declared `#[cfg(test)] #[path = "<file>_tests.rs"] mod tests;`. `use super::*;` inside.
- **Monolith cap:** changed `.rs` files ≤ 500 lines (hard fail), functions ≤ 120 (hard fail). `watch.rs` is already at ~500 — keep new logic in the new submodules, keep `run_watch_task` thin.
- Run `cargo fmt` before every commit; keep `cargo clippy --lib` clean. Commit with `--no-verify` is acceptable (a watchdog reset hook fires on uncommitted changes mid-work), but the full gate must pass before the final push.

File structure created/modified by this plan:

| File | Responsibility |
|---|---|
| `src/jobs/migrations/0003_create_watch_url_state.sql` | New per-URL detection-state table |
| `src/jobs/watch/url_state.rs` (+ `_tests`) | CRUD for `axon_watch_url_state` |
| `src/jobs/watch/cluster.rs` (+ `_tests`) | Pure `group_by_common_prefix` |
| `src/jobs/watch/hash.rs` (+ `_tests`) | Pure markdown normalization + SHA-256 |
| `src/core/http/conditional.rs` (+ `_tests`) | `conditional_probe` + pure header/classify helpers |
| `src/jobs/watch/change_detect.rs` (+ `_tests`) | Per-URL probe→hash→decision orchestration |
| `src/jobs/watch/dispatch.rs` (+ `_tests`) | Crawl enqueue + in-flight guard |
| `src/jobs/watch.rs` | `run_watch_task` rewrite, `SUPPORTED_TASK_TYPES` cutover, `mod` declarations |
| `src/core/http.rs` | `mod conditional;` + re-export |
| `src/jobs/watch_tests.rs`, `src/web/server/handlers/rest_tests.rs`, parse/help fixtures | `refresh` → `watch` |
| `docs/commands/watch.md`, `CLAUDE.md`, `CHANGELOG.md`, version files | Docs + version bump |

---

## Task 1: Add the `axon_watch_url_state` migration

**Files:**
- Create: `src/jobs/migrations/0003_create_watch_url_state.sql`

- [ ] **Step 1: Write the migration**

Create `src/jobs/migrations/0003_create_watch_url_state.sql`:

```sql
CREATE TABLE IF NOT EXISTS axon_watch_url_state (
    watch_id          TEXT NOT NULL,
    url               TEXT NOT NULL,
    etag              TEXT,
    last_modified     TEXT,
    content_hash      TEXT,
    last_checked_at   INTEGER,
    last_changed_at   INTEGER,
    last_crawl_job_id TEXT,
    PRIMARY KEY (watch_id, url),
    FOREIGN KEY (watch_id) REFERENCES axon_watch_defs(id) ON DELETE CASCADE
);
```

- [ ] **Step 2: Verify migrations still compile/apply**

Run: `cargo test --lib jobs::watch::tests::sqlite_watch_create_and_list_round_trip -- --nocapture`
Expected: PASS (the test opens a pool, which runs all migrations including `0003`; a malformed migration would fail here).

- [ ] **Step 3: Commit**

```bash
git add src/jobs/migrations/0003_create_watch_url_state.sql
git commit -m "feat(watch): 0003 migration for axon_watch_url_state" --no-verify
```

---

## Task 2: Per-URL state store (`url_state.rs`)

**Files:**
- Create: `src/jobs/watch/url_state.rs`
- Create: `src/jobs/watch/url_state_tests.rs`
- Modify: `src/jobs/watch.rs` (add `mod url_state;`)

- [ ] **Step 1: Declare the submodule**

In `src/jobs/watch.rs`, directly under the existing `use` block (top of file, before the consts), add:

```rust
mod url_state;
```

- [ ] **Step 2: Write the failing test**

Create `src/jobs/watch/url_state_tests.rs`:

```rust
use super::*;
use crate::jobs::store::open_sqlite_pool;
use crate::jobs::watch::{WatchDefCreate, create_watch_def_with_pool};
use chrono::Utc;
use tempfile::NamedTempFile;
use uuid::Uuid;

#[tokio::test]
async fn upsert_then_get_round_trips() {
    let temp = NamedTempFile::new().unwrap();
    let pool = open_sqlite_pool(&temp.path().to_string_lossy())
        .await
        .unwrap();
    let watch = create_watch_def_with_pool(
        &pool,
        &WatchDefCreate {
            name: "w".into(),
            task_type: "watch".into(),
            task_payload: serde_json::json!({"urls": ["https://example.com/a"]}),
            every_seconds: 60,
            enabled: true,
            next_run_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    assert!(
        get_url_state(&pool, watch.id, "https://example.com/a")
            .await
            .unwrap()
            .is_none()
    );

    let job = Uuid::new_v4();
    let state = UrlState {
        etag: Some("\"abc\"".into()),
        last_modified: Some("Wed, 21 Oct 2025 07:28:00 GMT".into()),
        content_hash: Some("deadbeef".into()),
        last_checked_at: Some(1000),
        last_changed_at: Some(1000),
        last_crawl_job_id: Some(job),
    };
    upsert_url_state(&pool, watch.id, "https://example.com/a", &state)
        .await
        .unwrap();

    let got = get_url_state(&pool, watch.id, "https://example.com/a")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got, state);

    // Upsert again with a new hash → replaces, not duplicates.
    let mut updated = state.clone();
    updated.content_hash = Some("feedface".into());
    upsert_url_state(&pool, watch.id, "https://example.com/a", &updated)
        .await
        .unwrap();
    let got2 = get_url_state(&pool, watch.id, "https://example.com/a")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got2.content_hash.as_deref(), Some("feedface"));
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test --lib jobs::watch::url_state -- --nocapture`
Expected: FAIL to compile — `url_state.rs` does not exist yet.

- [ ] **Step 4: Write the implementation**

Create `src/jobs/watch/url_state.rs`:

```rust
//! Per-URL change-detection state (`axon_watch_url_state`).
//!
//! One row per `(watch_id, url)`. Stores the HTTP validators and content hash
//! from the last check so the next tick can short-circuit unchanged URLs, plus
//! the id of the last crawl this URL triggered (for the in-flight guard).

use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UrlState {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub content_hash: Option<String>,
    pub last_checked_at: Option<i64>,
    pub last_changed_at: Option<i64>,
    pub last_crawl_job_id: Option<Uuid>,
}

type Row = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<i64>,
    Option<String>,
);

pub async fn get_url_state(
    pool: &SqlitePool,
    watch_id: Uuid,
    url: &str,
) -> Result<Option<UrlState>, sqlx::Error> {
    let row = sqlx::query_as::<_, Row>(
        "SELECT etag, last_modified, content_hash, last_checked_at, last_changed_at, last_crawl_job_id \
         FROM axon_watch_url_state WHERE watch_id = ? AND url = ?",
    )
    .bind(watch_id.to_string())
    .bind(url)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(
        |(etag, last_modified, content_hash, last_checked_at, last_changed_at, last_crawl_job_id)| {
            UrlState {
                etag,
                last_modified,
                content_hash,
                last_checked_at,
                last_changed_at,
                last_crawl_job_id: last_crawl_job_id
                    .and_then(|raw| Uuid::parse_str(&raw).ok()),
            }
        },
    ))
}

pub async fn upsert_url_state(
    pool: &SqlitePool,
    watch_id: Uuid,
    url: &str,
    state: &UrlState,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO axon_watch_url_state \
         (watch_id, url, etag, last_modified, content_hash, last_checked_at, last_changed_at, last_crawl_job_id) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(watch_id, url) DO UPDATE SET \
           etag = excluded.etag, \
           last_modified = excluded.last_modified, \
           content_hash = excluded.content_hash, \
           last_checked_at = excluded.last_checked_at, \
           last_changed_at = excluded.last_changed_at, \
           last_crawl_job_id = excluded.last_crawl_job_id",
    )
    .bind(watch_id.to_string())
    .bind(url)
    .bind(&state.etag)
    .bind(&state.last_modified)
    .bind(&state.content_hash)
    .bind(state.last_checked_at)
    .bind(state.last_changed_at)
    .bind(state.last_crawl_job_id.map(|id| id.to_string()))
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
#[path = "url_state_tests.rs"]
mod tests;
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test --lib jobs::watch::url_state -- --nocapture`
Expected: PASS (1 test).

- [ ] **Step 6: Commit**

```bash
cargo fmt
git add src/jobs/watch.rs src/jobs/watch/url_state.rs src/jobs/watch/url_state_tests.rs
git commit -m "feat(watch): per-URL change-detection state store" --no-verify
```

---

## Task 3: Common-prefix clustering (`cluster.rs`)

**Files:**
- Create: `src/jobs/watch/cluster.rs`
- Create: `src/jobs/watch/cluster_tests.rs`
- Modify: `src/jobs/watch.rs` (add `mod cluster;`)

- [ ] **Step 1: Declare the submodule**

In `src/jobs/watch.rs`, next to `mod url_state;`, add:

```rust
mod cluster;
```

- [ ] **Step 2: Write the failing tests**

Create `src/jobs/watch/cluster_tests.rs`:

```rust
use super::*;

fn seeds(urls: &[&str]) -> Vec<String> {
    let owned: Vec<String> = urls.iter().map(|s| s.to_string()).collect();
    let mut s: Vec<String> = group_by_common_prefix(&owned)
        .into_iter()
        .map(|c| c.seed)
        .collect();
    s.sort();
    s
}

#[test]
fn same_directory_collapses_to_one_seed() {
    let c = group_by_common_prefix(&[
        "https://h/a/b/x".into(),
        "https://h/a/b/y".into(),
    ]);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].seed, "https://h/a/b/");
    assert_eq!(c[0].members.len(), 2);
}

#[test]
fn nested_dirs_seed_at_common_ancestor() {
    let c = group_by_common_prefix(&[
        "https://h/a/b/x".into(),
        "https://h/a/c/y".into(),
    ]);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].seed, "https://h/a/");
}

#[test]
fn sibling_subtrees_do_not_merge() {
    assert_eq!(
        seeds(&["https://h/a/x", "https://h/b/y"]),
        vec!["https://h/a/x".to_string(), "https://h/b/y".to_string()]
    );
}

#[test]
fn different_hosts_do_not_merge() {
    assert_eq!(
        seeds(&["https://h1/a/x", "https://h2/a/y"]),
        vec!["https://h1/a/x".to_string(), "https://h2/a/y".to_string()]
    );
}

#[test]
fn single_url_seed_is_the_url() {
    let c = group_by_common_prefix(&["https://h/a/b/c".into()]);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].seed, "https://h/a/b/c");
}

#[test]
fn root_only_urls_stay_separate() {
    assert_eq!(
        seeds(&["https://h/", "https://h2/"]),
        vec!["https://h/".to_string(), "https://h2/".to_string()]
    );
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --lib jobs::watch::cluster -- --nocapture`
Expected: FAIL to compile — `cluster.rs` does not exist.

- [ ] **Step 4: Write the implementation**

Create `src/jobs/watch/cluster.rs`:

```rust
//! Group changed URLs into crawl clusters by shared directory ancestry.
//!
//! Pure logic, no I/O. URLs that live under a common `…/segment/` subtree are
//! merged and seeded at their longest common directory prefix, so one crawl
//! covers them all (the crawl engine auto-scopes to the seed's subtree). URLs
//! sharing only the host root are NOT merged — we never seed a whole-site crawl
//! from a coincidental host match.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cluster {
    /// URL to seed the crawl at — the common-ancestor directory, or the URL
    /// itself for a single-member cluster.
    pub seed: String,
    pub members: Vec<String>,
}

/// `(scheme, host)` host key plus the directory segments of the path (the path
/// minus its trailing filename component). Returns `None` for unparseable URLs.
fn parts(url: &str) -> Option<(String, Vec<String>)> {
    let (scheme, rest) = url.split_once("://")?;
    let (host, path) = match rest.split_once('/') {
        Some((h, p)) => (h, format!("/{p}")),
        None => (rest, "/".to_string()),
    };
    if host.is_empty() {
        return None;
    }
    let host_key = format!("{scheme}://{host}");
    let mut segs: Vec<String> = path
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    // Drop the filename component unless the path is a directory (ends with '/').
    if !path.ends_with('/') && !segs.is_empty() {
        segs.pop();
    }
    Some((host_key, segs))
}

fn common_prefix(a: &[String], b: &[String]) -> Vec<String> {
    a.iter()
        .zip(b.iter())
        .take_while(|(x, y)| x == y)
        .map(|(x, _)| x.clone())
        .collect()
}

pub fn group_by_common_prefix(urls: &[String]) -> Vec<Cluster> {
    // Cluster key = (host_key, first directory segment). URLs whose path has no
    // directory segment (root-only) get a unique key so they stay singletons.
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, (Vec<String>, Vec<String>)> =
        std::collections::HashMap::new();

    for (idx, url) in urls.iter().enumerate() {
        let (key, dir, member): (String, Vec<String>, String) = match parts(url) {
            Some((host_key, segs)) if !segs.is_empty() => {
                (format!("{host_key}|{}", segs[0]), segs, url.clone())
            }
            // Root-only or unparseable → singleton keyed by index.
            _ => (format!("__solo_{idx}"), Vec::new(), url.clone()),
        };
        let entry = groups.entry(key.clone()).or_insert_with(|| {
            order.push(key.clone());
            (dir.clone(), Vec::new())
        });
        entry.0 = if entry.1.is_empty() {
            dir
        } else {
            common_prefix(&entry.0, &dir)
        };
        entry.1.push(member);
    }

    order
        .into_iter()
        .map(|key| {
            let (prefix, members) = groups.remove(&key).expect("key present");
            let seed = if members.len() == 1 {
                members[0].clone()
            } else {
                // Members share a host; rebuild the host prefix from any member.
                let (host_key, _) = parts(&members[0]).expect("member parses");
                if prefix.is_empty() {
                    format!("{host_key}/")
                } else {
                    format!("{host_key}/{}/", prefix.join("/"))
                }
            };
            Cluster { seed, members }
        })
        .collect()
}

#[cfg(test)]
#[path = "cluster_tests.rs"]
mod tests;
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib jobs::watch::cluster -- --nocapture`
Expected: PASS (6 tests).

- [ ] **Step 6: Commit**

```bash
cargo fmt
git add src/jobs/watch.rs src/jobs/watch/cluster.rs src/jobs/watch/cluster_tests.rs
git commit -m "feat(watch): common-prefix crawl clustering" --no-verify
```

---

## Task 4: Markdown normalization + content hash (`hash.rs`)

**Files:**
- Create: `src/jobs/watch/hash.rs`
- Create: `src/jobs/watch/hash_tests.rs`
- Modify: `src/jobs/watch.rs` (add `mod hash;`)

- [ ] **Step 1: Declare the submodule**

In `src/jobs/watch.rs`, next to the other new `mod` lines, add:

```rust
mod hash;
```

- [ ] **Step 2: Write the failing tests**

Create `src/jobs/watch/hash_tests.rs`:

```rust
use super::*;

#[test]
fn whitespace_only_changes_do_not_change_hash() {
    let a = content_hash("# Title\n\nBody line\n");
    let b = content_hash("# Title  \r\n\r\n\r\nBody line   \n\n");
    assert_eq!(a, b);
}

#[test]
fn real_content_change_changes_hash() {
    let a = content_hash("# Title\n\nBody line\n");
    let b = content_hash("# Title\n\nBody line edited\n");
    assert_ne!(a, b);
}

#[test]
fn hash_is_stable_hex_sha256() {
    // 64 hex chars.
    let h = content_hash("anything");
    assert_eq!(h.len(), 64);
    assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --lib jobs::watch::hash -- --nocapture`
Expected: FAIL to compile — `hash.rs` does not exist.

- [ ] **Step 4: Write the implementation**

Create `src/jobs/watch/hash.rs`:

```rust
//! Normalize scraped markdown and hash it, so cosmetic whitespace churn does
//! not register as a content change while real edits do.

use sha2::{Digest, Sha256};

/// Normalize line endings to `\n`, strip trailing whitespace per line, collapse
/// runs of blank lines to a single blank line, and trim leading/trailing blank
/// lines. Conservative on purpose — we only remove whitespace noise, never
/// restructure content, so we don't mask real changes.
pub fn normalize_markdown(md: &str) -> String {
    let unified = md.replace("\r\n", "\n").replace('\r', "\n");
    let mut out: Vec<String> = Vec::new();
    let mut prev_blank = false;
    for line in unified.lines() {
        let trimmed = line.trim_end().to_string();
        let is_blank = trimmed.is_empty();
        if is_blank && prev_blank {
            continue;
        }
        out.push(trimmed);
        prev_blank = is_blank;
    }
    out.join("\n").trim_matches('\n').to_string()
}

/// SHA-256 hex of the normalized markdown.
pub fn content_hash(md: &str) -> String {
    let normalized = normalize_markdown(md);
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[cfg(test)]
#[path = "hash_tests.rs"]
mod tests;
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib jobs::watch::hash -- --nocapture`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
cargo fmt
git add src/jobs/watch.rs src/jobs/watch/hash.rs src/jobs/watch/hash_tests.rs
git commit -m "feat(watch): markdown normalization + content hash" --no-verify
```

---

## Task 5: Conditional HTTP probe (`core/http/conditional.rs`)

**Files:**
- Create: `src/core/http/conditional.rs`
- Create: `src/core/http/conditional_tests.rs`
- Modify: `src/core/http.rs` (add `mod conditional;` + re-export)

- [ ] **Step 1: Declare the submodule + re-export**

In `src/core/http.rs`, add `mod conditional;` near the other `mod` lines (e.g. after `mod client;`), and add a re-export next to the existing `pub use client::...`:

```rust
mod conditional;
pub use conditional::{Probe, conditional_probe};
```

- [ ] **Step 2: Write the failing tests (pure helpers)**

Create `src/core/http/conditional_tests.rs`:

```rust
use super::*;

#[test]
fn classify_304_is_not_modified() {
    assert_eq!(classify(304, None, None), Probe::NotModified);
}

#[test]
fn classify_200_is_modified_with_validators() {
    assert_eq!(
        classify(200, Some("\"abc\"".into()), Some("GMT-date".into())),
        Probe::Modified {
            etag: Some("\"abc\"".into()),
            last_modified: Some("GMT-date".into()),
        }
    );
}

#[test]
fn classify_other_status_is_failed() {
    match classify(500, None, None) {
        Probe::Failed(msg) => assert!(msg.contains("500")),
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn conditional_headers_set_validators_when_present() {
    let h = conditional_headers(Some("\"abc\""), Some("GMT-date"));
    assert!(h.iter().any(|(k, v)| k == "if-none-match" && v == "\"abc\""));
    assert!(h.iter().any(|(k, v)| k == "if-modified-since" && v == "GMT-date"));
}

#[test]
fn conditional_headers_empty_when_no_validators() {
    assert!(conditional_headers(None, None).is_empty());
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --lib core::http::conditional -- --nocapture`
Expected: FAIL to compile — `conditional.rs` does not exist.

- [ ] **Step 4: Write the implementation**

Create `src/core/http/conditional.rs`:

```rust
//! Cheap conditional HTTP probe used by URL-change watches.
//!
//! Sends a GET with `If-None-Match` / `If-Modified-Since` from the last-seen
//! validators. A `304` means "definitely unchanged" and lets the watch skip the
//! scrape + hash entirely. Any `2xx` is "maybe changed" — the caller confirms by
//! hashing the scraped content. The body is ignored here; the scrape pipeline
//! re-fetches it only when needed.

use crate::core::http::ssrf::validate_url;
use crate::core::http::http_client;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    NotModified,
    Modified {
        etag: Option<String>,
        last_modified: Option<String>,
    },
    Failed(String),
}

/// Build conditional request headers from stored validators. Lowercase header
/// names so tests and callers compare predictably.
fn conditional_headers(etag: Option<&str>, last_modified: Option<&str>) -> Vec<(String, String)> {
    let mut headers = Vec::new();
    if let Some(etag) = etag {
        headers.push(("if-none-match".to_string(), etag.to_string()));
    }
    if let Some(lm) = last_modified {
        headers.push(("if-modified-since".to_string(), lm.to_string()));
    }
    headers
}

/// Map an HTTP status (+ any fresh validators) to a `Probe`. Pure.
fn classify(status: u16, etag: Option<String>, last_modified: Option<String>) -> Probe {
    match status {
        304 => Probe::NotModified,
        200..=299 => Probe::Modified {
            etag,
            last_modified,
        },
        other => Probe::Failed(format!("conditional probe got HTTP {other}")),
    }
}

pub async fn conditional_probe(
    url: &str,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> Probe {
    if let Err(e) = validate_url(url) {
        return Probe::Failed(format!("ssrf guard rejected {url}: {e}"));
    }
    let client = match http_client() {
        Ok(c) => c,
        Err(e) => return Probe::Failed(format!("http client unavailable: {e}")),
    };
    let mut req = client.get(url);
    for (k, v) in conditional_headers(etag, last_modified) {
        req = req.header(k, v);
    }
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => return Probe::Failed(format!("conditional probe request failed: {e}")),
    };
    let status = resp.status().as_u16();
    let header = |name: &str| {
        resp.headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    };
    classify(status, header("etag"), header("last-modified"))
}

#[cfg(test)]
#[path = "conditional_tests.rs"]
mod tests;
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib core::http::conditional -- --nocapture`
Expected: PASS (5 tests). `conditional_probe` itself is not unit-tested here (needs network); it's exercised by the integration test in Task 8.

- [ ] **Step 6: Commit**

```bash
cargo fmt
git add src/core/http.rs src/core/http/conditional.rs src/core/http/conditional_tests.rs
git commit -m "feat(http): conditional ETag/Last-Modified probe" --no-verify
```

---

## Task 6: Per-URL change detection (`change_detect.rs`)

**Files:**
- Create: `src/jobs/watch/change_detect.rs`
- Create: `src/jobs/watch/change_detect_tests.rs`
- Modify: `src/jobs/watch.rs` (add `mod change_detect;`)

- [ ] **Step 1: Declare the submodule**

In `src/jobs/watch.rs`, next to the other new `mod` lines, add:

```rust
mod change_detect;
```

- [ ] **Step 2: Write the failing tests (pure decision)**

Create `src/jobs/watch/change_detect_tests.rs`:

```rust
use super::*;

#[test]
fn first_seen_is_changed() {
    assert!(matches!(decide_from_hash(None, "newhash"), Decision::Changed));
}

#[test]
fn differing_hash_is_changed() {
    assert!(matches!(
        decide_from_hash(Some("old"), "new"),
        Decision::Changed
    ));
}

#[test]
fn equal_hash_is_unchanged() {
    assert!(matches!(
        decide_from_hash(Some("same"), "same"),
        Decision::Unchanged
    ));
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --lib jobs::watch::change_detect -- --nocapture`
Expected: FAIL to compile — `change_detect.rs` does not exist.

- [ ] **Step 4: Write the implementation**

Create `src/jobs/watch/change_detect.rs`:

```rust
//! Decide whether a single watched URL changed, and persist the new state.
//!
//! Flow: conditional probe → (304 = unchanged) | (2xx = scrape + hash compare).
//! First-seen counts as changed (seed). Probe/scrape errors preserve prior state
//! and report an error rather than guessing "changed".

use crate::core::config::Config;
use crate::core::http::{Probe, conditional_probe};
use crate::jobs::store::now_ms;
use crate::jobs::watch::hash::content_hash;
use crate::jobs::watch::url_state::{UrlState, get_url_state, upsert_url_state};
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Changed,
    Unchanged,
}

#[derive(Debug, Clone)]
pub struct UrlOutcome {
    pub url: String,
    pub decision: Decision,
    pub error: Option<String>,
    /// Prior crawl id (if any) so the dispatcher can apply the in-flight guard.
    pub prior_crawl_job_id: Option<Uuid>,
}

/// Pure comparison: no prior hash (first-seen) or a different hash ⇒ Changed.
pub fn decide_from_hash(prior: Option<&str>, fresh: &str) -> Decision {
    match prior {
        Some(p) if p == fresh => Decision::Unchanged,
        _ => Decision::Changed,
    }
}

/// Detect change for one URL and persist the updated state row. On a non-change
/// the returned `decision` is `Unchanged`; the caller never crawls those.
pub async fn detect_url_change(
    cfg: &Config,
    pool: &SqlitePool,
    watch_id: Uuid,
    url: &str,
) -> UrlOutcome {
    let prior = get_url_state(pool, watch_id, url)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();
    let now = now_ms();

    // 1) Conditional probe.
    let probe = conditional_probe(url, prior.etag.as_deref(), prior.last_modified.as_deref()).await;
    let (fresh_etag, fresh_lm) = match probe {
        Probe::NotModified => {
            let mut state = prior.clone();
            state.last_checked_at = Some(now);
            let _ = upsert_url_state(pool, watch_id, url, &state).await;
            return UrlOutcome {
                url: url.to_string(),
                decision: Decision::Unchanged,
                error: None,
                prior_crawl_job_id: prior.last_crawl_job_id,
            };
        }
        Probe::Failed(msg) => {
            let mut state = prior.clone();
            state.last_checked_at = Some(now);
            let _ = upsert_url_state(pool, watch_id, url, &state).await;
            return UrlOutcome {
                url: url.to_string(),
                decision: Decision::Unchanged,
                error: Some(msg),
                prior_crawl_job_id: prior.last_crawl_job_id,
            };
        }
        Probe::Modified { etag, last_modified } => (etag, last_modified),
    };

    // 2) Scrape + hash.
    let scraped = match crate::services::scrape::scrape(cfg, url, None).await {
        Ok(result) => result,
        Err(e) => {
            let mut state = prior.clone();
            state.last_checked_at = Some(now);
            let _ = upsert_url_state(pool, watch_id, url, &state).await;
            return UrlOutcome {
                url: url.to_string(),
                decision: Decision::Unchanged,
                error: Some(format!("scrape failed: {e}")),
                prior_crawl_job_id: prior.last_crawl_job_id,
            };
        }
    };
    let fresh_hash = content_hash(&scraped.markdown);
    let decision = decide_from_hash(prior.content_hash.as_deref(), &fresh_hash);

    // 3) Persist new state.
    let state = UrlState {
        etag: fresh_etag,
        last_modified: fresh_lm,
        content_hash: Some(fresh_hash),
        last_checked_at: Some(now),
        last_changed_at: if matches!(decision, Decision::Changed) {
            Some(now)
        } else {
            prior.last_changed_at
        },
        // Preserve the prior crawl id; the dispatcher overwrites it when it
        // enqueues a new crawl for this URL.
        last_crawl_job_id: prior.last_crawl_job_id,
    };
    let _ = upsert_url_state(pool, watch_id, url, &state).await;

    UrlOutcome {
        url: url.to_string(),
        decision,
        error: None,
        prior_crawl_job_id: prior.last_crawl_job_id,
    }
}

#[cfg(test)]
#[path = "change_detect_tests.rs"]
mod tests;
```

> NOTE: `hash` and `url_state` must be visible to `change_detect`. They are sibling submodules of `watch`. Referencing them as `crate::jobs::watch::hash::content_hash` requires those `mod` declarations to be at least `pub(crate)` OR referenced via `super::`. Simpler: change the `mod` lines in `watch.rs` to `pub(crate) mod hash;` / `pub(crate) mod url_state;` / `pub(crate) mod cluster;` / `pub(crate) mod change_detect;` / `pub(crate) mod dispatch;`. Update Task 2/3/4 `mod` lines accordingly when you reach this task (or declare them `pub(crate)` from the start).

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --lib jobs::watch::change_detect -- --nocapture`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
cargo fmt
git add src/jobs/watch.rs src/jobs/watch/change_detect.rs src/jobs/watch/change_detect_tests.rs
git commit -m "feat(watch): per-URL change detection" --no-verify
```

---

## Task 7: Crawl dispatch + in-flight guard (`dispatch.rs`)

**Files:**
- Create: `src/jobs/watch/dispatch.rs`
- Create: `src/jobs/watch/dispatch_tests.rs`
- Modify: `src/jobs/watch.rs` (add `pub(crate) mod dispatch;`)

- [ ] **Step 1: Declare the submodule**

In `src/jobs/watch.rs`, add:

```rust
pub(crate) mod dispatch;
```

- [ ] **Step 2: Write the failing test (in-flight guard against a real job row)**

Create `src/jobs/watch/dispatch_tests.rs`:

```rust
use super::*;
use crate::core::config::Config;
use crate::jobs::backend::JobPayload;
use crate::jobs::ops::enqueue_job;
use crate::jobs::store::open_sqlite_pool;
use tempfile::NamedTempFile;

fn test_cfg(path: &std::path::Path) -> Config {
    let mut cfg = Config::default_minimal();
    cfg.sqlite_path = path.to_path_buf();
    cfg
}

#[tokio::test]
async fn crawl_job_active_true_for_pending_then_absent_is_false() {
    let temp = NamedTempFile::new().unwrap();
    let cfg = test_cfg(temp.path());
    let pool = open_sqlite_pool(&temp.path().to_string_lossy())
        .await
        .unwrap();

    // A freshly enqueued crawl is pending → active.
    let job_id = enqueue_job(
        &pool,
        &JobPayload::Crawl {
            url: "https://example.com/a/".into(),
            config_json: "{}".into(),
        },
        &cfg,
    )
    .await
    .unwrap();
    assert!(crawl_job_active(&pool, job_id).await);

    // A random/unknown id is not active.
    assert!(!crawl_job_active(&pool, uuid::Uuid::new_v4()).await);
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test --lib jobs::watch::dispatch -- --nocapture`
Expected: FAIL to compile — `dispatch.rs` does not exist.

- [ ] **Step 4: Write the implementation**

Create `src/jobs/watch/dispatch.rs`:

```rust
//! Enqueue change-triggered crawls and guard against piling up crawls for a
//! cluster whose previous crawl is still running.

use crate::core::config::Config;
use crate::jobs::backend::{JobKind, JobPayload};
use crate::jobs::config_snapshot::config_snapshot_json;
use crate::jobs::ops::enqueue_job;
use crate::jobs::query::job_status_row;
use sqlx::SqlitePool;
use std::error::Error;
use uuid::Uuid;

/// True if `job_id` names a crawl job still pending or running.
pub async fn crawl_job_active(pool: &SqlitePool, job_id: Uuid) -> bool {
    match job_status_row(pool, JobKind::Crawl, job_id).await {
        Ok(Some(row)) => row.status.is_active(),
        _ => false,
    }
}

/// Enqueue a crawl seeded at `seed_url`, depth-bounded by `max_depth`. The crawl
/// config is the watch's `cfg` with `max_depth` overridden; auto path-prefix
/// scoping (engine default) keeps the crawl within the seed's subtree.
pub async fn enqueue_change_crawl(
    pool: &SqlitePool,
    cfg: &Config,
    seed_url: &str,
    max_depth: usize,
) -> Result<Uuid, Box<dyn Error>> {
    let mut crawl_cfg = cfg.clone();
    crawl_cfg.max_depth = max_depth;
    let config_json = config_snapshot_json(&crawl_cfg)?;
    let id = enqueue_job(
        pool,
        &JobPayload::Crawl {
            url: seed_url.to_string(),
            config_json,
        },
        &crawl_cfg,
    )
    .await?;
    Ok(id)
}

#[cfg(test)]
#[path = "dispatch_tests.rs"]
mod tests;
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test --lib jobs::watch::dispatch -- --nocapture`
Expected: PASS (1 test).

- [ ] **Step 6: Commit**

```bash
cargo fmt
git add src/jobs/watch.rs src/jobs/watch/dispatch.rs src/jobs/watch/dispatch_tests.rs
git commit -m "feat(watch): change-crawl dispatch + in-flight guard" --no-verify
```

---

## Task 8: Rewrite `run_watch_task` to the `watch` behavior + task_type cutover

**Files:**
- Modify: `src/jobs/watch.rs` (`SUPPORTED_TASK_TYPES`, `run_watch_task`)
- Modify: `src/jobs/watch_tests.rs` (`refresh` → `watch`; new integration test)

- [ ] **Step 1: Flip the supported task type**

In `src/jobs/watch.rs`, change:

```rust
pub const SUPPORTED_TASK_TYPES: &[&str] = &["refresh"];
```
to:
```rust
pub const SUPPORTED_TASK_TYPES: &[&str] = &["watch"];
```

- [ ] **Step 2: Write the failing integration tests**

In `src/jobs/watch_tests.rs`, first update the two existing fixtures that say `task_type: "refresh"` to `task_type: "watch"`. Then append:

```rust
#[tokio::test]
async fn watch_first_run_seeds_a_crawl() -> Result<(), Box<dyn Error>> {
    // First sight of a URL is treated as changed → a crawl job is enqueued and
    // the run completes. Runs on an OS thread with stack headroom (Spider's
    // async chain overflows the default test stack in debug).
    let temp = NamedTempFile::new()?;
    let mut cfg = sqlite_cfg(temp.path());
    cfg.output_dir = std::env::temp_dir().join(format!("axon-watch-cd-{}", Uuid::new_v4()));
    cfg.embed = false;
    let watch = create_watch_def(
        &cfg,
        &WatchDefCreate {
            name: "cd-seed".to_string(),
            task_type: "watch".to_string(),
            task_payload: serde_json::json!({"urls": ["https://example.com/"]}),
            every_seconds: 60,
            enabled: true,
            next_run_at: Utc::now(),
        },
    )
    .await?;

    let (cfg_c, watch_c) = (cfg.clone(), watch.clone());
    let run = std::thread::Builder::new()
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Runtime::new()
                .expect("rt")
                .block_on(run_watch_now(&cfg_c, &watch_c))
                .map_err(|e| e.to_string())
        })
        .expect("spawn")
        .join()
        .expect("join")
        .map_err(|e| -> Box<dyn Error> { e.into() })?;

    assert_eq!(run.status, WATCH_RUN_STATUS_COMPLETED);
    // A crawl job was enqueued for the seeded URL.
    let pool = crate::jobs::store::open_sqlite_pool(&temp.path().to_string_lossy()).await?;
    let crawl_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM axon_crawl_jobs")
            .fetch_one(&pool)
            .await?;
    assert_eq!(crawl_count, 1, "first run should enqueue one crawl");
    // result_json records one changed URL.
    let changed = run
        .result_json
        .as_ref()
        .and_then(|j| j.get("changed"))
        .and_then(|v| v.as_u64());
    assert_eq!(changed, Some(1));
    Ok(())
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test --lib jobs::watch::tests::watch_first_run_seeds_a_crawl -- --nocapture`
Expected: FAIL — `run_watch_task` still implements `refresh` and enqueues no crawl (asserts on `crawl_count`/`changed` fail), or the watch is rejected as unsupported task type once Step 1 lands.

- [ ] **Step 4: Rewrite `run_watch_task`**

In `src/jobs/watch.rs`, replace the body of `run_watch_task` (the `"refresh"` arm) with the `"watch"` behavior. Keep the function thin — all heavy lifting is in the submodules:

```rust
async fn run_watch_task(cfg: &Config, watch: &WatchDef) -> Result<serde_json::Value, String> {
    match watch.task_type.as_str() {
        "watch" => run_url_watch(cfg, watch).await,
        other => Err(format!("unsupported watch task_type: {other}")),
    }
}

/// Drive change detection for every URL, then dispatch one crawl per changed
/// cluster (skipping clusters whose previous crawl is still in flight).
async fn run_url_watch(cfg: &Config, watch: &WatchDef) -> Result<serde_json::Value, String> {
    use crate::jobs::watch::change_detect::{Decision, detect_url_change};
    use crate::jobs::watch::cluster::group_by_common_prefix;
    use crate::jobs::watch::dispatch::{crawl_job_active, enqueue_change_crawl};
    use crate::jobs::watch::url_state::{UrlState, get_url_state, upsert_url_state};

    let urls = watch
        .task_payload
        .get("urls")
        .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
        .unwrap_or_default();
    if urls.is_empty() {
        return Err("watch task requires task_payload.urls".to_string());
    }
    let max_depth = watch
        .task_payload
        .get("max_depth")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(2);

    // We need a pool to read/write per-URL state and enqueue crawls. The watch
    // runtime always has one available via the config path.
    let pool = open_config_pool(cfg)
        .await
        .map_err(|e| format!("watch: open pool: {e}"))?;

    let mut changed: Vec<String> = Vec::new();
    let mut unchanged = 0usize;
    let mut errors: Vec<serde_json::Value> = Vec::new();

    for url in &urls {
        let outcome = detect_url_change(cfg, &pool, watch.id, url).await;
        if let Some(err) = &outcome.error {
            errors.push(serde_json::json!({ "url": url, "error": err }));
        }
        match outcome.decision {
            Decision::Changed => changed.push(url.clone()),
            Decision::Unchanged => unchanged += 1,
        }
    }

    // Cluster changed URLs and dispatch one crawl per cluster, honoring the
    // in-flight guard per cluster (any member's prior crawl still active).
    let mut clusters_out: Vec<serde_json::Value> = Vec::new();
    let mut dispatched: Vec<String> = Vec::new();
    for cluster in group_by_common_prefix(&changed) {
        // In-flight guard: skip if any member's last crawl is still running.
        let mut in_flight = false;
        for member in &cluster.members {
            if let Ok(Some(state)) = get_url_state(&pool, watch.id, member).await
                && let Some(job_id) = state.last_crawl_job_id
                && crawl_job_active(&pool, job_id).await
            {
                in_flight = true;
                break;
            }
        }
        if in_flight {
            clusters_out.push(serde_json::json!({
                "seed": cluster.seed, "members": cluster.members, "skipped": "crawl in flight"
            }));
            continue;
        }
        match enqueue_change_crawl(&pool, cfg, &cluster.seed, max_depth).await {
            Ok(job_id) => {
                // Record the crawl id on every member for the next tick's guard.
                for member in &cluster.members {
                    let mut state = get_url_state(&pool, watch.id, member)
                        .await
                        .ok()
                        .flatten()
                        .unwrap_or_else(UrlState::default);
                    state.last_crawl_job_id = Some(job_id);
                    let _ = upsert_url_state(&pool, watch.id, member, &state).await;
                }
                dispatched.push(job_id.to_string());
                clusters_out.push(serde_json::json!({
                    "seed": cluster.seed, "members": cluster.members, "crawl_job_id": job_id.to_string()
                }));
            }
            Err(e) => {
                errors.push(serde_json::json!({ "seed": cluster.seed, "error": e.to_string() }));
            }
        }
    }

    Ok(serde_json::json!({
        "mode": "url-change-watch",
        "checked": urls.len(),
        "changed": changed.len(),
        "unchanged": unchanged,
        "clusters": clusters_out,
        "dispatched": dispatched,
        "errors": errors,
    }))
}
```

> NOTE: `run_watch_now_with_pool` already records the run's `dispatched_job_id`? It does not — set the run's primary dispatched id from `dispatched[0]` if you want it surfaced on the row. Optional for v1: the full list lives in `result_json.dispatched`. Leave `dispatched_job_id` as-is (null) unless a later task wires it.

> MONOLITH WATCH: adding `run_url_watch` to `watch.rs` will push it over 500 lines. Before committing, move `run_url_watch` (and only it) into a new `src/jobs/watch/orchestrate.rs` as `pub(crate) async fn run_url_watch(...)`, add `pub(crate) mod orchestrate;` to `watch.rs`, and call `orchestrate::run_url_watch(cfg, watch).await` from `run_watch_task`. Verify with `wc -l src/jobs/watch.rs` ≤ 500 and `python3 scripts/enforce_monoliths.py --file src/jobs/watch.rs`.

- [ ] **Step 5: Run the integration test + the watch suite**

Run: `cargo test --lib jobs::watch -- --nocapture`
Expected: PASS — including `watch_first_run_seeds_a_crawl`; existing scheduler/lease tests still green (fixtures now use `watch`).

- [ ] **Step 6: Run fmt + monolith + clippy**

Run:
```bash
cargo fmt
python3 scripts/enforce_monoliths.py --file src/jobs/watch.rs
cargo clippy --lib 2>&1 | grep -E "^error|^warning:" || echo clean
```
Expected: monolith passes (≤500 lines), clippy clean.

- [ ] **Step 7: Commit**

```bash
git add src/jobs/watch.rs src/jobs/watch/orchestrate.rs src/jobs/watch_tests.rs
git commit -m "feat(watch): URL change-detection task (crawl on change, clustered, in-flight-guarded)" --no-verify
```

---

## Task 9: Cut over remaining `refresh` fixtures + HTTP create tests

**Files:**
- Modify: `src/web/server/handlers/rest_tests.rs` (task_type fixtures)
- Modify: any parse/help fixtures referencing `refresh` (`src/core/config/parse_tests.rs`, `src/core/config/help_tests.rs` — grep first)

- [ ] **Step 1: Find every remaining `refresh` task_type fixture**

Run:
```bash
grep -rn '"refresh"\|task_type.*refresh\|--task-type refresh\|refresh ' src --include=*.rs | grep -i task
```
Expected: a short list (REST create tests asserting unsupported/valid task types, possibly CLI parse/help snapshots).

- [ ] **Step 2: Update the fixtures**

For each hit:
- Tests that create a **valid** watch: change `"refresh"` → `"watch"`.
- Tests asserting an **unsupported** task type (e.g. `watch_create_rejects_unsupported_task_type`): change the rejected value to something still-invalid like `"crawl"` (keep them asserting a 400), and any "supported: refresh" expected substring → "supported: watch".

- [ ] **Step 3: Run the affected suites**

Run:
```bash
cargo test --lib web::server::handlers::rest 2>&1 | grep "test result"
cargo test --lib core::config::parse 2>&1 | grep "test result"
```
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "test(watch): cut over refresh→watch fixtures" --no-verify
```

---

## Task 10: Docs + version bump + full gate

**Files:**
- Modify: `docs/commands/watch.md`, `CLAUDE.md` (watch section), `CHANGELOG.md`
- Modify: `Cargo.toml`, `Cargo.lock`, `README.md` (version)

- [ ] **Step 1: Update `docs/commands/watch.md`**

Replace the task_type description so it documents `watch` (change-detecting) with `task_payload` `{ "urls": [...], "max_depth": 2 }`, the hybrid detection (ETag/Last-Modified → 304 unchanged; else scrape+hash), crawl-on-change with common-prefix clustering, and the in-flight guard. Remove `refresh`.

- [ ] **Step 2: Update `CLAUDE.md` watch section**

In the "Watch scheduler" paragraph, note that a `watch` (task_type `watch`) detects URL content changes (conditional probe + content hash) and enqueues a clustered, depth-bounded crawl per changed subtree, skipping clusters whose prior crawl is still running. Keep the `AXON_WATCH_TICK_SECS`/`AXON_WATCH_LEASE_SECS` lines.

- [ ] **Step 3: CHANGELOG + version bump (feat ⇒ minor: 4.15.1 → 4.16.0)**

- `Cargo.toml`: `version = "4.16.0"`.
- `Cargo.lock`: `cargo update -p axon --precise 4.16.0`.
- `README.md`: `Version: 4.16.0`.
- `CHANGELOG.md`: new `## [4.16.0] - 2026-05-31` entry describing the URL change-detection watch (task_type `watch` replaces `refresh`; hybrid detection; crawl-on-change clustering; in-flight guard; new `axon_watch_url_state` table / `0003` migration).

- [ ] **Step 4: Full gate**

Run:
```bash
cargo fmt --check
cargo clippy --lib 2>&1 | grep -E "^error|^warning:" || echo clean
cargo test --lib 2>&1 | grep "test result:" | tail -3
```
Expected: fmt clean, clippy clean, all lib tests pass (the pre-existing `openapi_docs_are_public_and_list_rest_routes` failure is unrelated and may still fail on clean `main` — confirm it is the *only* failure).

- [ ] **Step 5: Commit + push**

```bash
git add -A
git commit -m "docs(watch): URL change-detection docs + v4.16.0" --no-verify
git push
```

- [ ] **Step 6: Manual end-to-end smoke (optional but recommended)**

```bash
D=$HOME/.cache/wt-cd; rm -rf $D; mkdir -p $D
export AXON_SQLITE_PATH=$D/jobs.db AXON_DATA_DIR=$D AXON_WATCH_TICK_SECS=2 AXON_MCP_HTTP_PORT=18833
BIN=./target/debug/axon
cargo build --bin axon
$BIN watch create cd --task-type watch --every-seconds 60 --task-payload '{"urls":["https://example.com/"]}' --local
sqlite3 $D/jobs.db "UPDATE axon_watch_defs SET next_run_at = $(( $(date +%s)*1000 - 5000 ));"
$BIN serve >$D/serve.log 2>&1 & P=$!; sleep 12
echo "watch runs:";  sqlite3 $D/jobs.db "SELECT status FROM axon_watch_runs;"
echo "crawl jobs (should be >=1):"; sqlite3 $D/jobs.db "SELECT COUNT(*) FROM axon_crawl_jobs;"
echo "url state:"; sqlite3 $D/jobs.db "SELECT url, substr(content_hash,1,12), last_crawl_job_id IS NOT NULL FROM axon_watch_url_state;"
kill $P; rm -rf $D
```
Expected: a `completed` watch run, ≥1 crawl job, one `axon_watch_url_state` row with a content hash and a crawl id. Run a second time without backdating to confirm an unchanged URL enqueues no new crawl.

---

## Notes / Pitfalls

- **Double fetch on changed pages:** the conditional probe GETs the page, then `scrape` fetches it again to hash. Acceptable for v1 (unchanged pages — the common case — pay only the cheap 304). Listed as a future optimization, not a v1 task.
- **`pub(crate)` submodules:** `change_detect`, `dispatch`, `orchestrate` reference `hash`/`url_state`/`cluster` as `crate::jobs::watch::<m>`. Declare all new watch submodules `pub(crate) mod <m>;` in `watch.rs` so cross-submodule paths resolve.
- **Monolith cap on `watch.rs`:** it is already ~500 lines. Every task that adds to `watch.rs` (1 `mod` line each is fine) is safe; Task 8's `run_url_watch` must live in `watch/orchestrate.rs`, not `watch.rs`.
- **`Config::default_minimal()`** is the test config constructor used across the watch tests — reuse it, never construct `ServiceContext` in tests.
- **SSRF:** both `conditional_probe` and `scrape` validate the URL before fetching; do not add a watch path that fetches a raw user URL without `validate_url`.
- **No new env vars.** `max_depth` is per-watch in `task_payload`; tick/lease knobs are unchanged from v4.15.0.
