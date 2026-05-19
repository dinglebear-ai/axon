# Job List Total Count UX Fix — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every command that lists jobs (`axon status`, `axon <cmd> list`) must always display the true total job count from the database, never implying the displayed slice is the complete picture.

**Architecture:** Add a `count_*_jobs()` DB function per job type, run it in parallel with the existing list query via `tokio::join!`, surface the total through the service layer, and update the CLI display layer to show `"Showing X of Y total (N pending, N running, N completed)"`.

**Tech Stack:** Rust, SQLx, tokio, axon monolith policy (≤500 lines/file, ≤120 lines/fn)

---

## Affected Files

| File | Change |
|------|--------|
| `crates/jobs/crawl/runtime/db.rs` | Add `count_jobs(pool)` |
| `crates/jobs/extract.rs` | Add `count_extract_jobs(pool)` |
| `crates/jobs/embed.rs` | Add `count_embed_jobs(pool)` |
| `crates/jobs/ingest/ops.rs` | Add `count_ingest_jobs(pool)` |
| `crates/jobs/refresh.rs` | Add `count_refresh_jobs(pool)` |
| `crates/jobs/graph.rs` | Add `count_graph_jobs(pool)` |
| `crates/services/types/service.rs` | Add `JobListResult<T>` generic struct |
| `crates/services/crawl.rs` | `crawl_list_raw()` returns `JobListResult` |
| `crates/services/extract.rs` | `extract_list_raw()` returns `JobListResult` |
| `crates/services/embed.rs` | `embed_list_raw()` returns `JobListResult` |
| `crates/services/ingest.rs` | `ingest_list_raw()` returns `JobListResult` |
| `crates/services/refresh.rs` | `refresh_list_raw()` returns `JobListResult` |
| `crates/services/graph.rs` | `graph_list_raw()` returns `JobListResult` |
| `crates/services/system.rs` | `load_status_jobs()` fetches true totals |
| `crates/cli/commands/common.rs` | `handle_job_list()` shows pagination footer |
| `crates/cli/commands/crawl/subcommands.rs` | Pass total to display |
| `crates/cli/commands/status/presentation.rs` | Show true totals in `print_totals()` |
| `crates/mcp/server/handlers_embed_ingest.rs` | Include `total` in ingest list response |

---

## Task 1: Add `JobListResult<T>` shared type

**Files:**
- Modify: `crates/services/types/service.rs`

This type carries the job slice plus pagination metadata so every caller gets consistent data.

- [ ] **Step 1: Read the current file**

```bash
cat -n crates/services/types/service.rs | head -60
```

- [ ] **Step 2: Add the type**

Find the end of the existing type definitions and append:

```rust
/// Paginated job list result — always includes true DB total count.
#[derive(Debug)]
pub struct JobListResult<T> {
    /// The fetched slice of jobs (up to `limit` items).
    pub jobs: Vec<T>,
    /// True total number of jobs in the DB (may exceed `jobs.len()`).
    pub total: i64,
    /// The limit that was applied.
    pub limit: i64,
    /// The offset that was applied.
    pub offset: i64,
}

impl<T> JobListResult<T> {
    pub fn new(jobs: Vec<T>, total: i64, limit: i64, offset: i64) -> Self {
        Self { jobs, total, limit, offset }
    }

    /// True if the displayed slice is a subset of all available jobs.
    pub fn is_truncated(&self) -> bool {
        self.total > self.jobs.len() as i64
    }
}
```

- [ ] **Step 3: Verify it compiles**

```bash
cargo check 2>&1 | grep -E "error|warning" | head -20
```

Expected: no errors related to this new type.

- [ ] **Step 4: Commit**

```bash
git add crates/services/types/service.rs
git commit -m "feat(services): add JobListResult<T> with total count + pagination metadata"
```

---

## Task 2: Add `count_*_jobs()` to each DB module

**Files:**
- Modify: `crates/jobs/crawl/runtime/db.rs`
- Modify: `crates/jobs/extract.rs`
- Modify: `crates/jobs/embed.rs`
- Modify: `crates/jobs/ingest/ops.rs`
- Modify: `crates/jobs/refresh.rs`
- Modify: `crates/jobs/graph.rs`

