# Axon Status Trim — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cut the default `axon status` MCP response from ~374 KB single-line to under 10 KB by replacing per-job blobs with totals + per-state histograms, while preserving today's per-job listing under a new `subaction=jobs`.

**Architecture:**
- Add a SQL helper `count_jobs_by_status(pool, kind)` that runs `SELECT status, COUNT(*) ... GROUP BY status` per family and returns a `HashMap<JobStatus, i64>`. Expose it through `ServiceJobRuntime`.
- Introduce `ServiceJobSummary` (a trimmed `ServiceJob` without `result_json` / `config_json` / `urls_json`) plus a `StatusView` enum (`Summary` | `Jobs`) consumed by `services::system::full_status`.
- Default `subaction` of the MCP `status` action becomes `summary`. The existing per-job payload moves under `subaction=jobs`. The `axon_status_dashboard` MCP App keeps calling the jobs variant explicitly so its widget keeps working.
- CLI `axon status` defaults to the summary; `axon status jobs` produces today's per-job render.

**Tech Stack:** Rust 2024 · sqlx (SQLite) · rmcp · tokio · serde_json. No new crates.

**Bead:** axon_rust-9pbb

**Out of scope:**
- Pretty-printing artifact JSON: `src/mcp/server/artifacts/respond.rs::write_json_artifact` already calls `serde_json::to_string_pretty`. The single-line spill observed in the bug report was the harness tool-result spill (under `~/.claude/projects/...`), not Axon's artifact. No change required here.
- A `subaction=job` for full per-job blobs: existing per-family commands (`crawl status <id>`, `extract status <id>`, etc.) already cover that.

---

## File Structure

