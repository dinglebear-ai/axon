# Job Lifecycle
Last Modified: 2026-07-14

The async-job state machine for axon. Jobs use SQLite persistence and in-process tokio workers; there is no message broker, Postgres, or Redis runtime.

> Current runtime only. Legacy family-specific job tables still exist for
> migration/status compatibility, but web page/site/docs acquisition now uses
> Source jobs. See
> [`../pipeline-unification/runtime/job-contract.md`](../pipeline-unification/runtime/job-contract.md).

For module layout, helper-function index, and the `JobBackend` / `ServiceJobRuntime` distinction, see [`../../crates/axon-jobs/src/CLAUDE.md`](../../crates/axon-jobs/src/CLAUDE.md) and [`../../crates/axon-services/src/CLAUDE.md`](../../crates/axon-services/src/CLAUDE.md).

## Table of Contents

1. Scope
2. Job Kinds and Tables
3. State Machine
4. Enqueue Flow
5. Claim and Execute Flow
6. Cancellation Model
7. Stale-Job Recovery
8. Worker Runtime
9. Data Model
10. Operational Commands
11. Failure Modes
12. Source Map

## Scope

Legacy async job families are persisted in SQLite and processed by in-process
workers spawned by `SqliteJobBackend::new_with_workers`:

- **Crawl** — legacy rows only; new web acquisition uses Source jobs.
- **Extract** — LLM structured extraction (`crates/axon-jobs/src/workers/runners/extract.rs`)
- **Embed** — TEI embedding + Qdrant upsert (`crates/axon-jobs/src/workers/runners/embed.rs`)
- **Ingest** — GitHub / GitLab / Gitea / Git / RSS / Reddit / YouTube / sessions (`crates/axon-jobs/src/workers/runners/ingest.rs`)

Refresh and graph job runners were removed with the legacy queue runtime. No migration, `JobKind`, runner, or service code creates or references those old tables.

Watch (recurring scheduler) is **not** part of `JobBackend`/`JobKind`. It is a separate SQLite-backed scheduler in `crates/axon-jobs/src/watch.rs` whose CRUD shim lives in `crates/axon-services/src/watch.rs`. Web watch runs dispatch Source jobs when a watch fires.

## Job Kinds and Tables

`JobKind` (`src/jobs/backend.rs:20-40`) enumerates exactly four variants. There is one SQLite table per kind:

| `JobKind` | SQL table | Type-specific column(s) | Queue cap env var | Default cap |
|-----------|-----------|-------------------------|-------------------|-------------|
| `Crawl`   | `axon_crawl_jobs`   | `url TEXT`                              | `AXON_MAX_PENDING_CRAWL_JOBS`   | 100 |
| `Embed`   | `axon_embed_jobs`   | `input_text TEXT`                       | `AXON_MAX_PENDING_EMBED_JOBS`   | 50  |
| `Extract` | `axon_extract_jobs` | `urls_json TEXT` (JSON array)           | `AXON_MAX_PENDING_EXTRACT_JOBS` | 50  |
| `Ingest`  | `axon_ingest_jobs`  | `target TEXT`, `source_type TEXT`       | `AXON_MAX_PENDING_INGEST_JOBS`  | 50  |

`JobKind::table_name()` is the single source of truth for table names — never hardcode `"axon_*_jobs"` strings outside `enqueue.rs` (which interpolates compile-time `&'static str` literals; see safety note in `src/jobs/ops/enqueue.rs:55-64`).

`JobPayload` (`src/jobs/backend.rs:43-62`) carries the per-kind body submitted to `enqueue`:

- `Crawl { url, config_json }`
- `Embed { input, config_json }`
- `Extract { urls: Vec<String>, config_json }`
- `Ingest { target, source_type, config_json }`

`config_json` is a worker-side configuration snapshot produced by `config_snapshot_json()` (`src/jobs/config_snapshot.rs`), so each job replays the submitter's non-secret config knobs irrespective of the worker's local environment.

## State Machine

Five statuses, defined in `JobStatus` (`src/jobs/status.rs:26-32`) and enforced by a SQL CHECK constraint (migration `0003_add_status_checks.sql`):

```
pending → running → { completed | failed | canceled }
pending → canceled                  (cancel before claim)
running → pending                   (stale-job reclaim only)
```

