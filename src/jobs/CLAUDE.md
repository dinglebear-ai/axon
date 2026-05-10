# src/jobs — SQLite Job Workers
Last Modified: 2026-05-09

Async job workers. The single backend is `LiteBackend` — SQLite persistence + in-process tokio workers.

## Module Layout

```text
jobs/
├── backend.rs       # JobBackend trait + JobPayload + JobKind + JobStatusRow + JobSummary
├── lite.rs          # LiteBackend: SQLite pool + in-process worker spawning
├── lite/
│   ├── cancel.rs            # SQLite-runtime cancel signaling (status update + spider control)
│   ├── config_snapshot.rs   # Config snapshotting per-job
│   ├── ops.rs / ops/        # Insert/claim/mark/list helpers
│   ├── query.rs             # Service-job query helpers (lite_query::*)
│   ├── store.rs             # Schema bootstrap + lifecycle SQL
│   ├── workers.rs / workers/ # In-process tokio workers, one per JobKind
│   └── migrations/          # SQLite migration files
├── status.rs        # JobStatus enum
├── error.rs         # Job error types
├── crawl.rs         # Crawl job schema/payload helpers
├── crawl/sitemap.rs # Sitemap helpers used by the crawl worker
├── embed.rs         # Embed job schema/payload helpers
├── extract.rs       # Extract job schema/payload helpers
├── ingest.rs        # Ingest job schema + payload (github/reddit/youtube/sessions)
├── ingest/{tests,types}.rs
└── watch_lite.rs    # Watch task scheduler (SQLite-backed, in-process)
```

There are no longer separate `crawl/{processor,repo,watchdog,worker,runtime}.rs` or `embed/`/`extract/` worker subdirs — those workers were consolidated into `src/jobs/lite/workers.rs` (and its sibling `workers/` submodule directory) when full mode was retired.

## Backend Selection

`ServiceContext::new(cfg)` calls `resolve_runtime(cfg)` in `src/services/runtime.rs`, which always returns a `LiteServiceRuntime`:

```rust
LiteServiceRuntime { backend: LiteBackend::new(cfg).await? }
```

**LiteBackend:**
- Opens a single SQLite pool (`AXON_SQLITE_PATH` env or `$AXON_DATA_DIR/jobs.db` → `~/.axon/jobs.db` by default — `AXON_DATA_DIR` defaults to `~/.axon`, flat layout)
- Spawns in-process tokio workers at startup — no external message broker needed
- Do NOT call `open_config_pool()` before `LiteBackend::new()` — the backend opens its own pool internally

### LiteBackend / ServiceContext Worker Split

- `LiteBackend::new(cfg)` = **enqueue-only**, no workers. Safe for CLI fire-and-forget.
- `LiteBackend::new_with_workers(cfg)` = spawns in-process workers. Use in serve/mcp.
- **Why:** CLI fire-and-forget with workers claims jobs then exits, orphaning them.

## `JobBackend` Trait (`backend.rs`)

> **`JobBackend` is NOT the canonical abstraction.** The canonical trait consumed by all callers (CLI, MCP) is [`ServiceJobRuntime`](../services/runtime.rs) in `src/services/runtime.rs`, which returns the richer `ServiceJob` type and adds pagination, `has_active_jobs`, `recover_jobs`, and `run_worker`.
>
> In practice, only **3 of 8** `JobBackend` methods are delegated through the trait by the service layer: `enqueue`, `wait_for_job`, and `job_errors`. These return simple types (`Uuid`, `String`, `Option<String>`) that need no mapping. The remaining methods (`list_jobs`, `job_status`, `cancel_job`, `cleanup_jobs`, `clear_jobs`) are **bypassed** — `LiteServiceRuntime` calls `lite_query::*` directly to avoid lossy type mapping from `JobStatusRow`/`JobSummary` → `ServiceJob`.

The low-level persistence interface:

```rust
#[async_trait]
pub trait JobBackend: Send + Sync {
    async fn enqueue(&self, payload: JobPayload) -> BackendResult<JobId>;
    async fn job_status(&self, id: JobId, kind: JobKind) -> BackendResult<Option<JobStatusRow>>;
    async fn cancel_job(&self, id: JobId, kind: JobKind) -> BackendResult<bool>;
    async fn list_jobs(&self, kind: JobKind) -> BackendResult<Vec<JobSummary>>;
    async fn cleanup_jobs(&self, kind: JobKind) -> BackendResult<u64>;
    async fn clear_jobs(&self, kind: JobKind) -> BackendResult<u64>;
    async fn job_errors(&self, id: JobId, kind: JobKind) -> BackendResult<Option<String>>;
    async fn wait_for_job(&self, id: JobId, kind: JobKind) -> BackendResult<String>;
}
```

`wait_for_job()` polls until the job reaches a terminal state — used to keep the process alive while in-process workers finish. Times out after `AXON_JOB_WAIT_TIMEOUT_SECS` (default 300s).

**`JobPayload`** variants: `Crawl { url, config_json }`, `Embed { input, config_json }`, `Extract { urls, config_json }`, `Ingest { target, source_type, config_json }`.

**`JobKind`** variants with table names: `Crawl` → `axon_crawl_jobs`, `Embed` → `axon_embed_jobs`, `Extract` → `axon_extract_jobs`, `Ingest` → `axon_ingest_jobs`.

## Critical Patterns