**Modify:**
- `src/jobs/query.rs` — add `count_jobs_by_status` SQL helper
- `src/jobs/query_tests.rs` — add helper unit tests (sidecar file declared from `query.rs`; create if it doesn't exist)
- `src/services/runtime.rs` — add `count_jobs_by_status` trait method + `SqliteServiceRuntime` impl
- `src/services/runtime_tests.rs` — add coverage for the new method
- `src/services/types/service.rs` — add `ServiceJobSummary`, `StatusView`, `StatusSummary`, `StatusKindSummary`; new `From<&ServiceJob>` for `ServiceJobSummary`
- `src/services/system/status.rs` — accept `StatusView`; new `build_summary_payload`; rework `full_status`
- `src/services/system/status_tests.rs` — extend with summary-view + jobs-view assertions
- `src/mcp/server/handlers_system.rs` — `handle_status` reads `subaction`, defaults to summary, routes to jobs view when requested, rejects other strings with `invalid_params`
- `src/mcp/schema/requests.rs` — drop `#[allow(dead_code)]` on `StatusRequest::subaction`
- `src/cli/commands/status.rs` — default human/JSON output to summary; route `cfg.positional[0] == "jobs"` to the existing per-job renderer
- `src/cli/commands/status_tests.rs` — add summary-render coverage
- `docs/MCP-TOOL-SCHEMA.md` and `docs/MCP.md` — document the new subactions

**Touch (only to update mock `ServiceJobRuntime` impls — add `count_jobs_by_status` returning empty map):**
- `src/services/search_crawl_tests.rs`
- `src/services/jobs_tests.rs`
- `src/services/embed_tests.rs`
- `src/services/ingest_tests.rs`
- `src/services/action_api_tests.rs`
- `src/services/crawl_tests.rs`
- `src/web/server_test_support_tests.rs`
- `src/web/server/handlers/rest_tests.rs`
- `src/cli/commands/ingest_tests.rs`

---

## Task 1: SQL helper — `count_jobs_by_status`

**Files:**
- Modify: `src/jobs/query.rs`
- Test:   `src/jobs/query_tests.rs` (sibling sidecar; create if missing and declare it from `query.rs`)

Adds one helper that groups by `status` and returns the histogram. This is what the new summary view depends on; everything else builds on it.

- [ ] **Step 1: Check whether a test sidecar exists**

Run: `ls src/jobs/query_tests.rs 2>&1 || echo missing`
Expected: either a path or `missing`. If `missing`, create it in Step 2.

- [ ] **Step 2: Add the failing test**

If `src/jobs/query_tests.rs` does not exist, create it with this content. If it already exists, append the test inside the existing `use super::*;` module.

```rust
use super::*;
use crate::jobs::backend::JobKind;
use crate::jobs::status::JobStatus;
use sqlx::sqlite::SqlitePoolOptions;

async fn fresh_pool() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    crate::jobs::store::ensure_schema(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn count_jobs_by_status_returns_histogram_per_kind() {
    let pool = fresh_pool().await;

    // Seed two pending, one running, one failed crawl rows.
    for status in [
        JobStatus::Pending,
        JobStatus::Pending,
        JobStatus::Running,
        JobStatus::Failed,
    ] {
        sqlx::query(
            "INSERT INTO axon_crawl_jobs (id, url, status, created_at, updated_at) \
             VALUES (?, 'https://example.com/', ?, 0, 0)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(status.as_str())
        .execute(&pool)
        .await
        .unwrap();
    }

    let histogram = count_jobs_by_status(&pool, JobKind::Crawl).await.unwrap();
    assert_eq!(histogram.get(&JobStatus::Pending).copied(), Some(2));
    assert_eq!(histogram.get(&JobStatus::Running).copied(), Some(1));
    assert_eq!(histogram.get(&JobStatus::Failed).copied(), Some(1));
    assert!(histogram.get(&JobStatus::Completed).is_none());
}
```

Declaration in `src/jobs/query.rs` (append at the end of the file if not present):

```rust
#[cfg(test)]
#[path = "query_tests.rs"]
mod query_tests;
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --locked --lib jobs::query::query_tests::count_jobs_by_status_returns_histogram_per_kind -- --nocapture`
Expected: FAIL — `count_jobs_by_status` is not defined.

- [ ] **Step 4: Implement `count_jobs_by_status`**

Add to `src/jobs/query.rs` immediately after `count_jobs`:

```rust
use crate::jobs::status::JobStatus;
use std::collections::HashMap;

/// Per-status histogram for a single job kind.
///
/// Returns one entry per distinct `status` value present in the table. Missing
/// statuses are absent from the map (callers must treat absent as zero).
/// Unknown DB values (should never happen given the CHECK constraint) are
/// folded into `JobStatus::Failed` to match `JobStatus::from_str`.
pub async fn count_jobs_by_status(
    pool: &SqlitePool,
    kind: JobKind,
) -> Result<HashMap<JobStatus, i64>, sqlx::Error> {
    let table = kind.table_name();
    let rows: Vec<(String, i64)> = sqlx::query_as(&format!(
        "SELECT status, COUNT(*) FROM {} GROUP BY status",
        table
    ))
    .fetch_all(pool)
    .await?;

    let mut out: HashMap<JobStatus, i64> = HashMap::new();
    for (raw_status, count) in rows {
        let key = JobStatus::from_str(&raw_status);
        *out.entry(key).or_insert(0) += count;
    }
    Ok(out)
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --locked --lib jobs::query::query_tests::count_jobs_by_status_returns_histogram_per_kind`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/jobs/query.rs src/jobs/query_tests.rs
git commit -m "feat(jobs): add count_jobs_by_status histogram helper"
```

---

## Task 2: Thread `count_jobs_by_status` through `ServiceJobRuntime`

**Files:**
- Modify: `src/services/runtime.rs`
- Modify: `src/services/runtime_tests.rs`
- Modify (mocks): `src/services/search_crawl_tests.rs`, `src/services/jobs_tests.rs`, `src/services/embed_tests.rs`, `src/services/ingest_tests.rs`, `src/services/action_api_tests.rs`, `src/services/crawl_tests.rs`, `src/web/server_test_support_tests.rs`, `src/web/server/handlers/rest_tests.rs`, `src/cli/commands/ingest_tests.rs`

CLI/MCP code only sees `ServiceJobRuntime`. Adding the method to the trait keeps the contract honest and lets the summary handler stay in the services layer instead of poking at `SqliteJobBackend` directly.

- [ ] **Step 1: Add a failing test for the SQLite runtime impl**

Append to `src/services/runtime_tests.rs`:

```rust
#[tokio::test]
async fn sqlite_runtime_count_jobs_by_status_groups_per_state() {
    use crate::jobs::backend::JobKind;
    use crate::jobs::status::JobStatus;

    let runtime = build_test_runtime().await; // existing helper in this file
    let pool = runtime.sqlite_pool().expect("sqlite pool");

    for status in [JobStatus::Pending, JobStatus::Running, JobStatus::Failed] {
        sqlx::query(
            "INSERT INTO axon_embed_jobs (id, input_text, config_json, status, created_at, updated_at) \
             VALUES (?, '', '{}', ?, 0, 0)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(status.as_str())
        .execute(&*pool)
        .await
        .unwrap();
    }

    let histogram = runtime.count_jobs_by_status(JobKind::Embed).await.unwrap();
    assert_eq!(histogram.get(&JobStatus::Pending).copied(), Some(1));
    assert_eq!(histogram.get(&JobStatus::Running).copied(), Some(1));
    assert_eq!(histogram.get(&JobStatus::Failed).copied(), Some(1));
}
```

If no `build_test_runtime` helper exists in `runtime_tests.rs`, scan the file for the pattern other tests use to build a `SqliteServiceRuntime` over an in-memory pool and copy that. Do NOT introduce a new pattern.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --locked --lib services::runtime::tests::sqlite_runtime_count_jobs_by_status_groups_per_state`
Expected: FAIL — method missing on the trait.

- [ ] **Step 3: Add the trait method (after the existing `count_jobs` declaration around `runtime.rs:130`)**

```rust
    /// Per-status histogram for a single job kind. Missing statuses are
    /// absent from the map; callers treat absent as zero.
    async fn count_jobs_by_status(
        &self,
        kind: JobKind,
    ) -> Result<std::collections::HashMap<crate::jobs::status::JobStatus, i64>, Box<dyn Error + Send + Sync>>;
```

- [ ] **Step 4: Implement on `SqliteServiceRuntime` (next to the existing `count_jobs` impl around `runtime.rs:293`)**

```rust
    async fn count_jobs_by_status(
        &self,
        kind: JobKind,
    ) -> Result<std::collections::HashMap<crate::jobs::status::JobStatus, i64>, Box<dyn Error + Send + Sync>> {
        Ok(job_query::count_jobs_by_status(self.backend.pool(), kind).await?)
    }
```

- [ ] **Step 5: Add `count_jobs_by_status` to every mock impl so the workspace compiles**

For each of these files, find the existing `async fn count_jobs(...)` method and add immediately after it:

```rust
    async fn count_jobs_by_status(
        &self,
        _kind: JobKind,
    ) -> Result<std::collections::HashMap<crate::jobs::status::JobStatus, i64>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(std::collections::HashMap::new())
    }
```

Files to update:
- `src/services/search_crawl_tests.rs`
- `src/services/jobs_tests.rs`
- `src/services/embed_tests.rs`
- `src/services/ingest_tests.rs`
- `src/services/action_api_tests.rs`
- `src/services/crawl_tests.rs`
- `src/web/server_test_support_tests.rs`
- `src/web/server/handlers/rest_tests.rs`
- `src/cli/commands/ingest_tests.rs`

If any of these files use a slightly different `Box<dyn Error ...>` alias (e.g. `Box<dyn StdError + Send + Sync>` in `search_crawl_tests.rs`), match the existing `count_jobs` error type in that file exactly — copy-paste the signature line and only change `count_jobs` → `count_jobs_by_status` and the return type.

- [ ] **Step 6: Verify the workspace builds**

Run: `cargo check --locked --workspace --tests`
Expected: PASS (no mismatched impls).

- [ ] **Step 7: Run the new test to verify it passes**

Run: `cargo test --locked --lib services::runtime::tests::sqlite_runtime_count_jobs_by_status_groups_per_state`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/services/runtime.rs src/services/runtime_tests.rs \
        src/services/search_crawl_tests.rs src/services/jobs_tests.rs \
        src/services/embed_tests.rs src/services/ingest_tests.rs \
        src/services/action_api_tests.rs src/services/crawl_tests.rs \
        src/web/server_test_support_tests.rs src/web/server/handlers/rest_tests.rs \
        src/cli/commands/ingest_tests.rs
git commit -m "feat(services): expose count_jobs_by_status on ServiceJobRuntime"
```

---

## Task 3: Typed summary structs — `ServiceJobSummary`, `StatusView`, `StatusSummary`

**Files:**
- Modify: `src/services/types/service.rs`

Locks down the new wire shapes before any rendering logic touches them. `ServiceJobSummary` is the row form returned by `subaction=jobs`; `StatusSummary` is the default payload.

- [ ] **Step 1: Add a failing test for the conversion + serialization**

Locate the existing test module in `src/services/types/service.rs` (search for `mod tests` or for `#[cfg(test)]` block). Append:

```rust
    #[test]
    fn service_job_summary_drops_heavy_blobs() {
        use super::{ServiceJob, ServiceJobSummary};
        let mut job = ServiceJob {
            id: uuid::Uuid::nil(),
            status: "completed".into(),
            created_at: chrono::DateTime::default(),
            updated_at: chrono::DateTime::default(),
            started_at: None,
            finished_at: None,
            error_text: None,
            url: Some("https://example.com/".into()),
            source_type: None,
            target: None,
            urls_json: Some(serde_json::json!(["heavy", "blob"])),
            result_json: Some(serde_json::json!({"x": 1})),
            config_json: Some(serde_json::json!({"y": 2})),
            attempt_count: 0,
            active_attempt_id: None,
            last_reclaimed_at: None,
            last_reclaimed_reason: None,
        };
        job.finished_at = Some(chrono::DateTime::default());

        let summary: ServiceJobSummary = (&job).into();
        let raw = serde_json::to_value(&summary).unwrap();
        for forbidden in ["result_json", "config_json", "urls_json"] {
            assert!(
                raw.get(forbidden).is_none(),
                "summary must not carry {forbidden}: {raw}"
            );
        }
        assert_eq!(raw["url"], "https://example.com/");
        assert_eq!(raw["status"], "completed");
    }

    #[test]
    fn status_summary_serializes_histogram_alphabetically() {
        use super::{StatusKindSummary, StatusSummary, StatusTotals};
        use crate::jobs::status::JobStatus;
        use std::collections::BTreeMap;

        let mut by_state = BTreeMap::new();
        by_state.insert(JobStatus::Pending.as_str().to_string(), 2);
        by_state.insert(JobStatus::Completed.as_str().to_string(), 5);
        let summary = StatusSummary {
            totals: StatusTotals { crawl: 7, extract: 0, embed: 0, ingest: 0 },
            crawl: StatusKindSummary { total: 7, by_state: by_state.clone() },
            extract: StatusKindSummary::default(),
            embed: StatusKindSummary::default(),
            ingest: StatusKindSummary::default(),
        };
        let v = serde_json::to_value(&summary).unwrap();
        assert_eq!(v["crawl"]["total"], 7);
        assert_eq!(v["crawl"]["by_state"]["pending"], 2);
        assert_eq!(v["crawl"]["by_state"]["completed"], 5);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --locked --lib services::types::service::tests::service_job_summary_drops_heavy_blobs`
Expected: FAIL — types not defined.

- [ ] **Step 3: Add the new types to `src/services/types/service.rs`**

Place these alongside `StatusTotals` and `StatusResult` (search for `pub struct StatusTotals`). Insert immediately after `StatusResult`:

```rust
/// View selector for `services::system::full_status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusView {
    /// Totals + per-state histogram per kind. No job rows. Cheap.
    Summary,
    /// Per-kind job rows, each stripped of `result_json` / `config_json` /
    /// `urls_json`. Honors a row cap (default 20) per kind.
    Jobs,
}

/// Per-kind summary: total count + per-state histogram.
///
/// `by_state` keys are the canonical `JobStatus::as_str()` values
/// (`"pending"`, `"running"`, `"completed"`, `"failed"`, `"canceled"`).
/// Missing keys mean zero.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StatusKindSummary {
    pub total: i64,
    pub by_state: std::collections::BTreeMap<String, i64>,
}

/// Default payload for `axon status` (subaction=summary).
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StatusSummary {
    pub totals: StatusTotals,
    pub crawl: StatusKindSummary,
    pub extract: StatusKindSummary,
    pub embed: StatusKindSummary,
    pub ingest: StatusKindSummary,
}

/// Trimmed `ServiceJob` returned by `subaction=jobs`. Drops `result_json`,
/// `config_json`, and `urls_json` — the three fields responsible for the
/// 374 KB single-line spill the original bug reported.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ServiceJobSummary {
    pub id: uuid::Uuid,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub error_text: Option<String>,
    pub url: Option<String>,
    pub source_type: Option<String>,
    pub target: Option<String>,
    pub attempt_count: i64,
    pub active_attempt_id: Option<String>,
    pub last_reclaimed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_reclaimed_reason: Option<String>,
}