```mermaid
stateDiagram-v2
    [*] --> pending
    pending --> running: claim_next_pending (BEGIN IMMEDIATE)
    pending --> canceled: cancel_row (early cancel)
    running --> completed: mark_completed
    running --> failed: mark_failed
    running --> canceled: cancel_row (late cancel)
    running --> pending: reclaim_stale_running_jobs (watchdog)
```

`JobStatus::from_str` falls back to `JobStatus::Failed` on unknown DB values and emits a `tracing::warn!` (`src/jobs/status.rs:55-69`) — corrupt status strings never crash the runtime.

### Column transitions

Every `axon_*_jobs` row carries the same lifecycle columns. The transitions are:

| Transition           | SQL written                                                                                          | Where                                                  |
|----------------------|------------------------------------------------------------------------------------------------------|--------------------------------------------------------|
| insert (pending)     | `status='pending'`, `created_at=now`, `updated_at=now`                                               | `enqueue_job` (`ops/enqueue.rs`)                  |
| claim (→running)     | `status='running'`, `started_at=now`, `updated_at=now`, `attempt_count+=1`, `active_attempt_id=<uuid>` *(guarded by `WHERE status='pending'`)* | `claim_next_pending_inner` (`ops/lifecycle.rs`)   |
| live progress        | `result_json=…`, `updated_at=now` *(guarded by `status='running'` and the active attempt when called by a worker)* | `update_result_json` (`ops/lifecycle.rs`)         |
| complete             | `status='completed'`, `finished_at=now`, `updated_at=now`, `active_attempt_id=NULL`, `result_json=…` *(if provided; active-attempt guarded in workers)* | `mark_completed_inner` (`ops/lifecycle.rs`)       |
| fail                 | `status='failed'`, `finished_at=now`, `updated_at=now`, `active_attempt_id=NULL`, `error_text=…` *(active-attempt guarded in workers)* | `mark_failed_inner` (`ops/lifecycle.rs`)          |
| cancel               | `status='canceled'`, `finished_at=now`, `updated_at=now`, `active_attempt_id=NULL` *(guarded by `WHERE status IN ('pending','running')`)* | `cancel_row` (`ops/lifecycle.rs`)                 |
| stale reclaim        | `status='pending'`, `error_text='reclaimed after unexpected shutdown'`, `active_attempt_id=NULL`, `last_reclaimed_at=now`, `last_reclaimed_reason=…`, `updated_at=now` *(guarded by `WHERE status='running' AND updated_at<threshold`)* | `reclaim_stale_running_jobs_for_table` (`store.rs`) |

`mark_completed` and `mark_failed` use `WHERE status='running'` so a row that was canceled mid-execution stays in `canceled` — the worker's terminal write is logged as a warning and silently dropped (`ops/lifecycle.rs:138-145`, `200-207`).

## Enqueue Flow

`SqliteJobBackend::enqueue` (`src/jobs/runtime.rs:124-134`):

1. `JobPayload::kind()` selects the target table.
2. `enqueue_job()` checks the per-kind queue cap via `check_pending_cap_for()` (`ops/enqueue.rs:65-87`).
   - `cap == 0` → unlimited; check is skipped.
   - `cap > 0` and `pending_count >= cap` → returns `JobError::QueueCapacityExceeded { kind, cap, current }`.
   - The cap value is parsed once at process start with `LazyLock` and `parse_cap_env`. A non-numeric env value logs `tracing::warn!` and falls back to the default.
3. Insert row with `status='pending'`, fresh UUID, `created_at`/`updated_at = now_ms()`.
4. Returns the new `JobId` (`Uuid`).
5. If the backend has workers (`new_with_workers` mode), `WorkerHandles::notify(kind)` fires the per-kind `Notify` so the lane wakes immediately instead of waiting for the next 5 s poll.

The old crawl-to-embed handoff is not the web source path. Source jobs acquire,
prepare, embed, and publish under one job id with no child Embed job.

## Claim and Execute Flow

The single claim primitive — used by every worker lane — is `claim_next_pending` (`src/jobs/ops/lifecycle.rs:15-88`):

1. Acquire a SQLite connection from the pool.
2. `BEGIN IMMEDIATE` — under WAL this acquires the write lock up front, serializing concurrent claims atomically and removing the SELECT/UPDATE TOCTOU window between lanes.
3. `SELECT id FROM <table> WHERE status='pending' ORDER BY created_at LIMIT 1`.
4. `UPDATE … SET status='running', started_at=?, updated_at=?, attempt_count=attempt_count+1, active_attempt_id=? WHERE id=? AND status='pending'`. The `AND status='pending'` predicate is the second-line defence — if a different lane somehow claimed it first the update affects 0 rows and the call returns `Ok(None)`.
5. `COMMIT` (or `ROLLBACK` on any error).