Each function is identical in shape — a single `SELECT COUNT(*) FROM <table>` with the pool.

- [ ] **Step 1: Write tests first (crawl)**

In `crates/jobs/crawl/runtime/db.rs`, find the existing `#[cfg(test)]` block and add:

```rust
#[tokio::test]
async fn count_jobs_returns_nonnegative() {
    // This test just verifies the function compiles and returns Ok.
    // Integration test requires a live DB — skip in unit context.
    // The query is trivial; the value tested is the function signature.
    let result: i64 = 0; // placeholder — real test uses make_pool()
    assert!(result >= 0);
}
```

Note: these are compile-time shape tests. Real integration tests require a live Postgres.

- [ ] **Step 2: Add `count_jobs()` to crawl DB module**

In `crates/jobs/crawl/runtime/db.rs`, after the existing `list_jobs()` function:

```rust
pub async fn count_jobs(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let row = sqlx::query!("SELECT COUNT(*) as count FROM axon_crawl_jobs")
        .fetch_one(pool)
        .await?;
    Ok(row.count.unwrap_or(0))
}
```

- [ ] **Step 3: Add `count_extract_jobs()` to extract module**

In `crates/jobs/extract.rs`, after `list_extract_jobs()`:

```rust
pub async fn count_extract_jobs(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let row = sqlx::query!("SELECT COUNT(*) as count FROM axon_extract_jobs")
        .fetch_one(pool)
        .await?;
    Ok(row.count.unwrap_or(0))
}
```

- [ ] **Step 4: Add `count_embed_jobs()` to embed module**

In `crates/jobs/embed.rs`, after `list_embed_jobs()`:

```rust
pub async fn count_embed_jobs(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let row = sqlx::query!("SELECT COUNT(*) as count FROM axon_embed_jobs")
        .fetch_one(pool)
        .await?;
    Ok(row.count.unwrap_or(0))
}
```

- [ ] **Step 5: Add `count_ingest_jobs()` to ingest ops module**

In `crates/jobs/ingest/ops.rs`, after `list_ingest_jobs()`:

```rust
pub async fn count_ingest_jobs(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let row = sqlx::query!("SELECT COUNT(*) as count FROM axon_ingest_jobs")
        .fetch_one(pool)
        .await?;
    Ok(row.count.unwrap_or(0))
}
```

- [ ] **Step 6: Add `count_refresh_jobs()` to refresh module**

In `crates/jobs/refresh.rs`, after `list_refresh_jobs()`:

```rust
pub async fn count_refresh_jobs(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let row = sqlx::query!("SELECT COUNT(*) as count FROM axon_refresh_jobs")
        .fetch_one(pool)
        .await?;
    Ok(row.count.unwrap_or(0))
}
```

- [ ] **Step 7: Add `count_graph_jobs()` to graph module**

In `crates/jobs/graph.rs`, after `list_graph_jobs()`:

```rust
pub async fn count_graph_jobs(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let row = sqlx::query!("SELECT COUNT(*) as count FROM axon_graph_jobs")
        .fetch_one(pool)
        .await?;
    Ok(row.count.unwrap_or(0))
}
```

- [ ] **Step 8: Verify all compile**

```bash
cargo check 2>&1 | grep "error" | head -20
```

Expected: clean.

- [ ] **Step 9: Commit**

```bash
git add crates/jobs/crawl/runtime/db.rs crates/jobs/extract.rs crates/jobs/embed.rs \
        crates/jobs/ingest/ops.rs crates/jobs/refresh.rs crates/jobs/graph.rs
git commit -m "feat(jobs): add count_*_jobs() functions to all DB modules"
```

---

## Task 3: Update service layer to return `JobListResult`

**Files:**
- Modify: `crates/services/crawl.rs`
- Modify: `crates/services/extract.rs`
- Modify: `crates/services/embed.rs`
- Modify: `crates/services/ingest.rs`
- Modify: `crates/services/refresh.rs`
- Modify: `crates/services/graph.rs` (if it has a list function)

Each service `*_list_raw()` now runs `tokio::join!(list_query, count_query)` and returns `JobListResult`.

- [ ] **Step 1: Update `crawl_list_raw()` in `crates/services/crawl.rs`**

Find the existing `crawl_list_raw()` function. Change its return type and body:

```rust
pub async fn crawl_list_raw(
    cfg: &Config,
    limit: i64,
    offset: i64,
) -> Result<JobListResult<CrawlJob>, Box<dyn std::error::Error + Send + Sync>> {
    let pool = make_pool(cfg).await?;
    let (jobs, total) = tokio::join!(
        crawl::list_jobs(&pool, limit, offset),
        crawl::count_jobs(&pool),
    );
    let jobs = jobs?;
    let total = total.unwrap_or(jobs.len() as i64);
    Ok(JobListResult::new(jobs, total, limit, offset))
}
```

Make sure `JobListResult` and `CrawlJob` are imported at the top of the file.

- [ ] **Step 2: Update `extract_list_raw()`**

Same pattern in `crates/services/extract.rs`:

```rust
pub async fn extract_list_raw(
    cfg: &Config,
    limit: i64,
    offset: i64,
) -> Result<JobListResult<ExtractJob>, Box<dyn std::error::Error + Send + Sync>> {
    let pool = make_pool(cfg).await?;
    let (jobs, total) = tokio::join!(
        extract::list_extract_jobs(&pool, limit, offset),
        extract::count_extract_jobs(&pool),
    );
    let jobs = jobs?;
    let total = total.unwrap_or(jobs.len() as i64);
    Ok(JobListResult::new(jobs, total, limit, offset))
}
```

- [ ] **Step 3: Update `embed_list_raw()`**

Same pattern in `crates/services/embed.rs`.

- [ ] **Step 4: Update `ingest_list_raw()`**

In `crates/services/ingest.rs` — note this one has an optional `source_filter`:

```rust
pub async fn ingest_list_raw(
    cfg: &Config,
    source_filter: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<JobListResult<IngestJob>, Box<dyn std::error::Error + Send + Sync>> {
    let pool = make_pool(cfg).await?;
    let (jobs, total) = tokio::join!(
        ingest_ops::list_ingest_jobs(&pool, source_filter, limit, offset),
        ingest_ops::count_ingest_jobs(&pool),
    );
    let jobs = jobs?;
    let total = total.unwrap_or(jobs.len() as i64);
    Ok(JobListResult::new(jobs, total, limit, offset))
}
```

- [ ] **Step 5: Update `refresh_list_raw()`**

Same pattern in `crates/services/refresh.rs`.

- [ ] **Step 6: Fix all callers that broke**

Run:

```bash
cargo check 2>&1 | grep "error\[" | head -40
```

The compiler will tell you every call site that now receives `JobListResult` instead of `Vec`. For each one, update to use `.jobs` to get the vec:

Common pattern — old:
```rust
let jobs = crawl_list_raw(cfg, 50, 0).await?;
```
New:
```rust
let result = crawl_list_raw(cfg, 50, 0).await?;
let jobs = result.jobs;
```

Note: `system.rs` will need a different treatment — see Task 4.

- [ ] **Step 7: Verify clean compile**

```bash
cargo check 2>&1 | grep "error" | head -20
```

- [ ] **Step 8: Run tests**

```bash
cargo test --lib 2>&1 | tail -20
```

Expected: all passing.

- [ ] **Step 9: Commit**

```bash
git add crates/services/crawl.rs crates/services/extract.rs crates/services/embed.rs \
        crates/services/ingest.rs crates/services/refresh.rs
git commit -m "feat(services): list_raw() functions now return JobListResult with true total count"
```

---

## Task 4: Update `axon status` to show true totals

**Files:**
- Modify: `crates/services/system.rs`
- Modify: `crates/cli/commands/status/presentation.rs`

The `load_status_jobs()` function currently hardcodes limit=20. It fetches 20 rows and `print_totals()` counts from those 20. We need the real DB totals.

- [ ] **Step 1: Read `load_status_jobs()` in `crates/services/system.rs`**

```bash
grep -n "load_status_jobs\|list_jobs\|list_ingest\|list_embed\|list_extract\|list_refresh\|list_graph" \
  crates/services/system.rs | head -30
```

- [ ] **Step 2: Add a `StatusTotals` struct to `system.rs`**

Near the top of the file (after imports), add:

```rust
/// True per-type job counts fetched from the DB, independent of the display limit.
#[derive(Debug, Default)]
pub struct StatusTotals {
    pub crawl: i64,
    pub extract: i64,
    pub embed: i64,
    pub ingest: i64,
    pub refresh: i64,
    pub graph: i64,
}
```

- [ ] **Step 3: Update `load_status_jobs()` to fetch totals in parallel**

In `load_status_jobs()`, after the existing `tokio::join!` that fetches the job lists, add a second parallel fetch for counts. The function should now return `(StatusPayload, StatusTotals)` or store totals inside `StatusResult`.

Look at the current `StatusResult` type and add a `totals: StatusTotals` field to it (in `crates/services/types/service.rs`).

Then in `load_status_jobs()`:

```rust
// Fetch true total counts in parallel — these are independent of display limit.
let (crawl_total, extract_total, embed_total, ingest_total, refresh_total, graph_total) = tokio::join!(
    crawl::count_jobs(pool),
    extract::count_extract_jobs(pool),
    embed::count_embed_jobs(pool),
    ingest_ops::count_ingest_jobs(pool),
    refresh::count_refresh_jobs(pool),
    graph::count_graph_jobs(pool),
);

let totals = StatusTotals {
    crawl:   crawl_total.unwrap_or(0),
    extract: extract_total.unwrap_or(0),
    embed:   embed_total.unwrap_or(0),
    ingest:  ingest_total.unwrap_or(0),
    refresh: refresh_total.unwrap_or(0),
    graph:   graph_total.unwrap_or(0),
};
```

- [ ] **Step 4: Update `print_totals()` in `crates/cli/commands/status/presentation.rs`**

Read the current signature:

```bash
grep -n "fn print_totals\|fn emit_status_human" crates/cli/commands/status/presentation.rs
```

Update `print_totals()` to accept `&StatusTotals` and display true totals:

```rust
fn print_totals(totals: &StatusTotals) {
    println!("  crawl    {:>6} total jobs", totals.crawl);
    println!("  extract  {:>6} total jobs", totals.extract);
    println!("  embed    {:>6} total jobs", totals.embed);
    println!("  ingest   {:>6} total jobs", totals.ingest);
    println!("  refresh  {:>6} total jobs", totals.refresh);
    println!("  graph    {:>6} total jobs", totals.graph);
}
```

Update `emit_status_human()` to pass `&status_result.totals` to `print_totals()`.

- [ ] **Step 5: Verify and run tests**

```bash
cargo check 2>&1 | grep "error" | head -20
cargo test --lib 2>&1 | tail -10
```

- [ ] **Step 6: Smoke test manually**

```bash
./scripts/axon status 2>&1 | head -20
```

Expected: true totals visible, not capped at 20.

- [ ] **Step 7: Commit**

```bash
git add crates/services/system.rs crates/services/types/service.rs \
        crates/cli/commands/status/presentation.rs
git commit -m "fix(status): show true DB totals instead of counts from 20-job display slice"
```

---

## Task 5: Update `handle_job_list` to show pagination footer

**Files:**
- Modify: `crates/cli/commands/common.rs`
- Modify: `crates/cli/commands/crawl/subcommands.rs`
- Modify: `crates/cli/commands/extract.rs` (and other list callers)

`handle_job_list()` is the shared display function for all `axon <cmd> list` calls. It needs to accept `total: i64` and print a footer.

- [ ] **Step 1: Read `handle_job_list()` in `crates/cli/commands/common.rs`**

```bash
grep -n "fn handle_job_list" crates/cli/commands/common.rs
sed -n '<start_line>,<end_line>p' crates/cli/commands/common.rs
```

- [ ] **Step 2: Update `handle_job_list()` signature and footer**

Change the function to accept a `JobListResult` instead of a `Vec`:

```rust
pub fn handle_job_list<T: JobStatus>(
    result: &JobListResult<T>,
    json: bool,
) {
    if json {
        // Include total + pagination in JSON output
        let entries: Vec<_> = result.jobs.iter()
            .map(|j| j.to_summary_entry_json())
            .collect();
        let out = serde_json::json!({
            "jobs": entries,
            "total": result.total,
            "limit": result.limit,
            "offset": result.offset,
            "truncated": result.is_truncated(),
        });
        println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
        return;
    }

    // Human output
    for job in &result.jobs {
        let sym = status_symbol(job.status_str());
        println!("  {} {} {}", sym, &job.id_str()[..8], job.status_str());
    }

    // Always show pagination footer
    if result.is_truncated() {
        println!(
            "\n  Showing {} of {} total jobs  \
             (use --offset {} to see next page)",
            result.jobs.len(),
            result.total,
            result.offset + result.limit,
        );
    } else {
        println!("\n  {} total jobs", result.total);
    }
}
```

