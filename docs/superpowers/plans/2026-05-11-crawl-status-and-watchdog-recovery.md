# Crawl Status and Watchdog Recovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `axon status` surface active crawl work before newest pending rows, make truncated output say so explicitly, and tighten the watchdog loop so reclaimed crawl rows re-notify workers promptly instead of waiting up to 5 seconds.

**Architecture:** Keep the current SQLite-backed queue and in-process worker model unchanged. Change the crawl status query path so rows are ordered by operational importance, thread the existing `StatusTotals` count through the renderers so the output can say "showing N of M", and reduce the watchdog sweep interval from 60s to 15s with a single post-reclaim `notify_one()` call to all four worker channels when stale rows are found. Anchor the reclaim error string as a shared constant before touching any of the other files so all three usage sites stay in sync.

**Tech Stack:** Rust, Tokio, sqlx/SQLite, serde_json, cargo test

---

## File structure

- `src/jobs/lite/store.rs` — add `pub(crate) const RECLAIMED_ERROR_TEXT` anchoring the reclaim string written into `error_text`; change the SQL UPDATE to use `.bind(RECLAIMED_ERROR_TEXT)` instead of interpolating it via `format!`.
- `src/jobs/lite/query.rs` — inline a CASE-status ORDER BY match in `list_service_jobs()` (no helper function); switch limit/offset from `format!()` to `.bind()`.
- `src/cli/commands/status.rs` — add `section_note: Option<&str>` param to `write_status_section()`; thread `crawl_total: i64` to `render_status_jobs_from_slices()` and down to the crawl section; update `render_status_payload` to extract `totals.crawl` from the JSON value; update `run_status_impl` to use `totals.crawl`; use `RECLAIMED_ERROR_TEXT` constant in `job_error_hint`; update/add tests.
- `src/jobs/lite/workers.rs` — add `const WATCHDOG_SWEEP_INTERVAL` at module level (near `POLL_INTERVAL`); reduce watchdog interval to 15s; call `reclaim_stale_running_jobs` then `notify_one()` on all four channels when `total > 0`; replace `if let Err` with explicit match; add `biased;` to the `select!`.

---

## Notes

- Do **not** redesign the queue model or add new persistence tables.
- Keep the existing JSON payload keys (`local_crawl_jobs`, `totals`, etc.) stable.
- `WATCHDOG_RECLAIM_PREFIX` in `services/system.rs:20` is a completely separate constant for a **different** string format (`"watchdog reclaimed stale running "`) used by the legacy full-mode `mark_failed` path. Do **not** touch it; do not attempt to unify with it.
- `StatusTotals` does not derive `serde::Deserialize`; extract `totals.crawl` from `serde_json::Value` directly instead of adding a derive.
- The `service_job_order_by` helper seen in earlier drafts is unnecessary — inline the match directly in `list_service_jobs`. No separate function.
- Secondary sort for crawl must be `created_at DESC, updated_at DESC, id` within each status group — **not** `COALESCE(started_at, created_at) ASC` (which would surface oldest completions first, a regression).
- The SQL UPDATE in `reclaim_stale_running_jobs_for_table` must use `.bind(RECLAIMED_ERROR_TEXT)` for `error_text` — not `format!("... error_text='{}' ...", RECLAIMED_ERROR_TEXT)`. Status comparisons (`status='running'`, `status='pending'`) remain as inline literals since they come from a closed enum.

---

### Task 0: Regression-test job_error_hint before touching any code

**Files:**
- Test: `src/cli/commands/status.rs` (tests module, after line 410)

`job_error_hint` at `status.rs:339-351` already handles `status=="pending"` with reclaim `error_text` and emits a human-friendly message. This task writes a regression test that locks in that behavior **before** any refactoring so subsequent tasks can't silently break it.

- [ ] **Step 1: Add the regression test**

In the `#[cfg(test)] mod tests` block of `src/cli/commands/status.rs`, add after the existing test:

```rust
#[test]
fn render_status_payload_surfaces_reclaimed_pending_crawl_rows() {
    let mut reclaimed = job("pending");
    reclaimed.error_text = Some("reclaimed after unexpected shutdown".to_string());
    reclaimed.result_json = None;

    let payload = build_status_payload(
        &[reclaimed],
        &[],
        &[],
        &[],
        &crate::services::types::StatusTotals::default(),
    );

    let rendered = render_status_payload(&payload).expect("payload should render");

    // job_error_hint translates the raw marker into a human-friendly message
    assert!(
        rendered.contains("recovered after worker shutdown"),
        "expected reclaim hint; got:\n{rendered}"
    );
    // the raw internal marker string must NOT leak into user-facing output
    assert!(
        !rendered.contains("reclaimed after unexpected shutdown"),
        "raw reclaim marker leaked into output:\n{rendered}"
    );
}
```

- [ ] **Step 2: Run the test to verify it passes immediately**

Run: `cargo test render_status_payload_surfaces_reclaimed_pending_crawl_rows -- --nocapture`

Expected: PASS — this is a regression test proving existing behavior. If it fails, the current `job_error_hint` is broken and must be fixed before proceeding.

- [ ] **Step 3: Commit**

```bash
git add src/cli/commands/status.rs
git commit -m "test: regression test for reclaimed crawl row display"
```

---

### Task 1: Anchor the reclaim string as a shared constant

**Files:**
- Modify: `src/jobs/lite/store.rs` (above `reclaim_stale_running_jobs` at line 88; SQL at lines 129-137)
- Modify: `src/cli/commands/status.rs:340`
- Modify: `src/jobs/lite/ops/tests.rs:54`

The string `"reclaimed after unexpected shutdown"` appears in three places with no shared source of truth. A one-character change in any one silently breaks the renderer with no compile error. This task adds the constant first.

- [ ] **Step 1: Add the constant and update the SQL UPDATE to use .bind()**

In `src/jobs/lite/store.rs`, add this line immediately above `pub async fn reclaim_stale_running_jobs` (currently at line 88):

```rust
pub(crate) const RECLAIMED_ERROR_TEXT: &str = "reclaimed after unexpected shutdown";
```

Then update `reclaim_stale_running_jobs_for_table` to use a bind parameter for `error_text` rather than interpolating the constant into the SQL string. The current code at lines 129-137 reads:

```rust
let result = sqlx::query(&format!(
    "UPDATE {} SET status='pending', error_text='reclaimed after unexpected shutdown', \
     updated_at=? WHERE status='running' AND updated_at < ?",
    table
))
.bind(now_ms())
.bind(threshold)
.execute(pool)
.await?;
```

Replace with:

```rust
let result = sqlx::query(&format!(
    "UPDATE {} SET status='pending', error_text=?, \
     updated_at=? WHERE status='running' AND updated_at < ?",
    table
))
.bind(RECLAIMED_ERROR_TEXT)
.bind(now_ms())
.bind(threshold)
.execute(pool)
.await?;
```

Note: `table` is the only value that must stay in `format!()` because sqlx cannot bind table names. Status string literals (`status='running'`, `status='pending'`) remain as inline literals — they come from a closed enum and are never user-controlled.

- [ ] **Step 2: Update status.rs to use the constant**

In `src/cli/commands/status.rs`, add the import with the other `use crate::jobs` imports near the top:

```rust
use crate::jobs::lite::store::RECLAIMED_ERROR_TEXT;
```

Then in `job_error_hint` (line 340), change the literal comparison:

```rust
// before
if error_text.trim_start() == "reclaimed after unexpected shutdown" {

// after
if error_text.trim_start() == RECLAIMED_ERROR_TEXT {
```

- [ ] **Step 3: Update ops/tests.rs to use the constant**

In `src/jobs/lite/ops/tests.rs`, add an import at the top of the `mod tests` block:

```rust
use crate::jobs::lite::store::RECLAIMED_ERROR_TEXT;
```

Then change the INSERT at line 54. The current query ends with:
```rust
"VALUES (?, 'pending', 'docs', '{}', 1, 1, 'reclaimed after unexpected shutdown')",
)
.bind(&id)
```

Change to bind the constant instead:
```rust
"VALUES (?, 'pending', 'docs', '{}', 1, 1, ?)",
)
.bind(&id)
.bind(RECLAIMED_ERROR_TEXT)
```

- [ ] **Step 4: Update the regression test to use the constant**

In the test added in Task 0, change the literal to use the constant. Import it at the top of the `mod tests` block if not already imported:

```rust
use crate::jobs::lite::store::RECLAIMED_ERROR_TEXT;
```

Then change the test setup:

```rust
reclaimed.error_text = Some(RECLAIMED_ERROR_TEXT.to_string());
```

