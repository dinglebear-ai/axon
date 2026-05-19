# Watch Scheduler (Top-Level) Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a top-level `axon watch` scheduling system that supports `crawl`, `refresh`, `scrape`, `extract`, and `research`, and surface all job families (including refresh) plus full per-job details in `/jobs` and `/jobs/[id]`.

**Architecture:** Introduce watch definitions and watch run history as first-class database entities in `crates/jobs/watch.rs`, then dispatch watch executions to existing command/job handlers (`crawl`, `refresh`, `scrape`, `extract`, `research`). Keep `refresh schedule` as a compatibility alias that forwards to watch definitions. Extend web APIs to include refresh jobs and complete crawl artifact metadata (manifest entries + URL lists + all result/config payload fields).

**Tech Stack:** Rust (`sqlx`, existing jobs/worker infrastructure), Next.js API routes/UI in `apps/web`, PostgreSQL tables under existing schema-init model, RabbitMQ/Redis unchanged.

---

## Preconditions

- Branch from current `main` in `/home/jmagar/workspace/axon_rust`.
- Ensure local services are up for integration tests:

```bash
docker compose up -d axon-postgres axon-redis axon-rabbitmq axon-qdrant
```

- Use existing verification commands during work:

```bash
cargo test
cargo check
```

---

### Task 1: Add failing tests for `/api/jobs` to include refresh jobs in list/counts/filters

**Files:**
- Modify: `apps/web/app/api/jobs/route.ts`
- Modify/Create: `apps/web/__tests__/api/jobs-route.test.ts` (or existing route test file if present)

**Step 1: Write the failing test**

```ts
it('includes refresh jobs in type=all union and in counts', async () => {
  // seed 1 row in axon_refresh_jobs
  // GET /api/jobs?type=all&status=all
  // expect at least one job with type === 'refresh'
  // expect counts aggregate includes refresh table rows
})

it('supports type=refresh filter', async () => {
  // GET /api/jobs?type=refresh
  // expect only refresh rows returned
})
```

**Step 2: Run test to verify it fails**

Run:

```bash
pnpm --dir apps/web vitest run apps/web/__tests__/api/jobs-route.test.ts
```

Expected: FAIL because `refresh` is not part of `JOB_TYPES`, `VALID_TYPES`, or SQL union/count query.

**Step 3: Write minimal implementation**

- Update job type allowlist and union query in API to include `axon_refresh_jobs`.
- Add dedicated refresh query helper similar to `queryCrawl/queryExtract/...`.
- Include refresh in status count aggregation.

**Step 4: Run test to verify it passes**

Run same test command; expect PASS.

**Step 5: Commit**

```bash
git add apps/web/lib/server/job-types.ts apps/web/app/api/jobs/route.ts apps/web/__tests__/api/jobs-route.test.ts
git commit -m "feat(web): include refresh jobs in /api/jobs filters and counts"
```

---

### Task 2: Add failing tests for Jobs dashboard to expose Refresh filter/tab and render refresh rows

**Files:**
- Modify: `apps/web/components/jobs/jobs-dashboard.tsx`
- Modify: `apps/web/components/jobs/job-cells.tsx`
- Modify/Create: `apps/web/__tests__/jobs-dashboard.test.tsx`

**Step 1: Write the failing test**

```tsx
it('renders Refresh type filter and requests type=refresh', async () => {
  // render dashboard, click Refresh filter
  // assert apiFetch called with /api/jobs?type=refresh
})

it('renders refresh row chip and status correctly', async () => {
  // mock jobs response with one { type: 'refresh' }
  // assert row visible and TypeChip renders "refresh"
})
```

**Step 2: Run test to verify it fails**

Run:

```bash
pnpm --dir apps/web vitest run apps/web/__tests__/jobs-dashboard.test.tsx
```

Expected: FAIL (type union and tabs do not include refresh).

**Step 3: Write minimal implementation**

- Add `refresh` to `TypeFilter` tabs and type styles.
- Ensure table row rendering accepts the expanded `JobType` union.

**Step 4: Run test to verify it passes**

Run same command; expect PASS.

**Step 5: Commit**