### Job Lifecycle

Always use the SQLite store functions — never write raw SQL job state updates:

```text
claim_next_pending() → mark_job_started() → mark_job_completed() / mark_job_failed()
```

### JobStatus Enum (`status.rs`)

Use `JobStatus::Pending` etc. — **never** raw strings like `"pending"`, `"running"`, `"completed"`, `"failed"`, `"canceled"`. Serializes to the SQL strings automatically.

### SQLite Pool — Create Once, Pass Down

The SQLite pool is expensive. `LiteBackend::new()` creates one pool at startup and passes it to all helper functions. Do not create pools inside loops or per-job handlers.

**SQLite PRAGMAs**: use `SqliteConnectOptions::pragma()`, NOT `sqlx::query("PRAGMA...")`.

### Bounded Channels

All internal async channels use `tokio::sync::mpsc::channel(256)` — **never** `unbounded_channel()`. Unbounded channels hide backpressure bugs and cause OOM under load.

### Liveness Enforcement (Heartbeat + Watchdog)

Two cooperating mechanisms keep job state honest:

**Heartbeat (per running job):**
- `HeartbeatGuard` in `src/jobs/lite/workers/heartbeat.rs` is spawned by `worker_loop` for every claimed job and aborted (RAII drop) when the runner returns.
- Loops every 30s and calls `touch_heartbeat()` (in `lite/ops/lifecycle.rs`) which bumps `updated_at` only on rows still in `running` state. It never writes `result_json` — that column is owned by the progress persisters.
- Purpose: keep `updated_at` advancing during long blocking phases (crawl rendering a single page, embed pipeline mid-batch) where no progress event has fired yet.

**Watchdog (periodic + startup):**
- Startup-time sweep: `LiteBackend::init` calls `reclaim_stale_running_jobs` once, resetting any `running` row whose `updated_at < now - (watchdog_stale_timeout_secs + watchdog_confirm_secs)` to `pending`.
- Periodic sweep: `spawn_workers` spawns a 60s ticker that re-runs `reclaim_stale_running_jobs` while the process is alive, cooperating with the heartbeat to detect both **crash** (process gone, heartbeat stopped) and **hang** (heartbeat task wedged) cases.
- Thresholds via `cfg.watchdog_stale_timeout_secs` (default 300s) and `cfg.watchdog_confirm_secs` (60s) → 360s total. With a 30s heartbeat that gives ~12x safety margin.

### Cancellation

All four job runners (`crawl`, `embed`, `extract`, `ingest`) accept an `Option<CancellationToken>`. `worker_loop` registers a token in the shared `CancelStore` for each claimed job, runs the job future, and removes the token when the runner returns.

`LiteBackend::cancel_job` calls `CancelStore::cancel`, which (a) flips the SQLite row to `canceled` and (b) fires the in-memory token. Each runner observes the token at its safe interruption points:

- **crawl**: top-level `tokio::select!` between `token.cancelled()` and the engine future. On cancel, the runner sends `spider::utils::shutdown("{job_id}{url}")` to the active Spider control target, waits briefly for drain, and returns canceled. The row remains `canceled`; any progress JSON already persisted by the crawl progress task is kept.
- **embed**: top-level `tokio::select!` between `token.cancelled()` and the engine future. Cancel returns immediately; in-flight network IO inside the engine may continue briefly but its result is dropped.
- **extract**: per-URL check before each iteration plus a `select!` around the per-URL extract future.
- **ingest**: Reddit consumes the token natively (mid-loop); GitHub / YouTube / Sessions are wrapped in `tokio::select!` at the runner boundary.

When the runner exits with `Err("<kind> canceled")`, the worker loop calls `mark_failed`. Because `mark_failed`'s `WHERE status='running'` guard already failed (the row is now `canceled`), the late-arriving terminal write is silently dropped — the row stays `canceled`. This is the intended semantics.

### Stale Job Recovery

- The SQLite-runtime watchdog (in `src/jobs/lite/store.rs::reclaim_stale_running_jobs`) marks jobs stuck in `running` state as `pending` after the stale timeout, both at startup and on the periodic 60s tick from `spawn_workers`.
- `axon crawl recover` subcommand: reclaims all stale jobs (re-queues them as `pending`).

## ingest_jobs Schema Difference
`axon_ingest_jobs` uses `source_type` + `target` columns instead of `url`/`urls_json` used by all other job tables. When querying or listing ingest jobs, join/filter on `source_type` (`github`/`reddit`/`youtube`) not on `url`.

## Testing

```bash
cargo test jobs           # all job-related unit tests
cargo test crawl_jobs     # crawl pipeline tests
cargo test status         # JobStatus enum serialization tests
cargo test -- --nocapture # show log output from tests
```

Unit tests (enum, serialization) run without live services. Integration tests that call `LiteBackend::new()` need an `AXON_SQLITE_PATH` or writable `AXON_DATA_DIR`.

## Adding a New Job Type
1. Create `<name>.rs` (or `<name>/` module if complex)
2. Add schema helper + `ensure_schema()` call in the worker startup — it's idempotent
3. Reuse `lite/store.rs` helpers for claim/mark/enqueue operations
4. Add `JobKind::<Name>` variant in `backend.rs`
5. Add `JobPayload::<Name>` variant in `backend.rs`
6. Wire up the in-process worker in `lite/workers.rs`