And the negative assertion:

```rust
assert!(
    !rendered.contains(RECLAIMED_ERROR_TEXT),
    "raw reclaim marker leaked into output:\n{rendered}"
);
```

- [ ] **Step 5: Compile-check the changed files**

Run: `cargo check 2>&1 | grep -E "^error"`

Expected: no errors.

- [ ] **Step 6: Run the regression test**

Run: `cargo test render_status_payload_surfaces_reclaimed_pending_crawl_rows -- --nocapture`

Expected: PASS (same behavior, now using the constant).

- [ ] **Step 7: Commit**

```bash
git add src/jobs/lite/store.rs src/cli/commands/status.rs src/jobs/lite/ops/tests.rs
git commit -m "refactor: anchor reclaim error string as shared constant"
```

---

### Task 2: Prioritize active crawl rows in the status query

**Files:**
- Modify: `src/jobs/lite/query.rs:255-269`
- Test: `src/jobs/lite/query.rs` (tests module, currently ends near line 424)

- [ ] **Step 1: Write the failing query-ordering test**

In the `#[cfg(test)] mod tests` block at the bottom of `src/jobs/lite/query.rs`, add:

```rust
#[tokio::test]
async fn list_service_jobs_prioritizes_running_crawl_rows_over_newer_pending_rows() {
    let pool = open_sqlite_pool(":memory:").await.unwrap();
    let older_running = Uuid::new_v4().to_string();
    let newer_pending = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO axon_crawl_jobs (id, status, url, config_json, created_at, updated_at, started_at) \
         VALUES (?, 'running', 'https://running.example', '{}', ?, ?, ?)",
    )
    .bind(&older_running)
    .bind(1_000_i64)
    .bind(1_000_i64)
    .bind(1_000_i64)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO axon_crawl_jobs (id, status, url, config_json, created_at, updated_at) \
         VALUES (?, 'pending', 'https://pending.example', '{}', ?, ?)",
    )
    .bind(&newer_pending)
    .bind(2_000_i64)
    .bind(2_000_i64)
    .execute(&pool)
    .await
    .unwrap();

    let jobs = list_service_jobs(&pool, JobKind::Crawl, 20, 0).await.unwrap();

    assert_eq!(jobs[0].id.to_string(), older_running);
    assert_eq!(jobs[0].status, "running");
    assert_eq!(jobs[1].id.to_string(), newer_pending);
    assert_eq!(jobs[1].status, "pending");
}
```

- [ ] **Step 2: Run the test to verify it currently fails**

Run: `cargo test list_service_jobs_prioritizes_running_crawl_rows_over_newer_pending_rows -- --nocapture`

Expected: FAIL — current code uses `ORDER BY created_at DESC` so the newer pending row appears first.

- [ ] **Step 3: Rewrite list_service_jobs with inline CASE ORDER BY**

Replace the `list_service_jobs` function (lines 255–269 of `src/jobs/lite/query.rs`) with:

```rust
pub async fn list_service_jobs(
    pool: &SqlitePool,
    kind: JobKind,
    limit: i64,
    offset: i64,
) -> Result<Vec<ServiceJob>, sqlx::Error> {
    let order_by = match kind {
        JobKind::Crawl => {
            "ORDER BY CASE status \
                WHEN 'running' THEN 0 \
                WHEN 'pending' THEN 1 \
                WHEN 'completed' THEN 2 \
                WHEN 'failed' THEN 3 \
                WHEN 'canceled' THEN 4 \
                ELSE 5 \
             END, \
             created_at DESC, \
             updated_at DESC, \
             id"
        }
        _ => "ORDER BY created_at DESC, updated_at DESC, id",
    };
    let query = format!(
        "{} {} LIMIT ?1 OFFSET ?2",
        service_select_from(kind),
        order_by,
    );
    let rows: Vec<ServiceJobTuple> = sqlx::query_as(&query)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(service_job_from_tuple).collect())
}
```

Secondary sort is `created_at DESC, updated_at DESC, id` — this matches the `list_ingest_service_jobs` pattern and ensures recent jobs surface within each status group. `COALESCE(started_at, created_at)` would surface oldest completions first and is not used here.

`LIMIT ?1 OFFSET ?2` uses numbered positional parameters consistent with the sibling `list_ingest_service_jobs` function (which uses `?1`, `?2`, `?3`).