```bash
git add apps/web/components/jobs/jobs-dashboard.tsx apps/web/components/jobs/job-cells.tsx apps/web/__tests__/jobs-dashboard.test.tsx
git commit -m "feat(web): add refresh filter and row support in jobs dashboard"
```

---

### Task 3: Add failing tests for job detail API to support refresh jobs and artifact path normalization

**Files:**
- Modify: `apps/web/app/api/jobs/[id]/route.ts`
- Modify/Create: `apps/web/__tests__/api/job-detail-route.test.ts`

**Step 1: Write the failing tests**

```ts
it('returns refresh job detail when id exists in axon_refresh_jobs', async () => {
  // seed refresh row with result_json/config_json
  // GET /api/jobs/:id
  // expect type === 'refresh' and refresh-specific fields populated
})

it('normalizes crawl output_dir to AXON_OUTPUT_DIR mount before reading manifest', async () => {
  // set AXON_OUTPUT_DIR=/axon-output
  // result_json.output_dir=/app/.cache/axon-rust/output/domains/x/sync
  // assert readCrawlManifest attempts /axon-output/domains/x/sync/manifest.jsonl
})
```

**Step 2: Run test to verify it fails**

Run:

```bash
pnpm --dir apps/web vitest run apps/web/__tests__/api/job-detail-route.test.ts
```

Expected: FAIL (no `findRefreshJob`; no output-dir remap function).

**Step 3: Write minimal implementation**

- Add `findRefreshJob` query:

```sql
SELECT id, status, created_at, started_at, finished_at, error_text, urls_json, result_json, config_json
FROM axon_refresh_jobs WHERE id = $1
```

- Extend `JobDetail` type to include refresh-specific fields:
  - `checked`, `changed`, `unchanged`, `notModified`, `failedCount`, `total`, `manifestPath`.
- Add output-dir remap helper in route:

```ts
function normalizeOutputDirForWeb(outputDir: string | null): string | null {
  // map worker path prefix /app/.cache/axon-rust/output -> process.env.AXON_OUTPUT_DIR || '/axon-output'
}
```

**Step 4: Run test to verify it passes**

Run same command; expect PASS.

**Step 5: Commit**

```bash
git add apps/web/app/api/jobs/[id]/route.ts apps/web/__tests__/api/job-detail-route.test.ts
git commit -m "feat(web): add refresh job detail and crawl output-dir normalization"
```

---

### Task 4: Add failing tests for crawl artifact fields (`thin_urls`, `waf_blocked_urls`, `error_pages`) in result_json

**Files:**
- Modify: `crates/jobs/crawl/runtime/worker/result_builder.rs`
- Modify: `crates/jobs/crawl/runtime/worker/process.rs`
- Modify/Create: `crates/jobs/crawl/runtime/tests.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn crawl_result_json_includes_artifact_url_lists_and_error_metrics() {
    // construct CrawlSummary with thin_urls/waf_blocked_urls/error_pages/waf_blocked_pages
    // call build_completed_result or integration path
    // assert JSON has keys: thin_urls, waf_blocked_urls, error_pages, waf_blocked_pages
}
```

**Step 2: Run test to verify it fails**

Run:

```bash
cargo test crawl_result_json_includes_artifact_url_lists_and_error_metrics -- --nocapture
```

Expected: FAIL, keys absent.

**Step 3: Write minimal implementation**

- Extend `CompletedResultContext` and call sites to pass needed summary values.
- Add JSON keys in completed payload:

```rust
"error_pages": result_ctx.final_summary.error_pages,
"waf_blocked_pages": result_ctx.final_summary.waf_blocked_pages,
"thin_urls": sorted_vec(result_ctx.final_summary.thin_urls),
"waf_blocked_urls": sorted_vec(result_ctx.final_summary.waf_blocked_urls),
```

- Ensure canceled/partial payload also includes available artifact metrics when present.

**Step 4: Run test to verify it passes**

Run same test command; expect PASS.

**Step 5: Commit**

```bash
git add crates/jobs/crawl/runtime/worker/result_builder.rs crates/jobs/crawl/runtime/worker/process.rs crates/jobs/crawl/runtime/tests.rs
git commit -m "feat(crawl): persist thin/waf url artifacts and error metrics in result_json"
```