impl From<&ServiceJob> for ServiceJobSummary {
    fn from(job: &ServiceJob) -> Self {
        Self {
            id: job.id,
            status: job.status.clone(),
            created_at: job.created_at,
            updated_at: job.updated_at,
            started_at: job.started_at,
            finished_at: job.finished_at,
            error_text: job.error_text.clone(),
            url: job.url.clone(),
            source_type: job.source_type.clone(),
            target: job.target.clone(),
            attempt_count: job.attempt_count,
            active_attempt_id: job.active_attempt_id.clone(),
            last_reclaimed_at: job.last_reclaimed_at,
            last_reclaimed_reason: job.last_reclaimed_reason.clone(),
        }
    }
}
```

- [ ] **Step 4: Run tests to verify both pass**

Run: `cargo test --locked --lib services::types::service::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/services/types/service.rs
git commit -m "feat(services): add StatusView, StatusSummary, ServiceJobSummary types"
```

---

## Task 4: Rework `services::system::status` to produce summary or jobs view

**Files:**
- Modify: `src/services/system/status.rs`
- Modify: `src/services/system/status_tests.rs`

`full_status` becomes view-aware. The summary path uses `count_jobs_by_status` (no row fetch). The jobs path keeps today's `load_status_jobs` work but downsamples to `ServiceJobSummary` before building the payload.

- [ ] **Step 1: Add the failing tests**

Append to `src/services/system/status_tests.rs`:

```rust
use super::*;
use crate::services::types::{StatusKindSummary, StatusView};