Lock-contention errors (`database is locked`, `database table is locked`) are swallowed by `retry_busy` (`ops/retry.rs`) up to 5 attempts with exponential backoff starting at 25 ms.

Once a worker holds an `Uuid` plus `active_attempt_id`, `worker_loop` (`src/jobs/workers.rs:170-238`) drives the per-kind runner:

- **Ok(`Some(result_json)`)** → `mark_completed(pool, kind, id, result_json)`.
- **Ok(`None`)** → `mark_completed(pool, kind, id, None)`. Returned when the row was deleted between claim and execute (e.g. `axon … clear` mid-run); a warn is logged.
- **Err(e)** → `mark_failed(pool, kind, id, &e.to_string())`.

Any failure in `mark_completed`/`mark_failed` itself is logged at `error` level and leaves the row in `running` — the next stale-job sweep on process restart will repair it.

A worker processes up to `WORKER_BATCH_LIMIT = 32` jobs per wake before yielding (`workers.rs:36`, `188-235`); shutdown is checked between jobs and between batches.

### Live progress writes

Long-running jobs persist progress through `update_result_json` without changing status. Worker-originated progress includes the active attempt ID in the SQL predicate, so late progress from an older reclaimed attempt cannot update a newer retry that reused the same job ID.

- **Crawl** progress support is legacy/migration-only. New web Source jobs emit
  Source progress events and metrics under the shared Source job id.
- **Embed** uses `spawn_embed_progress_persister` (`workers/progress.rs:30-48`) keyed off `EmbedProgress` from `vector::ops::tei`.
- **Extract** and **ingest** runners write progress directly via `update_result_json` from inside their bodies.

Because progress writes update `updated_at`, they double as a heartbeat that prevents the watchdog from flagging an actively-progressing job as stale.

## Cancellation Model

Cancellation is **in-process only** for the current worker runtime (see `src/jobs/cancel.rs:9-12`). There is no Redis flag and no cross-process polling.

`CancelStore` (`src/jobs/cancel.rs`) is a `DashMap<Uuid, CancellationToken>`. Two paths of consumption:

1. **DB row update** — `cancel_row` flips `status` to `canceled` (gated `WHERE status IN ('pending','running')`).
2. **In-memory token** — runners that registered a token via `cancel_store.register(id)` observe `token.is_cancelled()` mid-execution and abort cleanly.

All active runners register a cancel token for claimed jobs. Crawl observes cancellation at the runner boundary, sends `spider::utils::shutdown("{job_id}{url}")` to the active Spider control target, waits briefly for drain, and returns canceled. Crawl progress JSON written before cancellation remains on the row, including output paths and counts when available. Extract and ingest check cancellation inside their loops or per-target futures; embed observes cancellation at the runner boundary.

`CancelStore::cancel(id, pool, kind)` performs both writes and returns `true` when the row update affected at least one row.

## Stale-Job Recovery

axon detects dead workers by tracking `updated_at` on `running` rows. Reclaim
runs at startup, on the periodic worker watchdog tick, and through explicit
`recover` subcommands.

### Startup sweep

`SqliteJobBackend::init` (`src/jobs/runtime.rs:34-43`) runs on every `SqliteJobBackend::new` / `new_with_workers` boot:

```rust
let stale_threshold_ms =
    (cfg.watchdog_stale_timeout_secs + cfg.watchdog_confirm_secs).max(0) * 1_000;
store::reclaim_stale_running_jobs(&pool, stale_threshold_ms).await?;
store::reclaim_stale_watch_leases(&pool).await?;
```

`reclaim_stale_running_jobs` iterates `JobKind::all()` and for each table runs:

```sql
UPDATE <table>
   SET status='pending',
       error_text='reclaimed after unexpected shutdown',
       updated_at=?
 WHERE status='running'
   AND updated_at < (now_ms - stale_threshold_ms);
```

(`src/jobs/store.rs:90-130`.)