---

### Task 5: Add failing tests for job detail page artifact reload behavior after crawl completion

**Files:**
- Modify: `apps/web/app/jobs/[id]/page.tsx`
- Modify/Create: `apps/web/__tests__/job-detail-page.test.tsx`

**Step 1: Write the failing test**

```tsx
it('re-requests includeArtifacts=1 after status transitions from running to completed', async () => {
  // first response: running, no output_dir/artifacts
  // second response: completed with output_dir and artifact data
  // assert page shows observedUrls/markdownFiles populated
})
```

**Step 2: Run test to verify it fails**

Run:

```bash
pnpm --dir apps/web vitest run apps/web/__tests__/job-detail-page.test.tsx
```

Expected: FAIL (poll loop uses includeArtifacts=0 forever while running).

**Step 3: Write minimal implementation**

- In poll loop, detect terminal transition and force one refetch with `includeArtifacts=1`.
- Preserve existing merge behavior for previous artifact state.

Pseudo-code:

```tsx
if (previous?.status === 'running' && data.status !== 'running') {
  const complete = await fetchJob(true)
  // or set flag to trigger one includeArtifacts pass
}
```

**Step 4: Run test to verify it passes**

Run same command; expect PASS.

**Step 5: Commit**

```bash
git add apps/web/app/jobs/[id]/page.tsx apps/web/__tests__/job-detail-page.test.tsx
git commit -m "fix(web): refresh crawl artifacts when job reaches terminal state"
```

---

### Task 6: Add failing tests for full metadata rendering on job detail page

**Files:**
- Modify: `apps/web/app/jobs/[id]/page.tsx`
- Modify/Create: `apps/web/__tests__/job-detail-page-metadata.test.tsx`

**Step 1: Write the failing test**

```tsx
it('renders full result_json and config_json key-value metadata sections', async () => {
  // supply job with nested resultJson + configJson
  // assert flattened metadata rows exist for all keys
})

it('renders refresh-specific stats and URLs', async () => {
  // type=refresh payload with checked/changed/manifestPath/urls_json
  // assert visible sections and values
})
```

**Step 2: Run test to verify it fails**

Run:

```bash
pnpm --dir apps/web vitest run apps/web/__tests__/job-detail-page-metadata.test.tsx
```

Expected: FAIL (no full metadata sections, no refresh page rendering).

**Step 3: Write minimal implementation**

- Add structured sections for:
  - `Refresh Summary`
  - `Result JSON (flattened key-value)`
  - `Config JSON (flattened key-value)`
- Keep existing raw JSON block for debugging.
- Add sections for all known crawl artifact pointers:
  - manifest path
  - output dir
  - audit report path

**Step 4: Run test to verify it passes**

Run same command; expect PASS.

**Step 5: Commit**

```bash
git add apps/web/app/jobs/[id]/page.tsx apps/web/__tests__/job-detail-page-metadata.test.tsx
git commit -m "feat(web): render full job metadata and refresh detail sections"
```

---

### Task 7: Add failing Rust tests for new `watch` schema + CRUD in jobs module

**Files:**
- Create: `crates/jobs/watch.rs`
- Create: `crates/jobs/watch/tests.rs`
- Modify: `crates/jobs/mod.rs` (or equivalent module export location)

**Step 1: Write failing tests**

```rust
#[tokio::test]
async fn create_watch_persists_definition() { /* ... */ }

#[tokio::test]
async fn claim_due_watches_uses_skip_locked_and_lease() { /* ... */ }

#[tokio::test]
async fn create_watch_run_records_dispatched_job() { /* ... */ }
```

**Step 2: Run tests to verify they fail**

Run:

```bash
cargo test watch:: -- --nocapture
```

Expected: FAIL (`watch` module not implemented).

**Step 3: Write minimal implementation**

- Implement tables with `ensure_schema_once` pattern:
  - `axon_watch_defs`
  - `axon_watch_runs`
  - `axon_watch_run_artifacts`
- Add CRUD + claim/run lifecycle helpers.