#[tokio::test]
async fn full_status_summary_view_omits_per_job_rows() {
    let ctx = build_test_service_context().await; // existing test helper
    seed_two_crawl_jobs(&ctx, "pending", "completed").await; // existing helper or inline equivalent

    let result = full_status(&ctx, StatusView::Summary).await.unwrap();
    let raw = serde_json::to_string(&result.payload).unwrap();

    assert!(!raw.contains("local_crawl_jobs"), "summary must not include row arrays");
    assert!(!raw.contains("result_json"), "summary must not include heavy blobs");
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(v["crawl"]["total"], 2);
    assert_eq!(v["crawl"]["by_state"]["pending"], 1);
    assert_eq!(v["crawl"]["by_state"]["completed"], 1);
}

#[tokio::test]
async fn full_status_jobs_view_strips_heavy_blobs() {
    let ctx = build_test_service_context().await;
    seed_two_crawl_jobs(&ctx, "completed", "completed").await;

    let result = full_status(&ctx, StatusView::Jobs).await.unwrap();
    let raw = serde_json::to_string(&result.payload).unwrap();

    assert!(raw.contains("local_crawl_jobs"));
    assert!(!raw.contains("result_json"), "jobs view must drop result_json blobs: {raw}");
    assert!(!raw.contains("config_json"), "jobs view must drop config_json blobs: {raw}");
    assert!(!raw.contains("urls_json"), "jobs view must drop urls_json blobs: {raw}");
}
```

If `build_test_service_context` / `seed_two_crawl_jobs` helpers do not already exist in this file, copy the construction pattern used by other tests in `src/services/system/status_tests.rs` for setting up an in-memory SQLite-backed `ServiceContext` and inserting raw rows via the runtime's pool. Do NOT introduce a new helper pattern.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --locked --lib services::system::status_tests::full_status_summary_view_omits_per_job_rows services::system::status_tests::full_status_jobs_view_strips_heavy_blobs`
Expected: FAIL — `full_status` does not yet take a `StatusView` argument.