Reclaimed jobs go back to `pending` so the next claim cycle picks them up. The previous `error_text` is overwritten with the marker string, `active_attempt_id` is cleared to reject late writes from the old owner, and `last_reclaimed_at` / `last_reclaimed_reason` are updated. The next claim increments `attempt_count` and assigns a fresh `active_attempt_id`.

When periodic reclaim happens inside a live worker process, the watchdog cancels any matching local `CancellationToken` before waking worker lanes. Startup reclaim does not have local tokens from the prior process.

The same function powers `axon … recover`, which `ServiceJobRuntime::recover_jobs` (`src/services/runtime.rs:239-248`) wires through to `reclaim_stale_running_jobs_for_table` for a single kind.

### Threshold knobs

| Env var                          | Field                          | Default | Floor |
|----------------------------------|--------------------------------|---------|-------|
| `AXON_JOB_STALE_TIMEOUT_SECS`    | `watchdog_stale_timeout_secs`  | 300     | 30    |
| `AXON_JOB_STALE_CONFIRM_SECS`    | `watchdog_confirm_secs`        | 60      | 10    |

Effective threshold = `(timeout + confirm) * 1_000` ms (default **360 s**). Floors are applied in `src/core/config/parse/build_config.rs:447-448` to prevent dangerously short windows from being configured.

### Crash semantics

If a process dies mid-job:

1. The `running` row's `updated_at` stops advancing.
2. After 360 s (default), the next startup, periodic watchdog tick, or explicit
   `recover` command sweeps it back to `pending`.
3. A worker re-claims it on the next poll.

There is no two-pass `_watchdog` confirmation marker today — the single timeout/confirm sum is the only window. Any caller that needs continuous reclaim during a long-lived process must explicitly invoke `axon <kind> recover` (or call `recover_jobs` through the service runtime).

### Watch leases

`reclaim_stale_watch_leases` (`store.rs:136-145`) clears `lease_expires_at` on `axon_watch_defs` rows whose lease has already expired, so the watch scheduler in `src/jobs/watch.rs` can re-acquire them on the next tick.

## Worker Runtime

In-process workers live entirely in `src/jobs/workers.rs` plus the runner modules under `workers/runners/`. Spawned only by `SqliteJobBackend::new_with_workers`; never by `SqliteJobBackend::new`.

```
spawn_workers (workers.rs:79)
├─ crawl_worker        (1 lane — spider futures are !Send)
├─ embed_worker        (N lanes; AXON_EMBED_LANES; CPU-scaled default 2..=32)
├─ extract_worker      (1 lane)
└─ ingest_worker       (N lanes; AXON_INGEST_LANES; CPU-scaled default 2..=16)
```

Lane count is resolved by `resolve_lane_count(env, cpu_min, cpu_max)` (`workers.rs:23-33`): env wins; otherwise `available_parallelism()` clamped to `[cpu_min, cpu_max]`.

Each lane runs `worker_loop` (`workers.rs:170-238`):

```text
loop {
    select! {
        notify.notified() | sleep(POLL_INTERVAL=5s) | shutdown.cancelled()
    }
    while processed < 32 && !shutdown {
        match claim_next_pending() {
            Some(id) => { run_job(id); mark_completed/mark_failed; processed += 1 }
            None => break,
            Err  => break (logged),
        }
    }
}
```

- All lanes for a kind share the same `Arc<Notify>`. `notify_one()` wakes exactly one waiting lane; SQLite `BEGIN IMMEDIATE` serializes claim attempts, so no semaphore is needed.
- Crawl is forced to a single lane because spider's chrome futures are `!Send` and cannot be moved between tokio worker threads.
- After auto-enqueueing an embed job, the crawl runner pings `embed_notify` so the embed lane wakes without waiting for the 5 s poll.
- `Drop` for `WorkerHandles` cancels the shutdown token and `notify_waiters()` on every lane — joining the worker tasks is graceful: a lane finishes its current job (terminal mark included) before exiting.

`AXON_JOB_WAIT_TIMEOUT_SECS` (default 300 s) bounds `JobBackend::wait_for_job`, used by CLI commands invoked with `--wait true`.

## Data Model

Schema is created by sqlx migrations under `src/jobs/migrations/`:

- `0001_create_tables.sql` — creates the four active job tables.
- `0002_create_watch_tables.sql` — `axon_watch_defs`, `axon_watch_runs`, `axon_watch_run_artifacts` (used by `watch.rs`).
- `0003_add_status_checks.sql` — adds the `status IN (...)` CHECK constraint to every job table.
- `0004_status_created_at_index.sql` — adds an `idx_<kind>_status_created` composite index `(status, created_at DESC)` to all four job tables, keeping the `list_service_jobs` status-filter + `created_at DESC` sort index-friendly as tables grow.
- `0005_add_attempt_metadata.sql` — adds the attempt-tracking columns (`attempt_count`, `active_attempt_id`, `last_reclaimed_at`, `last_reclaimed_reason`) to every job table.
- `0006_create_ingest_payloads.sql` — adds `axon_ingest_payloads` (`job_id` PK, `payload_kind`, `payload_json`, `created_at`), an out-of-band store for large ingest payloads keyed to `axon_ingest_jobs` via `ON DELETE CASCADE`.

Common columns on every active job table:

```
id                    TEXT PRIMARY KEY  -- UUIDv4
status                TEXT NOT NULL DEFAULT 'pending'
config_json           TEXT NOT NULL DEFAULT '{}'
result_json           TEXT              -- live progress + final summary
error_text            TEXT              -- failure reason / reclaim marker
created_at            INTEGER NOT NULL  -- unix millis
updated_at            INTEGER NOT NULL  -- bumped on every mutation
started_at            INTEGER           -- set on claim
finished_at           INTEGER           -- set on terminal mark
attempt_count         INTEGER NOT NULL DEFAULT 0  -- incremented on each claim (0005)
active_attempt_id     TEXT              -- per-claim UUID; gates late writes from reclaimed attempts (0005)
last_reclaimed_at     INTEGER           -- set when a stale running row is reclaimed (0005)
last_reclaimed_reason TEXT              -- reclaim marker text (0005)
```

Plus an `idx_<kind>_status` index on `status` (used by `claim_next_pending` and the cap query) and the `idx_<kind>_status_created` composite `(status, created_at DESC)` index from `0004` (used by the `list_service_jobs` sort).

Per-kind columns:

| Table              | Extra columns                              |
|--------------------|--------------------------------------------|
| `axon_crawl_jobs`  | `url TEXT NOT NULL DEFAULT ''`             |
| `axon_embed_jobs`  | `input_text TEXT NOT NULL DEFAULT ''`      |
| `axon_extract_jobs`| `urls_json TEXT NOT NULL DEFAULT '[]'`     |
| `axon_ingest_jobs` | `source_type TEXT NOT NULL DEFAULT ''`, `target TEXT NOT NULL DEFAULT ''` |

`SqliteConnectOptions` set in `open_sqlite_pool` (`store.rs:43-53`):

- `journal_mode = WAL`
- `busy_timeout = 5000`
- `foreign_keys = ON`
- pool: `max_connections = 8`, `acquire_timeout = 30s`

The DB file is pre-created at mode `0o600` with `O_NOFOLLOW` to close the TOCTOU window where it could be world-readable (`store.rs:28-41`).

## Operational Commands

Each kind exposes the same job-management subcommands. Invocation: `axon <kind> <subcommand>`.

| Subcommand         | Service entry point                              | Effect |
|--------------------|--------------------------------------------------|--------|
| `status <id>`      | `ServiceJobRuntime::job_status`                  | Read row → `ServiceJob` |
| `cancel <id>`      | `ServiceJobRuntime::cancel_job`                  | DB flip + in-memory token (where applicable) |
| `errors <id>`      | `JobBackend::job_errors`; crawl has custom renderer | Generic kinds read `error_text`; crawl also reads `result_json.diagnostic_counts` and bounded `diagnostics` samples |
| `list`             | `job_query::list_jobs` (paginated)              | Most-recent 500, summary view |
| `cleanup`          | `job_query::cleanup_jobs`                       | Delete `completed`/`failed` older than 24 h |
| `clear`            | `job_query::clear_jobs`                         | Delete every row in the table |
| `recover`          | `ServiceJobRuntime::recover_jobs`                | Reclaim stale `running` rows for that kind |
| `worker`           | `ServiceJobRuntime::run_worker`                  | In-process: drains the queue then exits |

The four ingest source aliases (`axon github`, `axon reddit`, `axon youtube`, `axon sessions`) all route through `JobKind::Ingest` and share the `axon_ingest_jobs` table — the `source_type` column distinguishes them.