**Step 4: Run tests to verify they pass**

Run same command; expect PASS.

**Step 5: Commit**

```bash
git add crates/jobs/watch.rs crates/jobs/watch/tests.rs crates/jobs/mod.rs
git commit -m "feat(jobs): add watch definitions and run-history schema"
```

---

### Task 8: Add failing tests for top-level CLI `axon watch` parsing

**Files:**
- Modify: `crates/core/config/cli.rs`
- Modify: `crates/core/config/parse/build_config.rs`
- Modify: `crates/core/config/parse/helpers.rs`
- Modify/Create: `crates/core/config/parse.rs` tests

**Step 1: Write failing parse tests**

```rust
#[test]
fn parse_watch_create_with_every_and_type() { /* ... */ }

#[test]
fn parse_watch_run_now() { /* ... */ }

#[test]
fn parse_watch_history_with_limit() { /* ... */ }
```

**Step 2: Run tests to verify they fail**

Run:

```bash
cargo test parse_watch_ -- --nocapture
```

Expected: FAIL (no watch command/subcommand definitions).

**Step 3: Write minimal implementation**

- Add `CommandKind::Watch` and `CliCommand::Watch`.
- Add `WatchSubcommand` enum with `create/list/get/update/run/pause/resume/delete/history/artifacts`.
- Map watch subcommands into `Config.positional` and fields used by handlers.

**Step 4: Run tests to verify they pass**

Run same command; expect PASS.

**Step 5: Commit**

```bash
git add crates/core/config/cli.rs crates/core/config/parse/build_config.rs crates/core/config/parse/helpers.rs crates/core/config/parse.rs

git commit -m "feat(cli): add top-level watch command parsing"
```

---

### Task 9: Add failing tests for `axon watch` command handlers

**Files:**
- Create: `crates/cli/commands/watch.rs`
- Create: `crates/cli/commands/watch/` helpers as needed
- Modify: `crates/cli/commands/mod.rs` (or dispatch file)
- Modify/Create: `crates/cli/commands/watch_tests.rs`

**Step 1: Write failing handler tests**

```rust
#[tokio::test]
async fn watch_create_emits_json_with_id() { /* ... */ }

#[tokio::test]
async fn watch_list_returns_definitions() { /* ... */ }

#[tokio::test]
async fn watch_run_now_dispatches_task_and_returns_run_id() { /* ... */ }
```

**Step 2: Run tests to verify they fail**

Run:

```bash
cargo test watch_create_emits_json_with_id -- --nocapture
```

Expected: FAIL (no command handler).

**Step 3: Write minimal implementation**

- Implement command routing and JSON/human outputs aligned with existing CLI patterns.
- Reuse job contracts style for consistency.

**Step 4: Run tests to verify they pass**

Run same command; expect PASS.

**Step 5: Commit**

```bash
git add crates/cli/commands/watch.rs crates/cli/commands/watch crates/cli/commands/mod.rs crates/cli/commands/watch_tests.rs
git commit -m "feat(cli): implement watch command handlers"
```

---

### Task 10: Implement compatibility bridge from `refresh schedule` to `watch`

**Files:**
- Modify: `crates/cli/commands/refresh/schedule.rs`
- Modify: `crates/jobs/refresh/schedule.rs`
- Modify/Create: `crates/cli/commands/refresh/schedule_compat_tests.rs`

**Step 1: Write failing compatibility tests**

```rust
#[tokio::test]
async fn refresh_schedule_add_creates_watch_def_with_task_refresh() { /* ... */ }

#[tokio::test]
async fn refresh_schedule_list_reads_from_watch_defs_refresh() { /* ... */ }
```

**Step 2: Run tests to verify they fail**

Run:

```bash
cargo test refresh_schedule_add_creates_watch_def_with_task_refresh -- --nocapture
```

Expected: FAIL (legacy path still writes old schedule table only).

**Step 3: Write minimal implementation**

- Keep CLI interface stable.
- Internally map to watch definitions for refresh tasks.
- Keep backward-compatible rendering and outputs.

**Step 4: Run tests to verify they pass**

Run same command; expect PASS.

**Step 5: Commit**