- [ ] **Step 3: Replace `full_status` and `build_status_payload` in `src/services/system/status.rs`**

Replace the existing `full_status`, `load_status_jobs`, and `build_status_payload` block (current lines ~18–175) with the version below. Keep the `filter_and_view` helper and the `StatusJobs` struct unchanged.

```rust
use crate::services::types::{
    ServiceJob, ServiceJobSummary, StatusKindSummary, StatusResult, StatusSummary, StatusTotals,
    StatusView,
};

#[must_use = "full_status returns a Result that should be handled"]
pub async fn full_status(
    service_context: &ServiceContext,
    view: StatusView,
) -> Result<StatusResult, Box<dyn Error>> {
    match view {
        StatusView::Summary => summary_status(service_context).await,
        StatusView::Jobs => jobs_status(service_context).await,
    }
}

async fn summary_status(
    service_context: &ServiceContext,
) -> Result<StatusResult, Box<dyn Error>> {
    let (crawl, extract, embed, ingest) = tokio::join!(
        load_kind_summary(service_context, JobKind::Crawl),
        load_kind_summary(service_context, JobKind::Extract),
        load_kind_summary(service_context, JobKind::Embed),
        load_kind_summary(service_context, JobKind::Ingest),
    );

    let summary = StatusSummary {
        totals: StatusTotals {
            crawl: crawl.total,
            extract: extract.total,
            embed: embed.total,
            ingest: ingest.total,
        },
        crawl,
        extract,
        embed,
        ingest,
    };

    let text = [
        "Axon Status".to_string(),
        format!("crawl jobs:   {} total", summary.totals.crawl),
        format!("extract jobs: {} total", summary.totals.extract),
        format!("embed jobs:   {} total", summary.totals.embed),
        format!("ingest jobs:  {} total", summary.totals.ingest),
    ]
    .join("\n");

    let payload = serde_json::to_value(&summary)
        .map_err(|e| format!("status: encode summary: {e}"))?;
    Ok(StatusResult {
        payload,
        text,
        totals: summary.totals,
    })
}

async fn load_kind_summary(
    service_context: &ServiceContext,
    kind: JobKind,
) -> StatusKindSummary {
    let histogram = service_context
        .jobs
        .count_jobs_by_status(kind)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                kind = ?kind,
                error = %e,
                "status: count_jobs_by_status failed, defaulting to empty"
            );
            std::collections::HashMap::new()
        });
    let total: i64 = histogram.values().copied().sum();
    let by_state = histogram
        .into_iter()
        .map(|(status, count)| (status.as_str().to_string(), count))
        .collect();
    StatusKindSummary { total, by_state }
}

async fn jobs_status(
    service_context: &ServiceContext,
) -> Result<StatusResult, Box<dyn Error>> {
    let (jobs, totals) = load_status_jobs(service_context).await?;
    let payload = build_jobs_payload(
        &jobs.crawl,
        &jobs.extract,
        &jobs.embed,
        &jobs.ingest,
        &totals,
    );
    let text = [
        "Axon Status (jobs view)".to_string(),
        format!("crawl jobs:   {} total", totals.crawl),
        format!("extract jobs: {} total", totals.extract),
        format!("embed jobs:   {} total", totals.embed),
        format!("ingest jobs:  {} total", totals.ingest),
    ]
    .join("\n");
    Ok(StatusResult {
        payload,
        text,
        totals,
    })
}

/// Build the jobs-view payload using trimmed `ServiceJobSummary` rows.
///
/// Heavy fields (`result_json`, `config_json`, `urls_json`) are dropped here.
/// Callers that need them must use the per-family `<kind> status <id>` paths.
pub fn build_jobs_payload(
    crawl_jobs: &[ServiceJob],
    extract_jobs: &[ServiceJob],
    embed_jobs: &[ServiceJob],
    ingest_jobs: &[ServiceJob],
    totals: &StatusTotals,
) -> serde_json::Value {
    let to_summaries = |rows: &[ServiceJob]| -> Vec<ServiceJobSummary> {
        rows.iter().map(ServiceJobSummary::from).collect()
    };
    serde_json::json!({
        "local_crawl_jobs": to_summaries(crawl_jobs),
        "local_extract_jobs": to_summaries(extract_jobs),
        "local_embed_jobs": to_summaries(embed_jobs),
        "local_ingest_jobs": to_summaries(ingest_jobs),
        "totals": {
            "crawl": totals.crawl,
            "extract": totals.extract,
            "embed": totals.embed,
            "ingest": totals.ingest,
        },
    })
}
```

