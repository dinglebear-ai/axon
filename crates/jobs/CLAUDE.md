# crates/jobs — Job Workers (Full + Lite)
Last Modified: 2026-03-28

Async job workers. Two backends share the `JobBackend` trait:
- **FullBackend** — Postgres persistence + RabbitMQ dispatch (default; requires `AXON_PG_URL` + `AXON_AMQP_URL`)
- **LiteBackend** — SQLite persistence + in-process tokio workers (enabled via `AXON_LITE=1` or `--lite`)

## Module Layout

```text
jobs/
├── backend.rs       # JobBackend trait + JobPayload + JobKind + JobStatusRow + JobSummary
├── full.rs          # FullBackend: Postgres + RabbitMQ (wraps existing per-job-type functions)
├── lite.rs          # LiteBackend: SQLite pool + in-process worker spawning
├── lite/            # LiteBackend submodules: cancel, ops, query, store, workers
├── status.rs        # JobStatus enum
├── common/          # Shared infra: pool, AMQP channel, claim/mark/enqueue (FullBackend path)
├── crawl/           # manifest, processor, repo, sitemap, watchdog, worker, runtime
├── extract/         # Extract worker
├── embed/           # Embed worker
├── refresh/         # Periodic URL re-indexing scheduler (RefreshSchedule CRUD + worker)
├── ingest.rs        # Ingest job schema + worker (github/reddit/youtube/sessions)
└── worker_lane.rs   # Generic AMQP/polling lane runtime module root — used by embed, extract, and refresh workers
                     # (Crawl uses its own loop in crawl/runtime/worker/loops.rs due to !Send spider futures)
```

## Backend Selection (`AXON_LITE=1`)

`ServiceContext::new(cfg)` calls `resolve_runtime(cfg)` in `crates/services/runtime.rs`, which selects the backend:

```rust
if cfg.lite_mode {
    LiteServiceRuntime { backend: LiteBackend::new(cfg).await? }
} else {
    FullServiceRuntime { backend: FullBackend::new(cfg) }
}
```

`cfg.lite_mode` is set when `AXON_LITE=1` env var is present or `--lite` flag is passed.

**LiteBackend:**
- Opens a single SQLite pool (`AXON_SQLITE_PATH` env or `$AXON_DATA_DIR/axon/jobs.db`)
- Spawns in-process tokio workers at startup — no external AMQP workers needed
- Graph, refresh scheduling, and watch scheduler are **unsupported** in lite mode (guarded by `ServiceCapabilities`)
- Do NOT call `open_config_pool()` before `LiteBackend::new()` — the backend opens its own pool internally

**FullBackend:**
- Thin adapter over the existing per-job-type enqueue/query Postgres + RabbitMQ functions
- Workers remain separate processes (s6 workers in Docker, or `axon <cmd> worker` locally)
- `lift_err()` stringifies `Box<dyn Error>` to satisfy `Send+Sync` bounds on `BackendResult`

## `JobBackend` Trait (`backend.rs`)

> **`JobBackend` is NOT the canonical abstraction.** The canonical trait consumed by all callers (CLI, MCP, web) is [`ServiceJobRuntime`](../services/runtime.rs) in `crates/services/runtime.rs`, which returns the richer `ServiceJob` type and adds pagination, `has_active_jobs`, `recover_jobs`, and `run_worker`.
>
> In practice, only **3 of 8** `JobBackend` methods are delegated through the trait by the service layer: `enqueue`, `wait_for_job`, and `job_errors`. These return simple types (`Uuid`, `String`, `Option<String>`) that need no mapping. The remaining methods (`list_jobs`, `job_status`, `cancel_job`, `cleanup_jobs`, `clear_jobs`) are **bypassed** — `FullServiceRuntime` calls raw Postgres query functions directly, and `LiteServiceRuntime` calls `lite_query::*` directly, to avoid lossy type mapping from `JobStatusRow`/`JobSummary` → `ServiceJob`.

The low-level persistence interface that both backends implement:

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

`wait_for_job()` polls until the job reaches a terminal state — used in lite mode to keep the process alive while in-process workers finish. Times out after `AXON_JOB_WAIT_TIMEOUT_SECS` (default 300s).

**`JobPayload`** variants: `Crawl { url, config_json }`, `Embed { input, config_json }`, `Extract { urls, config_json }`, `Ingest { target, source_type, config_json }`, `Refresh { url, config_json }`, `Graph { config_json }`.

**`JobKind`** variants with table names: `Crawl` → `axon_crawl_jobs`, `Embed` → `axon_embed_jobs`, `Extract` → `axon_extract_jobs`, `Ingest` → `axon_ingest_jobs`, `Refresh` → `axon_refresh_jobs`, `Graph` → `axon_graph_jobs`.

## Critical Patterns

### Job Lifecycle

Always use `common::` functions — never write raw SQL job state updates:

```text
claim_next_pending() → mark_job_started() → mark_job_completed() / mark_job_failed()
```

### JobStatus Enum (`status.rs`)

Use `JobStatus::Pending` etc. — **never** raw strings like `"pending"`, `"running"`, `"completed"`, `"failed"`, `"canceled"`. Serializes to the SQL strings automatically.

### PgPool — Create Once, Pass Down

PgPool is expensive. Each worker creates one pool at startup and passes `&PgPool` to all helper functions. Helpers are named `*_with_pool()`. Do not create pools inside loops or per-job handlers.

### AMQP Channel (`common/`)

`open_amqp_channel()` has a **5-second connection timeout**. On failure it returns an error — callers should backoff and retry at the worker loop level, not in the channel helper itself.

### AMQP Reconnect Backoff (crawl worker)

`run_amqp_lane_with_reconnect()` in `crawl/runtime/worker/loops.rs` wraps the consumer loop in an infinite reconnect cycle. When the channel dies (broker restart, consumer_timeout, network blip):
- Backoff starts at **2s**, doubles on each attempt, capped at **60s**
- On successful reconnect the backoff resets to 2s
- In-flight jobs are not lost — they hold no AMQP reference and complete normally before reconnect fires

### Bounded Channels

All internal async channels use `tokio::sync::mpsc::channel(256)` — **never** `unbounded_channel()`. Unbounded channels hide backpressure bugs and cause OOM under load.

### Stale Job Recovery

- `watchdog.rs` (crawl_jobs): marks jobs stuck in `running` state as `failed` after `AXON_JOB_STALE_TIMEOUT_SECS` (default 300s) + `AXON_JOB_STALE_CONFIRM_SECS` (60s) grace period
- `axon crawl recover` subcommand: reclaims all stale jobs (re-queues them as `pending`)

### Refresh Module (`refresh/`)

`refresh/` implements **periodic URL re-indexing**: users create `RefreshSchedule` records (via `create_refresh_schedule`) that specify a URL and recurrence. `claim_due_refresh_schedules` polls for overdue schedules, enqueues re-crawl jobs, and updates `last_ran_at`. The worker (`run_refresh_worker`) runs as a separate s6 service (`refresh-worker`) and loops via the `worker_lane.rs` module.

Key exported API: `create_refresh_schedule`, `delete_refresh_schedule`, `list_refresh_schedules`, `set_refresh_schedule_enabled`, `start_refresh_job`, `recover_stale_refresh_jobs_startup`.

### AMQP Reconnect Backoff — Crawl vs Others

Two different reconnect semantics exist in this codebase:

| Worker | Location | Backoff reset condition |
|--------|----------|------------------------|
| `embed`, `extract`, `refresh` | `worker_lane.rs` module | Resets to 2s **only** if connection was alive ≥60s |
| `crawl` | `crawl/runtime/worker/loops.rs` | Resets to 2s on **every** successful reconnect |

The crawl worker's simpler policy is intentional — spider.rs futures are `!Send` and the crawl worker loop has different lifetime semantics than the generic lane. Do not "fix" one to match the other.

### worker_lane.rs Module (Embed / Extract / Refresh)

`worker_lane.rs` is the **generic** AMQP/polling lane runtime module root shared by embed, extract, and refresh workers. The crawl worker does **not** use it — crawl has its own loop in `crawl/runtime/worker/loops.rs` because spider.rs futures are `!Send` and require single-threaded pinning.

`AXON_INGEST_LANES` (default 2) controls how many ingest jobs run in parallel via `worker_lane.rs`. Each lane holds one AMQP consumer. Lane count is separate from per-job concurrency.

## ingest_jobs Schema Difference
`axon_ingest_jobs` uses `source_type` + `target` columns instead of `url`/`urls_json` used by all other job tables. When querying or listing ingest jobs, join/filter on `source_type` (`github`/`reddit`/`youtube`) not on `url`.

## Testing

```bash
cargo test jobs           # all job-related unit tests
cargo test common         # shared infra tests (pool, channel, claim/mark)
cargo test crawl_jobs     # crawl pipeline tests
cargo test status         # JobStatus enum serialization tests
cargo test -- --nocapture # show log output from tests
```

**Important:** Integration tests that exercise `make_pool`, `open_amqp_channel`, or `claim_next_pending` require live Postgres + RabbitMQ connections. Run `docker compose up -d axon-postgres axon-rabbitmq` before running integration tests. Unit tests (enum, serialization, rule engine) run without services.

## Adding a New Job Type
1. Create `<name>.rs` (or `<name>/` module if complex)
2. Call `ensure_schema()` in the worker startup — it's idempotent
3. Reuse `common::make_pool`, `open_amqp_channel`, `claim_next_pending`, `enqueue_job`
4. Add `CommandKind::<Name>` to `config.rs`
5. Add s6 worker script in `docker/s6/s6-rc.d/<name>-worker/`