```bash
git add crates/cli/commands/refresh/schedule.rs crates/jobs/refresh/schedule.rs crates/cli/commands/refresh/schedule_compat_tests.rs
git commit -m "feat(refresh): route schedule commands through watch definitions"
```

---

### Task 11: Add watch scheduler worker loop and dispatch integration

**Files:**
- Create: `crates/jobs/watch_worker.rs` (or `crates/jobs/watch/worker.rs`)
- Modify: `crates/jobs/worker_lane.rs` or dedicated loop wiring
- Modify: `docker/s6` service definitions for workers (if required by current runtime)
- Modify/Create: `crates/jobs/watch_worker_tests.rs`

**Step 1: Write failing worker tests**

```rust
#[tokio::test]
async fn watch_worker_claims_due_defs_and_dispatches_jobs() { /* ... */ }

#[tokio::test]
async fn watch_worker_records_run_result_on_success_and_failure() { /* ... */ }
```

**Step 2: Run tests to verify they fail**

Run:

```bash
cargo test watch_worker_claims_due_defs_and_dispatches_jobs -- --nocapture
```

Expected: FAIL (worker loop not present).

**Step 3: Write minimal implementation**

- Periodically claim due watches.
- Create run records.
- Dispatch by task type to existing service/job functions.
- Persist terminal state, metrics, and artifact pointers.
- Compute next run timestamps.

**Step 4: Run tests to verify they pass**

Run same command; expect PASS.

**Step 5: Commit**

```bash
git add crates/jobs/watch_worker.rs crates/jobs/watch_worker_tests.rs
git commit -m "feat(jobs): add watch scheduler worker dispatch loop"
```

---

### Task 12: Update docs and command references

**Files:**
- Modify: `README.md`
- Modify: `docs/SCHEMA.md`
- Modify: `docs/JOB-LIFECYCLE.md`
- Create: `docs/commands/watch.md`
- Modify: `docs/commands/refresh.md`

**Step 1: Write docs tests/checklist**

Add doc checklist in session notes test file or manual verification script:

```bash
rg -n "watch" README.md docs/commands docs/SCHEMA.md docs/JOB-LIFECYCLE.md
```

**Step 2: Run check to verify gaps**

Expected: missing watch docs before edits.

**Step 3: Write minimal documentation updates**

- Add top-level `watch` command docs with examples.
- Add schema sections for watch tables.
- Document refresh schedule compatibility and migration behavior.

**Step 4: Run docs check**

Run same `rg` command; expect all targets updated.

**Step 5: Commit**

```bash
git add README.md docs/SCHEMA.md docs/JOB-LIFECYCLE.md docs/commands/watch.md docs/commands/refresh.md
git commit -m "docs: add watch scheduler command and schema references"
```

---

## Final Verification Gate

Run full verification before PR:

```bash
cargo check
cargo test
pnpm --dir apps/web vitest run
```

Expected:
- Rust checks/tests pass.
- Web tests pass.
- `/api/jobs` includes refresh in list, filters, and counts.
- `/api/jobs/[id]` returns refresh details and full crawl artifact data.
- Job detail page renders all available metadata and artifact lists.

---

## Risks and Mitigations

- **Risk:** Path mismatches between worker and web container output roots.
  - **Mitigation:** centralize output-dir normalization in `apps/web/app/api/jobs/[id]/route.ts` and cover with tests.
- **Risk:** Expanding `result_json` shape breaks consumers.
  - **Mitigation:** additive keys only; keep existing keys unchanged.
- **Risk:** Watch scheduler introduces duplicate runs.
  - **Mitigation:** row-level claim lock + active-run de-dupe checks.

---

## Rollback Plan

1. Disable watch worker service.
2. Keep legacy command behavior through refresh compatibility bridge.
3. Revert web refresh-type additions only if UI regression is detected.

---

## Definition of Done

- `axon watch` top-level command exists and passes tests.
- Scheduling works for `crawl`, `refresh`, `scrape`, `extract`, `research`.
- `/jobs` shows all job families, including refresh.
- `/jobs/[id]` shows all available metadata and artifact details per job.
- Docs updated for operators and contributors.
