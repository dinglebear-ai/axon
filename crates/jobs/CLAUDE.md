# crates/jobs — Job Workers (Lite)
Last Modified: 2026-04-27

Async job workers. The single backend is `LiteBackend` — SQLite persistence + in-process tokio workers.

## Module Layout

```text
jobs/
├── backend.rs       # JobBackend trait + JobPayload + JobKind + JobStatusRow + JobSummary
├── lite.rs          # LiteBackend: SQLite pool + in-process worker spawning
├── lite/
│   ├── cancel.rs            # Lite-mode cancel signaling (status update + spider control)
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

There are no longer separate `crawl/{processor,repo,watchdog,worker,runtime}.rs` or `embed/`/`extract/` worker subdirs — those workers were consolidated into `crates/jobs/lite/workers.rs` (and its sibling `workers/` submodule directory) when full mode was retired.

## Backend Selection

`ServiceContext::new(cfg)` calls `resolve_runtime(cfg)` in `crates/services/runtime.rs`, which always returns a `LiteServiceRuntime`:

```rust
LiteServiceRuntime { backend: LiteBackend::new(cfg).await? }
```

**LiteBackend:**
- Opens a single SQLite pool (`AXON_SQLITE_PATH` env or `$AXON_DATA_DIR/axon/jobs.db`)
- Spawns in-process tokio workers at startup — no external message broker needed
- Do NOT call `open_config_pool()` before `LiteBackend::new()` — the backend opens its own pool internally

### LiteBackend / ServiceContext Worker Split

- `LiteBackend::new(cfg)` = **enqueue-only**, no workers. Safe for CLI fire-and-forget.
- `LiteBackend::new_with_workers(cfg)` = spawns in-process workers. Use in serve/mcp.
- **Why:** CLI fire-and-forget with workers claims jobs then exits, orphaning them.

## `JobBackend` Trait (`backend.rs`)

> **`JobBackend` is NOT the canonical abstraction.** The canonical trait consumed by all callers (CLI, MCP) is [`ServiceJobRuntime`](../services/runtime.rs) in `crates/services/runtime.rs`, which returns the richer `ServiceJob` type and adds pagination, `has_active_jobs`, `recover_jobs`, and `run_worker`.
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

Always use the lite store functions — never write raw SQL job state updates:

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

### Liveness Enforcement (Two Tiers)

**Tier 1 — Dead-process detection (watchdog):**
Reclaims jobs where `updated_at` goes stale (process died, heartbeat stopped).
- Threshold: `AXON_JOB_STALE_TIMEOUT_SECS` (default 300s) + `AXON_JOB_STALE_CONFIRM_SECS` (60s)
- Implemented in `crawl/watchdog.rs`

**Tier 2 — Stuck-process detection (content-aware heartbeat):**
Detects jobs that are alive (heartbeat touching `updated_at`) but making no progress (`result_json` unchanged).
- Warn at `STALE_STREAK_WARN_THRESHOLD` = 6 intervals × 30s = **3 min** no progress
- Kill at `STALE_STREAK_KILL_THRESHOLD` = 20 intervals × 30s = **10 min** no progress

The watchdog handles the **crash** case (process died). The heartbeat handles the **hang** case (process alive, job stuck).

### Stale Job Recovery

- The lite-mode watchdog (in `crates/jobs/lite/`) marks jobs stuck in `running` state as `failed` after the stale timeout. The exact submodule name was renamed during the lite-mode collapse — search for `STALE_TIMEOUT` / `recover_jobs` in `crates/jobs/lite/` if the path matters.
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