- [ ] **Step 4: Run the focused query tests**

Run: `cargo test jobs::lite::query::tests -- --nocapture`

Expected: PASS on all tests including the new ordering test.

- [ ] **Step 5: Commit**

```bash
git add src/jobs/lite/query.rs
git commit -m "fix: prioritize active crawl jobs in status query"
```

---

### Task 3: Make status output say when it is showing only a slice

**Files:**
- Modify: `src/cli/commands/status.rs:89-151` (`render_status_jobs_from_slices`), line 85 (`render_status_jobs`), line 58 (`run_status_impl`), line 67 (`render_status_payload`), line 292 (`write_status_section`)
- Test: `src/cli/commands/status.rs` (tests module)

- [ ] **Step 1: Write the failing renderer test**

In the `#[cfg(test)] mod tests` block of `src/cli/commands/status.rs`, add:

```rust
#[test]
fn render_status_payload_mentions_when_crawl_rows_are_truncated() {
    let payload = build_status_payload(
        &[job("running"), job("pending")],
        &[],
        &[],
        &[],
        &crate::services::types::StatusTotals {
            crawl: 24,
            ..Default::default()
        },
    );

    let rendered = render_status_payload(&payload).expect("payload should render");

    assert!(
        rendered.contains("showing 2 of 24"),
        "expected truncation note; got:\n{rendered}"
    );
    assert!(
        rendered.contains("running jobs listed first"),
        "expected ordering note; got:\n{rendered}"
    );
}
```

- [ ] **Step 2: Run the test to verify it currently fails**

Run: `cargo test render_status_payload_mentions_when_crawl_rows_are_truncated -- --nocapture`

Expected: FAIL — current renderer ignores `totals` and emits no truncation note.

- [ ] **Step 3: Add `section_note` to `write_status_section`**

`write_status_section` currently has this signature (line 292):

```rust
fn write_status_section(
    out: &mut String,
    title: &str,
    jobs: &[ServiceJob],
    label_for: impl Fn(&ServiceJob) -> String,
    progress_for: impl Fn(&ServiceJob) -> Option<String>,
)
```

Add a `section_note: Option<&str>` parameter after `title` and emit it when present, before any job lines:

```rust
fn write_status_section(
    out: &mut String,
    title: &str,
    section_note: Option<&str>,
    jobs: &[ServiceJob],
    label_for: impl Fn(&ServiceJob) -> String,
    progress_for: impl Fn(&ServiceJob) -> Option<String>,
) {
    let _ = writeln!(out, "{}", primary(title));
    if let Some(note) = section_note {
        let _ = writeln!(out, "  {}", muted(note));
    }
    if jobs.is_empty() {
        let _ = writeln!(out, "  {}", muted("None."));
        let _ = writeln!(out);
        return;
    }
    for job in jobs.iter().take(10) {
        let label = label_for(job);
        if let Some(p) = progress_for(job) {
            let _ = writeln!(
                out,
                "  {} {} {} {}  {}",
                symbol_for_status(&job.status),
                human_status_text(&job.status),
                label,
                muted(&job.id.to_string()),
                muted(&p),
            );
        } else {
            let _ = writeln!(
                out,
                "  {} {} {} {}",
                symbol_for_status(&job.status),
                human_status_text(&job.status),
                label,
                muted(&job.id.to_string()),
            );
        }
        if let Some(err) = job
            .error_text
            .as_deref()
            .and_then(|err| job_error_hint(&job.status, err))
        {
            let _ = writeln!(out, "    {}", muted(&err));
        }
    }
    let _ = writeln!(out);
}
```

- [ ] **Step 4: Update `render_status_jobs_from_slices` to accept and emit the crawl note**

Change the function signature to add `crawl_total: i64` and compute the note inline:

```rust
fn render_status_jobs_from_slices(
    crawl_jobs: &[ServiceJob],
    extract_jobs: &[ServiceJob],
    embed_jobs: &[ServiceJob],
    ingest_jobs: &[ServiceJob],
    crawl_total: i64,
) -> String {
    let crawl_url_map: HashMap<uuid::Uuid, &str> = crawl_jobs
        .iter()
        .filter_map(|job| {
            let url = job.url.as_deref()?;
            Some((job.id, url))
        })
        .collect();
    let embed_jobs_by_id: HashMap<String, &ServiceJob> = embed_jobs
        .iter()
        .map(|job| (job.id.to_string(), job))
        .collect();
    let embed_doc_totals = embed_doc_totals_from_crawls(crawl_jobs);
    let mut out = String::new();
    let crawl_note = (crawl_total > crawl_jobs.len() as i64).then(|| {
        format!(
            "showing {} of {} total · running jobs listed first",
            crawl_jobs.len(),
            crawl_total,
        )
    });
    write_status_section(
        &mut out,
        "Crawl",
        crawl_note.as_deref(),
        crawl_jobs,
        |job| job.url.clone().unwrap_or_else(|| job.id.to_string()),
        |job| crawl_progress_summary(job, &embed_jobs_by_id, &embed_doc_totals),
    );
    write_status_section(
        &mut out,
        "Extract",
        None,
        extract_jobs,
        |job| {
            job.urls_json
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| job.id.to_string())
        },
        extract_progress_summary,
    );
    write_status_section(
        &mut out,
        "Embed",
        None,
        embed_jobs,
        |job| {
            job.target
                .as_deref()
                .map(|target| metrics::display_embed_input(target, &crawl_url_map).into_owned())
                .unwrap_or_else(|| job.id.to_string())
        },
        |job| embed_progress_summary(job, embed_doc_totals.get(&job.id.to_string()).copied()),
    );
    write_status_section(
        &mut out,
        "Ingest",
        None,
        ingest_jobs,
        |job| match (&job.source_type, &job.target) {
            (Some(source_type), Some(target)) => format!("{source_type}: {target}"),
            (_, Some(target)) => target.clone(),
            _ => job.id.to_string(),
        },
        ingest_progress_summary,
    );
    out
}
```

- [ ] **Step 5: Update `render_status_jobs` and `run_status_impl`**

Change `render_status_jobs` (line 85):

```rust
fn render_status_jobs(jobs: &crate::services::system::StatusJobs, crawl_total: i64) -> String {
    render_status_jobs_from_slices(
        &jobs.crawl,
        &jobs.extract,
        &jobs.embed,
        &jobs.ingest,
        crawl_total,
    )
}
```

Update `run_status_impl` (line 58) — it currently discards `_totals`:

```rust
async fn run_status_impl(
    _cfg: &Config,
    service_context: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let (jobs, totals) = load_status_jobs(service_context).await?;
    print!("{}", render_status_jobs(&jobs, totals.crawl));
    Ok(())
}
```

- [ ] **Step 6: Update `render_status_payload` to extract totals.crawl from JSON**

`StatusTotals` does not derive `serde::Deserialize`, so extract via `serde_json::Value::get`. The `build_status_payload` function serializes totals as `"totals": {"crawl": N, ...}`. Replace the `render_status_payload` function (line 67):

```rust
pub(crate) fn render_status_payload(payload: &serde_json::Value) -> Result<String, Box<dyn Error>> {
    #[derive(serde::Deserialize)]
    struct StatusPayload {
        local_crawl_jobs: Vec<ServiceJob>,
        local_extract_jobs: Vec<ServiceJob>,
        local_embed_jobs: Vec<ServiceJob>,
        local_ingest_jobs: Vec<ServiceJob>,
    }

    let crawl_total = payload
        .get("totals")
        .and_then(|t| t.get("crawl"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let p: StatusPayload = serde_json::from_value(payload.clone())?;
    Ok(render_status_jobs_from_slices(
        &p.local_crawl_jobs,
        &p.local_extract_jobs,
        &p.local_embed_jobs,
        &p.local_ingest_jobs,
        crawl_total,
    ))
}
```

- [ ] **Step 7: Update the existing parity test**

The existing test `render_status_payload_matches_local_renderer` calls `render_status_jobs(&jobs)`. After the signature change, update it to pass `crawl_total`:

```rust
#[test]
fn render_status_payload_matches_local_renderer() {
    let jobs = StatusJobs {
        crawl: vec![job("completed")],
        extract: Vec::new(),
        embed: vec![job("completed")],
        ingest: Vec::new(),
    };
    let totals = crate::services::types::StatusTotals::default(); // crawl: 0

    let payload = build_status_payload(
        &jobs.crawl,
        &jobs.extract,
        &jobs.embed,
        &jobs.ingest,
        &totals,
    );

    let from_jobs = render_status_jobs(&jobs, totals.crawl);
    let from_payload = render_status_payload(&payload).expect("payload should render");

    assert_eq!(from_payload, from_jobs);
    assert!(from_payload.contains("Crawl"));
    assert!(from_payload.contains("Embed"));
    assert!(from_payload.contains("2 docs"));
}
```