`--wait true` on a submit command (`crawl`, `extract`, `embed`, `ingest`) constructs a `SqliteJobBackend` with workers, enqueues the job, and polls `wait_for_job` until terminal — bounded by `AXON_JOB_WAIT_TIMEOUT_SECS`.

## Failure Modes

| Layer | Symptom | Root cause | Recovery |
|-------|---------|-----------|----------|
| Worker panic / OOM / kill -9 | Row stuck in `running`, `updated_at` not advancing | Process died mid-job | Stale-job sweep on next boot or `axon <kind> recover` |
| Claim collision under load | `database is locked` | Two lanes raced `BEGIN IMMEDIATE` | `retry_busy` retries up to 5× with 25 ms..400 ms backoff |
| `mark_completed` 0 rows | `mark_completed: job row not found or not in running state` warn | Row was canceled or deleted mid-run | None needed — terminal state already correct |
| Queue cap reached | `JobError::QueueCapacityExceeded { kind, cap, current }` | `pending` count ≥ cap | Wait for workers to drain or raise the env var (`0` = unlimited) |
| Auto-embed deferred | Crawl `result_json.embed_deferred` populated; markdown unindexed | Embed queue at capacity when crawl finished | Drain embed queue, then re-embed the markdown directory manually |
| `wait_for_job` timeout | `job <id> timed out after Ns in state running` | Job exceeded `AXON_JOB_WAIT_TIMEOUT_SECS` | Continue polling via `axon <kind> status <id>`; raise the env var or run with `--wait false` |
| Crawl runner row-missing | `job row not found at execution time, may have been deleted mid-run` warn | Row deleted between claim and execute (e.g. `clear`) | None — runner returns `Ok(None)` and `mark_completed` no-ops |
| Cancel of mid-flight crawl/extract/embed | Row goes `canceled`; runner returns canceled at its safe interruption point | In-flight network or browser work may need a short drain window; crawl also sends Spider shutdown | Continue with `status`; partial crawl progress JSON is retained when it was already persisted |
| Unknown status string in DB | `unknown job status value in DB — treating as Failed` warn | DB hand-edited or schema drift | Restore via SQL or run a fresh DB |

## Source Map

Active code:

- `src/jobs/backend.rs` — `JobBackend` trait, `JobKind`, `JobPayload`, `JobStatusRow`, `JobSummary`, `wait_for_job`
- `src/jobs/status.rs` — `JobStatus` enum + `from_str`/`as_str`
- `src/jobs/error.rs` — `JobError` (`Db`, `JobNotFound`, `AlreadyClaimed`, `Timeout`, `QueueCapacityExceeded`, `Other`)
- `src/jobs/runtime.rs` — `SqliteJobBackend::{new, new_with_workers, new_with_path, init}` and `JobBackend` impl
- `src/jobs/store.rs` — `open_sqlite_pool`, `reclaim_stale_running_jobs`, `reclaim_stale_running_jobs_for_table`, `reclaim_stale_watch_leases`, `now_ms`
- `src/jobs/cancel.rs` — `CancelStore` (in-memory token map)
- `src/jobs/ops/enqueue.rs` — `enqueue_job`, `check_pending_cap_for`, queue-cap `LazyLock` statics
- `src/jobs/ops/lifecycle.rs` — `claim_next_pending`, `mark_completed`, `mark_failed`, `cancel_row`, `update_result_json`
- `src/jobs/ops/retry.rs` — `retry_busy` for transient SQLite lock contention
- `src/jobs/query.rs` — `list_jobs`, `count_jobs`, `cleanup_jobs`, `clear_jobs`, `job_status_row`, `job_errors`
- `src/jobs/workers.rs` — `spawn_workers`, `worker_loop`, `WorkerHandles`, `resolve_lane_count`
- `src/jobs/workers/runners/{crawl,embed,extract,ingest}.rs` — per-kind runner bodies
- `src/jobs/workers/progress.rs` — crawl/embed progress persisters
- `src/jobs/config_snapshot.rs` — submitter→worker config snapshotting
- `src/jobs/migrations/` — `0001_create_tables.sql`, `0002_create_watch_tables.sql`, `0003_add_status_checks.sql`
- `src/jobs/watch.rs` — recurring watch scheduler (separate state machine, not a `JobKind`)
- `src/services/runtime.rs` — `ServiceJobRuntime` trait + `SqliteServiceRuntime` (`recover_jobs`, `notify_worker`, `drain_jobs`, `count_jobs`)
- `src/services/jobs.rs` — `recover_jobs(ctx, kind)` shim used by CLI/MCP