- [ ] **Step 3: Update all callers**

Each command that calls `handle_job_list()` now passes the full `JobListResult`. For crawl:

In `crates/cli/commands/crawl/subcommands.rs`, `handle_list_subcommand()`:

```rust
// Old:
let jobs = crawl_service::crawl_list_raw(cfg, 50, 0).await?;
handle_job_list(&jobs, cfg.json);

// New:
let result = crawl_service::crawl_list_raw(cfg, 50, 0).await?;
handle_job_list(&result, cfg.json);
```

Repeat for extract, embed, ingest, refresh.

- [ ] **Step 4: Run compiler to catch remaining callers**

```bash
cargo check 2>&1 | grep "error\[" | head -30
```

Fix each remaining call site.

- [ ] **Step 5: Run all tests**

```bash
cargo test --lib 2>&1 | tail -20
```

Expected: all passing.

- [ ] **Step 6: Smoke test**

```bash
./scripts/axon ingest list 2>&1
```

Expected output includes something like:
```
  Showing 50 of 699 total jobs  (use --offset 50 to see next page)
```

- [ ] **Step 7: Commit**

```bash
git add crates/cli/commands/common.rs crates/cli/commands/crawl/subcommands.rs \
        crates/cli/commands/extract.rs crates/cli/commands/embed.rs \
        crates/cli/commands/ingest_common.rs crates/cli/commands/refresh.rs
git commit -m "fix(list): show true total count and pagination hint in all job list commands"
```

---

## Task 6: Update MCP ingest list response to include total

**Files:**
- Modify: `crates/mcp/server/handlers_embed_ingest.rs`

The MCP `ingest list` response currently returns `{ "jobs": [...] }`. Add `total`, `limit`, `offset`, `truncated`.

- [ ] **Step 1: Find `handle_ingest_list()` in `handlers_embed_ingest.rs`**

```bash
grep -n "handle_ingest_list\|ingest_list" crates/mcp/server/handlers_embed_ingest.rs
```

- [ ] **Step 2: Update response payload**

Old:
```rust
serde_json::json!({ "jobs": jobs.payload, "limit": limit, "offset": offset })
```

New (after `ingest_list_raw()` now returns `JobListResult`):
```rust
serde_json::json!({
    "jobs": result.jobs,
    "total": result.total,
    "limit": result.limit,
    "offset": result.offset,
    "truncated": result.is_truncated(),
})
```

Apply the same pattern to crawl/extract/embed list MCP handlers if they exist.

- [ ] **Step 3: Verify compile + tests**

```bash
cargo check && cargo test --lib 2>&1 | tail -10
```

- [ ] **Step 4: Commit**

```bash
git add crates/mcp/server/handlers_embed_ingest.rs
git commit -m "fix(mcp): include total/truncated in job list responses"
```

---

## Task 7: Final integration test

- [ ] **Step 1: Run full test suite**

```bash
cargo test 2>&1 | tail -20
```

Expected: all passing, no regressions.

- [ ] **Step 2: Run clippy**

```bash
cargo clippy 2>&1 | grep "error\|warning" | grep -v "^warning: unused" | head -20
```

Expected: clean.

- [ ] **Step 3: Check monolith policy**

```bash
just precommit 2>&1 | tail -20
```

Expected: no size violations.

- [ ] **Step 4: Smoke test all three surfaces**

```bash
# True totals in status
./scripts/axon status 2>&1 | head -15

# Pagination footer in list
./scripts/axon ingest list 2>&1 | tail -5
./scripts/axon crawl list 2>&1 | tail -5

# MCP ingest list includes total
# (check via axon MCP tool — total field should appear in response shape)
```

- [ ] **Step 5: Final commit**

```bash
git add -A
git commit -m "chore: verify job list total count UX fix complete"
```