- [ ] **Step 8: Run all status renderer tests**

Run: `cargo test render_status_payload -- --nocapture`

Expected: PASS for all three tests — parity test, new truncation test, and the Task 0 regression test.

- [ ] **Step 9: Commit**

```bash
git add src/cli/commands/status.rs
git commit -m "fix: show truncation note in crawl status output"
```

---

### Task 4: Add a reclaim detail test and verify the store

**Files:**
- Test: `src/jobs/lite/store.rs` (tests module)

`reclaim_stale_running_jobs_for_table` already writes the correct `error_text` and resets status to `pending`. This task adds a test verifying the exact text and that fresh/pending rows are not touched — needed by `job_error_hint` in the renderer.

- [ ] **Step 1: Verify the existing reclaim test passes**

Run: `cargo test reclaim_stale_running_jobs_only_reclaims_stale_running_rows -- --nocapture`

Expected: PASS. Locks in current reclaim semantics.

- [ ] **Step 2: Write the reclaim-detail test**

In `src/jobs/lite/store.rs`, add `use super::RECLAIMED_ERROR_TEXT;` to the `#[cfg(test)] mod tests` block. Then add:

```rust
#[tokio::test]
async fn reclaim_stale_running_jobs_for_table_sets_reclaim_error_text() {
    let pool = open_sqlite_pool(":memory:").await.expect("pool");
    let stale_updated_at = now_ms() - 10_000;

    let stale_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO axon_crawl_jobs \
         (id, status, url, config_json, created_at, updated_at, started_at) \
         VALUES (?, 'running', 'https://stale.example', '{}', ?, ?, ?)",
    )
    .bind(&stale_id)
    .bind(stale_updated_at)
    .bind(stale_updated_at)
    .bind(stale_updated_at)
    .execute(&pool)
    .await
    .unwrap();

    // Fresh running row — must not be reclaimed
    let fresh_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO axon_crawl_jobs \
         (id, status, url, config_json, created_at, updated_at) \
         VALUES (?, 'running', 'https://fresh.example', '{}', ?, ?)",
    )
    .bind(&fresh_id)
    .bind(now_ms())
    .bind(now_ms())
    .execute(&pool)
    .await
    .unwrap();

    // Pending row — must not be touched
    let pending_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO axon_crawl_jobs \
         (id, status, url, config_json, created_at, updated_at) \
         VALUES (?, 'pending', 'https://pending.example', '{}', ?, ?)",
    )
    .bind(&pending_id)
    .bind(stale_updated_at)
    .bind(stale_updated_at)
    .execute(&pool)
    .await
    .unwrap();

    let reclaimed = reclaim_stale_running_jobs_for_table(&pool, JobKind::Crawl, 5_000)
        .await
        .expect("reclaim");

    assert_eq!(reclaimed, 1, "only the stale running row should be reclaimed");

    let (status, error_text): (String, Option<String>) =
        sqlx::query_as("SELECT status, error_text FROM axon_crawl_jobs WHERE id = ?")
            .bind(&stale_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "pending");
    assert_eq!(error_text.as_deref(), Some(RECLAIMED_ERROR_TEXT));

    let fresh_status: String =
        sqlx::query_scalar("SELECT status FROM axon_crawl_jobs WHERE id = ?")
            .bind(&fresh_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(fresh_status, "running", "fresh row must not be reclaimed");

    let pending_status: String =
        sqlx::query_scalar("SELECT status FROM axon_crawl_jobs WHERE id = ?")
            .bind(&pending_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(pending_status, "pending", "pending row must not be touched");
}
```

- [ ] **Step 3: Run the new store test**

Run: `cargo test reclaim_stale_running_jobs_for_table_sets_reclaim_error_text -- --nocapture`

Expected: PASS — the reclaim behavior and error text are already correct. This is a regression test.

- [ ] **Step 4: Commit**

```bash
git add src/jobs/lite/store.rs
git commit -m "test: verify reclaim error_text is set correctly"
```

---

### Task 5: Tighten the watchdog loop and re-notify workers after reclaim