Companion docs:

- `src/jobs/CLAUDE.md` — module layout, helper-function index, and `JobBackend` vs `ServiceJobRuntime` rationale
- `src/services/CLAUDE.md` — services-first contract, `ServiceContext`, `SqliteJobBackend` worker split

Legacy refresh and graph job tables are not created by current migrations.

## App Artifact UX Contract

This section describes the behavior contract between the server and user-facing apps (desktop palette, web panel command palette). It is separate from the automation/agent API contract, which remains unchanged.

### Two tiers of consumers

| Consumer | Behavior | Expectations |
|---|---|---|
| Automation / agents / CLI | Fire-and-forget or explicit `--wait`; receives 202 with `job_id` + `status_url` | Raw JSON, job IDs, absolute paths all acceptable |
| Desktop palette / web panel | Renders artifact handles from completed inline results; generalized app job polling is deferred to `axon_rust-gnb6.5` | No raw absolute paths as primary output; authenticated artifact previews only |

### Accepted-job response (202)

When an async source/extract/operational command is submitted to the server, it
returns HTTP 202:

```json
{
  "job_id": "abc-123",
  "status": "accepted",
  "status_url": "/v1/jobs/abc-123"
}
```

The `status_url` field is the polling path for clients that choose to wait. This artifact UX contract does not change job submission semantics: automation-facing REST job submission routes keep returning `202 AcceptedJob`.

### Terminal job response

Once a job reaches a terminal state, `GET <status_url>` returns:

```json
{
  "job": {
    "status": "completed",
    "url": "https://docs.example.com",
    "result_json": {
      "pages_crawled": 42,
      "docs_embedded": 38,
      "chunks_embedded": 512,
      "elapsed_ms": 14500
    }
  }
}
```

Terminal statuses: `completed`, `failed`, `canceled`, `cancelled`.

App formatters (the Tauri palette's `apps/palette-tauri/src/lib/{payload,format,crawlJob}.ts`, `formatCommandResponse` in `apps/web/app/command-format.ts`) detect the `job` key and branch to the terminal-result rendering path when a terminal job response is already available. Zero-value metrics and sub-second elapsed times are omitted. The target URL or ingest source is shown as the final line.

### Artifact handles

Screenshot commands return an `artifact_handle` alongside the standard response fields:

```json
{
  "url": "https://example.com",
  "artifact_handle": {
    "relative_path": "screenshots/example.com-2024.png",
    "bytes": 153600
  }
}
```

Artifact handles are the app contract. Absolute `path` fields are debug metadata for the server host and must not be used as a primary UI label or preview source.

Web panel previews fetch `/api/panel/artifact/{relative_path}` with panel auth and render object URLs. REST clients use `GET /v1/artifacts?path=<relative_path>` with normal `axon:read` auth. The Tauri palette fetches raster image bytes through its capped artifact bridge command and renders object URLs.

Only raster image artifacts are previewed inline. Active or ambiguous types such as HTML, SVG, and unknown extensions are served as attachments with `X-Content-Type-Options: nosniff`. JSON, markdown, text, and logs keep accurate passive content types but are still attachments.

### What must not regress

- App flows must never display raw absolute server paths (`/home/axon/.axon/...`) as primary output.
- Artifact preview routes must not expose unauthenticated image URLs or accept absolute server paths.
- Automation API endpoints (`POST /v1/sources`, `GET /v1/jobs/{id}`, canonical
  `GET /v1/artifacts?path=...`) must remain available. The removed
  `/v1/crawl` route must remain absent; crawl-like web acquisition is a
  `SourceRequest` with `scope=site`.

### Coverage

| Behavior | Test file | Run |
|---|---|---|
| Artifact bridge + screenshot formatting | `apps/palette-tauri/src-tauri/src/axon_bridge_tests.rs`, `apps/palette-tauri/src/lib/format.test.ts`, `components/palette/OperationResultView.test.tsx` | `cargo test axon_bridge`; `pnpm --dir apps/palette-tauri vitest run ...` |
| Web panel artifact URLs | `apps/web/app/command-format.test.ts` | `npm --prefix apps/web exec vitest run app/command-format.test.ts` |
| Web panel TypeScript types | `apps/web/` | `npm --prefix apps/web run lint` |