Notes:
- Keep `load_status_jobs`, `StatusJobs`, and `filter_and_view` as-is — they are still used by the CLI human renderer and now by `jobs_status`.
- Delete the old `pub fn build_status_payload` (it's replaced by `build_jobs_payload`). The CLI module currently imports it; Task 7 updates that import.

- [ ] **Step 4: Run the new tests to verify they pass**

Run: `cargo test --locked --lib services::system::status_tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/services/system/status.rs src/services/system/status_tests.rs
git commit -m "feat(services): view-aware full_status (Summary | Jobs) with trimmed rows"
```

---

## Task 5: Route `handle_status` on `subaction` (MCP)

**Files:**
- Modify: `src/mcp/server/handlers_system.rs`
- Modify: `src/mcp/schema/requests.rs`

Default to summary. Accept `subaction=summary` and `subaction=jobs`. Reject anything else with `invalid_params`. Update the dashboard widget call too — it stays on the jobs view.

- [ ] **Step 1: Drop the `#[allow(dead_code)]` and tighten the field doc in `src/mcp/schema/requests.rs`**

Replace:

```rust
pub struct StatusRequest {
    #[allow(dead_code)] // accepted for API compat but ignored by handlers
    pub subaction: Option<String>,
    pub response_mode: Option<ResponseMode>,
}
```

with:

```rust
pub struct StatusRequest {
    /// One of `"summary"` (default — totals + per-state histogram) or
    /// `"jobs"` (per-kind summary rows, no `result_json`/`config_json`).
    pub subaction: Option<String>,
    pub response_mode: Option<ResponseMode>,
}
```

- [ ] **Step 2: Add a failing test for the dispatch**

Append to `src/mcp/schema_tests.rs` (or wherever existing `parse_axon_request` tests live — search for `parse_axon_request` in `src/mcp/`):

```rust
#[test]
fn status_request_accepts_subaction_summary_and_jobs() {
    use super::*;
    let summary: AxonRequest =
        serde_json::from_value(serde_json::json!({"action": "status"})).unwrap();
    let jobs: AxonRequest =
        serde_json::from_value(serde_json::json!({"action": "status", "subaction": "jobs"}))
            .unwrap();
    assert!(matches!(summary, AxonRequest::Status(StatusRequest { subaction: None, .. })));
    assert!(matches!(
        jobs,
        AxonRequest::Status(StatusRequest { subaction: Some(ref s), .. }) if s == "jobs"
    ));
}
```

Run: `cargo test --locked --lib mcp::schema::tests::status_request_accepts_subaction_summary_and_jobs`
Expected: PASS (this exercises the schema, which already supports the field; we just made it semantic).

- [ ] **Step 3: Update `handle_status` in `src/mcp/server/handlers_system.rs`**

Replace the existing `handle_status` body (around `handlers_system.rs:357`):

```rust
    pub(super) async fn handle_status(
        &self,
        req: StatusRequest,
    ) -> Result<AxonToolResponse, ErrorData> {
        use crate::services::types::StatusView;
        let response_mode = req.response_mode;
        let view = match req.subaction.as_deref() {
            None | Some("") | Some("summary") => StatusView::Summary,
            Some("jobs") => StatusView::Jobs,
            Some(other) => {
                return Err(ErrorData::invalid_params(
                    format!(
                        "status: unknown subaction `{other}`; expected `summary` (default) or `jobs`"
                    ),
                    None,
                ));
            }
        };
        let subaction_label = match view {
            StatusView::Summary => "summary",
            StatusView::Jobs => "jobs",
        };
        let ctx = self
            .base_service_context()
            .await
            .map_err(|e| logged_internal_error("status", e.as_ref()))?;
        let result = system::full_status(&ctx, view)
            .await
            .map_err(|e| logged_internal_error("status", e.as_ref()))?;
        respond_with_mode(
            "status",
            subaction_label,
            response_mode,
            "status",
            result.payload,
            InlineHint::Document,
        )
        .await
    }
```

- [ ] **Step 4: Update the dashboard call (around `src/mcp/server.rs:460`) to explicitly request the jobs view**

Find the call site:

```rust
match system::full_status(&ctx).await {
```

Change to:

```rust
match system::full_status(&ctx, crate::services::types::StatusView::Jobs).await {
```

This preserves the widget today — the dashboard wants per-job detail.

- [ ] **Step 5: Verify the workspace builds and existing handler tests still pass**

Run: `cargo check --locked --workspace --tests`
Run: `cargo test --locked --lib mcp::`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/mcp/server/handlers_system.rs src/mcp/schema/requests.rs \
        src/mcp/schema_tests.rs src/mcp/server.rs
git commit -m "feat(mcp): route status on subaction; default to summary view"
```

---

## Task 6: CLI — default `axon status` to summary; add `axon status jobs`

**Files:**
- Modify: `src/cli/commands/status.rs`
- Modify: `src/cli/commands/status_tests.rs`

Keep today's rich per-job renderer reachable via `axon status jobs`. The default human output becomes the totals summary that already appears as the header — minus the rows.

- [ ] **Step 1: Add a failing test for the summary code path**

Append to `src/cli/commands/status_tests.rs`:

```rust
#[tokio::test]
async fn run_status_default_summary_does_not_call_jobs_renderer() {
    use crate::services::types::StatusView;
    let ctx = build_test_service_context().await; // existing helper
    let mut cfg = test_config();
    cfg.json_output = true;
    cfg.positional = vec![]; // default subcommand
    // capture stdout
    let captured = capture_stdout(|| async {
        run_status(&cfg, &ctx).await.unwrap();
    })
    .await;
    let v: serde_json::Value = serde_json::from_str(&captured).unwrap();
    assert!(v.get("local_crawl_jobs").is_none(), "default --json must be summary, not jobs: {captured}");
    assert!(v.get("totals").is_some());
    assert!(v.get("crawl").and_then(|c| c.get("by_state")).is_some());
}

#[tokio::test]
async fn run_status_jobs_subcommand_returns_jobs_payload() {
    let ctx = build_test_service_context().await;
    let mut cfg = test_config();
    cfg.json_output = true;
    cfg.positional = vec!["jobs".to_string()];
    let captured = capture_stdout(|| async {
        run_status(&cfg, &ctx).await.unwrap();
    })
    .await;
    let v: serde_json::Value = serde_json::from_str(&captured).unwrap();
    assert!(v.get("local_crawl_jobs").is_some());
}
```

If `capture_stdout` / `test_config` helpers do not exist in this file, scan the file for the conventions used by existing tests in `src/cli/commands/status_tests.rs`. If `run_status` currently writes via `println!` and no capture helper exists, switch the tests to call `status_snapshot` and `full_status` directly (which return values, no stdout) and assert on the returned `serde_json::Value` instead. The behavior under test is "summary by default; jobs when asked", not stdout capture.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --locked --lib cli::commands::status_tests::run_status_default_summary_does_not_call_jobs_renderer cli::commands::status_tests::run_status_jobs_subcommand_returns_jobs_payload`
Expected: FAIL.

- [ ] **Step 3: Rewrite `run_status` in `src/cli/commands/status.rs`**

Replace the existing `run_status` (lines ~19–33) and adjust the imports at the top:

```rust
use crate::services::system::{full_status, load_status_jobs};
use crate::services::types::StatusView;
```

Replace the function:

```rust
pub async fn run_status(
    cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let want_jobs = cfg
        .positional
        .first()
        .map(|p| p.as_str())
        .is_some_and(|p| p == "jobs");
    log_info(&format!(
        "command=status json={} view={}",
        cfg.json_output,
        if want_jobs { "jobs" } else { "summary" }
    ));
    if want_jobs {
        let view = StatusView::Jobs;
        if cfg.json_output {
            let result = full_status(service_context, view).await?;
            println!("{}", serde_json::to_string_pretty(&result.payload)?);
        } else {
            run_status_impl(cfg, service_context).await?;
        }
    } else if cfg.json_output {
        let result = full_status(service_context, StatusView::Summary).await?;
        println!("{}", serde_json::to_string_pretty(&result.payload)?);
    } else {
        let result = full_status(service_context, StatusView::Summary).await?;
        println!("{}", result.text);
    }
    Ok(())
}
```

`run_status_impl` (the rich per-job renderer) stays as-is and is reached only via `axon status jobs`.

- [ ] **Step 4: Update `status_snapshot` to use the new payload builder**

Replace the existing `status_snapshot` body so it returns the jobs payload (its existing semantic):

```rust
pub async fn status_snapshot(
    _cfg: &Config,
    service_context: &ServiceContext,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let (jobs, totals) = load_status_jobs(service_context).await?;
    Ok(crate::services::system::build_jobs_payload(
        &jobs.crawl,
        &jobs.extract,
        &jobs.embed,
        &jobs.ingest,
        &totals,
    ))
}
```

If `build_jobs_payload` is not yet re-exported from `crate::services::system`, add it to that module's re-export list (search `src/services/system.rs` for `pub use status::` and add `build_jobs_payload`).

- [ ] **Step 5: Update `render_status_payload` to deserialize `ServiceJobSummary` instead of `ServiceJob`**

Inside the same file:

```rust
    #[derive(serde::Deserialize)]
    struct StatusPayload {
        local_crawl_jobs: Vec<ServiceJobSummary>,
        local_extract_jobs: Vec<ServiceJobSummary>,
        local_embed_jobs: Vec<ServiceJobSummary>,
        local_ingest_jobs: Vec<ServiceJobSummary>,
    }
```

Add `use crate::services::types::ServiceJobSummary;` to the file's imports, and update the helper functions in `src/cli/commands/status/presentation.rs` (and `failure_summary.rs`, `metrics.rs`) that currently consume `&ServiceJob` to consume `&ServiceJobSummary` — only the heavy fields are missing, so adjusting the type alias should compile cleanly. If any of those helpers actually read `result_json`/`config_json`/`urls_json`, leave that helper consuming `&ServiceJob` and route only the trimmed payload through the renderer that doesn't need those blobs.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --locked --lib cli::commands::status_tests`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/cli/commands/status.rs src/cli/commands/status_tests.rs \
        src/cli/commands/status/presentation.rs src/cli/commands/status/failure_summary.rs \
        src/cli/commands/status/metrics.rs src/services/system.rs
git commit -m "feat(cli): default axon status to summary; gate jobs view behind 'axon status jobs'"
```

---

## Task 7: Docs

**Files:**
- Modify: `docs/MCP-TOOL-SCHEMA.md`
- Modify: `docs/MCP.md`
- Modify: `CLAUDE.md`

Document the new contract so callers don't have to guess.

- [ ] **Step 1: Update `docs/MCP-TOOL-SCHEMA.md`**

Find the `status` section. Replace its body with:

```markdown
### `status`

`subaction` (optional, default `summary`):

| Subaction | Returns |
|-----------|---------|
| `summary` (default) | `{ totals, crawl, extract, embed, ingest }` where each kind is `{ total, by_state: { pending, running, completed, failed, canceled } }`. No per-job rows. |
| `jobs` | `{ local_<kind>_jobs: ServiceJobSummary[], totals }`. Rows omit `result_json`, `config_json`, and `urls_json`. Use `<kind> status <id>` for the full blob. |

Unknown subactions → `invalid_params`.
```

- [ ] **Step 2: Update `docs/MCP.md`**

Search for `status` in the action map and add the same subaction table inline. Add an admonition under "Response Contract" noting that the default `status` payload is intentionally small (totals + histogram) and that callers who want per-job rows must pass `subaction=jobs`.

- [ ] **Step 3: Update the CLI section in `CLAUDE.md`**

In the `Commands` table, replace the `status` row with:

```markdown
| `status` [`jobs`] | Show job-queue health. Default = totals + per-state histogram. `axon status jobs` shows per-kind rows (no heavy blobs). | No |
```

- [ ] **Step 4: Commit**

```bash
git add docs/MCP-TOOL-SCHEMA.md docs/MCP.md CLAUDE.md
git commit -m "docs: document axon status subactions (summary default, jobs opt-in)"
```

---

## Task 8: Acceptance — exercise the MCP path and measure the payload

**Files:**
- None (verification only)

Acceptance from the bead: "Default `axon status` MCP invocation returns under 10K characters for a project with 100s of historical jobs."

- [ ] **Step 1: Full build + test**

Run: `cargo build --release --bin axon`
Run: `cargo test --locked --workspace`
Expected: PASS.

- [ ] **Step 2: Smoke against the running MCP server via mcporter**

Pre-req: a populated SQLite DB (the dev box already has 248 crawl / 174 embed / 96 ingest). Use `mcporter` per `src/mcp/CLAUDE.md` "Testing Workflow":

Run:
```bash
mcporter --config config/mcporter.json call axon.axon action:status --output json > /tmp/status-summary.json
wc -c /tmp/status-summary.json
mcporter --config config/mcporter.json call axon.axon action:status subaction:jobs --output json > /tmp/status-jobs.json
wc -c /tmp/status-jobs.json
mcporter --config config/mcporter.json call axon.axon action:status subaction:bogus --output json || true
```

Expected:
- `status-summary.json` is < 10000 bytes.
- `status-jobs.json` is dramatically smaller than the pre-change ~374 KB (target: well under 64 KB at the dev box's job volume) and contains no `"result_json"` / `"config_json"` / `"urls_json"` substrings.
- The `subaction:bogus` call returns an `invalid_params` error mentioning the accepted values.

- [ ] **Step 3: Confirm the artifact on disk is multi-line (already true; sanity check)**

Run:
```bash
ls -la ~/.axon/artifacts/axon/status/ | tail -3
LATEST=$(ls -t ~/.axon/artifacts/axon/status/ | head -1)
wc -l ~/.axon/artifacts/axon/status/"$LATEST"
```

Expected: line count > 5. This is `write_json_artifact`'s pre-existing `to_string_pretty` behavior — captured as evidence in case a future change regresses it.

- [ ] **Step 4: Close the bead**

Run:
```bash
bd close axon_rust-9pbb --reason "axon status default trimmed to summary view; jobs view kept under subaction=jobs"
```

- [ ] **Step 5: Final push**

```bash
bd dolt push
git push
git status   # must say up to date with origin
```

---

## Self-Review

**Spec coverage (bead axon_rust-9pbb):**
- "Default returns totals + per-family histogram" → Tasks 3, 4, 5.
- "subaction=jobs returns rows without heavy blobs" → Tasks 3, 4, 5.
- "subaction=job for full blob" → **intentionally NOT added**; existing per-family `<kind> status <id>` already covers this. Architecture section calls this out.
- "Pretty-print spill artifacts" → **dropped from scope** (already pretty); Architecture section + Task 8 Step 3 document why.
- "Apply `limit` to row count AND payload size" → covered by the row trim (payload size is now bounded by the number of rows + a constant row size after blob removal). No explicit `limit` parameter wiring is added in this plan; if the dev box ever produces 10K+ rows of trimmed data, file a follow-up to push `limit` down into `load_status_jobs`.

**Placeholder scan:** No `TBD`, `TODO`, `implement later`, "appropriate error handling", or vague "similar to Task N" references. Every code change is shown inline.

**Type consistency:**
- `count_jobs_by_status` returns `HashMap<JobStatus, i64>` everywhere (Tasks 1, 2, 4).
- `StatusView::Summary` / `StatusView::Jobs` used identically in service, MCP handler, CLI handler, dashboard call (Tasks 3, 4, 5, 6).
- `build_jobs_payload` (Task 4) is the only payload builder for the jobs view; `status_snapshot` (Task 6) calls it.
- `StatusKindSummary { total, by_state }` shape matches between the type definition (Task 3), service emission (Task 4), and docs (Task 7).
- `ServiceJobSummary` is used for both the wire shape and the CLI deserialization (Tasks 3, 6).

---

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-23-axon-status-trim.md`. Two execution options:

**1. Subagent-Driven (recommended)** — dispatch a fresh subagent per task, review between tasks.

**2. Inline Execution** — execute tasks in this session using executing-plans, batch with checkpoints.

Which approach?