**Files:**
- Modify: `src/jobs/lite/workers.rs:38-39` (near `POLL_INTERVAL` and `WORKER_BATCH_LIMIT`)
- Modify: `src/jobs/lite/workers.rs:161-194` (watchdog block)

The current watchdog sweeps all tables every 60s but never wakes workers after reclaiming stale rows. Reclaimed jobs wait up to 5 seconds (`POLL_INTERVAL`) before a worker picks them up. This task: (1) reduces sweep interval to 15s, (2) adds `notify_one()` on all four channels when any reclaim occurs, (3) adds `biased;` so shutdown deterministically wins, (4) uses a `match` for the reclaim result (required because `?` inside `tokio::spawn(async move { ... })` returning `()` is a compile error).

- [ ] **Step 1: Add WATCHDOG_SWEEP_INTERVAL at module level**

In `src/jobs/lite/workers.rs`, lines 38-39 currently read:

```rust
const POLL_INTERVAL: Duration = Duration::from_secs(5);
const WORKER_BATCH_LIMIT: usize = 32;
```

Add the new constant immediately below:

```rust
const POLL_INTERVAL: Duration = Duration::from_secs(5);
const WORKER_BATCH_LIMIT: usize = 32;
const WATCHDOG_SWEEP_INTERVAL: Duration = Duration::from_secs(15);
```

- [ ] **Step 2: Replace the watchdog block**

In `src/jobs/lite/workers.rs`, replace the entire watchdog block (lines 161–194, the comment and the `{ ... }` block) with:

```rust
// Periodic watchdog: sweeps all job tables every 15s while the process is alive.
// Pairs with HeartbeatGuard — heartbeat keeps updated_at fresh for live jobs;
// watchdog reclaims rows whose updated_at has gone stale (process died, runner
// panicked, etc.). After any reclaim, wakes all four worker channels so pending
// rows are picked up within milliseconds rather than waiting for POLL_INTERVAL.
{
    let pool = Arc::clone(&pool);
    let cfg_for_watchdog = Arc::clone(&cfg);
    let shutdown = shutdown.clone();
    let crawl_notify_wd = Arc::clone(&crawl_notify);
    let embed_notify_wd = Arc::clone(&embed_notify);
    let extract_notify_wd = Arc::clone(&extract_notify);
    let ingest_notify_wd = Arc::clone(&ingest_notify);
    worker_handles.push(tokio::spawn(async move {
        let stale_threshold_ms = (cfg_for_watchdog.watchdog_stale_timeout_secs
            + cfg_for_watchdog.watchdog_confirm_secs)
            .max(0)
            * 1_000i64;
        let mut ticker = tokio::time::interval(WATCHDOG_SWEEP_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // Skip immediate tick — startup-time reclaim already ran in LiteBackend::init.
        ticker.tick().await;
        loop {
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => break,
                _ = ticker.tick() => {
                    match crate::jobs::lite::store::reclaim_stale_running_jobs(
                        &pool,
                        stale_threshold_ms,
                    )
                    .await
                    {
                        Ok(total) if total > 0 => {
                            tracing::info!(
                                reclaimed = total,
                                "watchdog: reclaimed stale jobs, waking workers"
                            );
                            crawl_notify_wd.notify_one();
                            embed_notify_wd.notify_one();
                            extract_notify_wd.notify_one();
                            ingest_notify_wd.notify_one();
                        }
                        Ok(_) => {}
                        Err(e) => {
                            tracing::warn!(error = %e, "watchdog: periodic reclaim failed");
                        }
                    }
                }
            }
        }
    }));
}
```

Key choices:
- `biased;` — ensures shutdown is checked first when both futures are ready simultaneously. Documented Tokio idiom for this pattern.
- `notify_one()` — each call is on a different `Arc<Notify>` (one per job kind). `notify_one()` is correct here because it stores a permit; if no worker is currently parked on `.notified().await`, the permit is consumed on the next poll. `notify_waiters()` does not store a permit and would silently drop the wakeup if workers are between polls.
- `match` instead of `if let Err` — allows acting on `Ok(total > 0)`. Using `.await?` would be a compile error because the spawn closure returns `()`.
- `MissedTickBehavior::Delay` — if the system is briefly under load and a tick is missed, the next tick fires 15s later rather than immediately. Prevents burst reclaim sweeps after temporary CPU saturation.

- [ ] **Step 3: Run the reclaim and heartbeat tests**

Run:
```bash
cargo test reclaim_stale_running_jobs -- --nocapture
cargo test touch_heartbeat_advances_updated_at_only_on_running_rows -- --nocapture
```

Expected: PASS on both. Reclaim semantics stay intact; heartbeat behavior is unchanged.

- [ ] **Step 4: Commit**

```bash
git add src/jobs/lite/workers.rs
git commit -m "fix: re-notify workers after stale job reclaim; reduce watchdog interval to 15s"
```

---

### Task 6: Full verification

**Files:**
- Modify: none
- Test: all touched modules

- [ ] **Step 1: Run all focused unit suites**

Run each in order:

```bash
cargo test jobs::lite::query::tests -- --nocapture
cargo test reclaim_stale_running_jobs -- --nocapture
cargo test touch_heartbeat_advances_updated_at_only_on_running_rows -- --nocapture
cargo test render_status_payload -- --nocapture
```

Expected: all PASS.

- [ ] **Step 2: Run the broader compile + lint gate**

Run:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test status -- --nocapture
```

Expected: clean fmt, zero new clippy warnings, all status-related tests pass.

- [ ] **Step 3: Live smoke check against the running container**

Run:

```bash
./scripts/axon status
docker logs --tail 50 axon
```

Expected: `status` shows running crawls at the top of the Crawl section; if any reclaimed rows exist they are labeled "recovered after worker shutdown" rather than showing the raw internal marker; if there are more than 10 crawl rows, the header says "showing N of M total · running jobs listed first".

- [ ] **Step 4: Commit the verification pass and push**

```bash
git add .
git commit -m "test: verify crawl status and watchdog recovery flow"
git pull --rebase
bd dolt push
git push
git status
```

Expected: `git status` reports a clean branch up to date with origin.

---

## Research Notes (2026-05-11, revised)

Evidence gathered from 4 domain-matched research agents (systems-programming:rust-pro, lavra:review:architecture-strategist, lavra:research:best-practices-researcher, lavra:review:code-simplicity-reviewer). Key findings that shaped this revised plan:

**Corrections from prior draft:**
- C1 (secondary sort bug): Removed `COALESCE(started_at, created_at) ASC` — this surfaced oldest completions first. Replaced with `created_at DESC, updated_at DESC, id` matching the `list_ingest_service_jobs` pattern.
- C2 (SQL const injection risk): Changed `format!("... error_text='{}' ...", RECLAIMED_ERROR_TEXT)` to `.bind(RECLAIMED_ERROR_TEXT)` in the SQL UPDATE. Using bind parameters for values is the sqlx idiom; `format!` is only needed for table names.
- C3 (no helper function): `service_job_order_by` helper removed — the `match kind` is inlined directly in `list_service_jobs`. A function used once is just a match.
- C4 (task ordering): Regression test (Task 0) moved before constant extraction (Task 1) so the test proves the extraction doesn't break anything.
- C5 (WATCHDOG_SWEEP_INTERVAL placement): Moved to module level near `POLL_INTERVAL` and `WORKER_BATCH_LIMIT`.

**Confirmed correct from prior draft:**
- B1 (`?` compile error): `match ... Ok(n) / Err(e)` pattern is required — `.await?` inside `tokio::spawn(async move { ... })` returning `()` is a compile error.
- B2 (regression test not code change): Task 0 and Task 4 are regression tests; only Tasks 1-3 and Task 5 add/change implementation.
- B3 (string drift): `RECLAIMED_ERROR_TEXT` in `store.rs`; `WATCHDOG_RECLAIM_PREFIX` in `system.rs:20` is a completely different constant for the legacy `mark_failed` path — left untouched.
- `biased;`: Documented Tokio idiom for shutdown-priority selects. Confirmed in official Tokio docs.
- `notify_one()` x4: Each call is on a different `Arc<Notify>`. `notify_one()` stores a permit; `notify_waiters()` does not and would be wrong for "wake to claim work" patterns.
- CASE ORDER BY performance: Full scan for the sort step is expected SQLite behavior (temp B-tree). Fine for small job tables; an index on `status` can still accelerate `WHERE` filters.
- `?1`/`?2` syntax: Consistent with `list_ingest_service_jobs`; both `?` and `?N` work in sqlx/SQLite. Numbered form matches existing pattern.
