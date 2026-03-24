# crates/jobs — AMQP Job Workers
Last Modified: 2026-03-24

Async job workers backed by RabbitMQ (lapin) + PostgreSQL (sqlx).

## Module Layout

```text
jobs/
├── common/          # Shared infra: pool, AMQP channel, claim/mark/enqueue
├── crawl/           # manifest, processor, repo, sitemap, watchdog, worker, runtime
├── extract/         # Extract worker
├── embed/           # Embed worker
├── refresh/         # Periodic URL re-indexing scheduler (RefreshSchedule CRUD + worker)
├── ingest.rs        # Ingest job schema + worker (github/reddit/youtube/sessions)
├── status.rs        # JobStatus enum
└── worker_lane.rs   # Generic AMQP/polling lane runtime module root — used by embed, extract, and refresh workers
                     # (Crawl uses its own loop in crawl/runtime/worker/loops.rs due to !Send spider futures)
```

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

### Liveness Enforcement (Two Tiers)

**Tier 1 — Dead-process detection (watchdog):**
Reclaims jobs where `updated_at` goes stale (process died, heartbeat stopped).
- Threshold: `AXON_JOB_STALE_TIMEOUT_SECS` (default 300s) + `AXON_JOB_STALE_CONFIRM_SECS` (60s)
- Implemented in `watchdog.rs` (crawl_jobs) and `worker_lane.rs` stale sweep

**Tier 2 — Stuck-process detection (content-aware heartbeat):**
Detects jobs that are alive (heartbeat touching `updated_at`) but making no progress (`result_json` unchanged).
- Warn at `STALE_STREAK_WARN_THRESHOLD` = 6 intervals × 30s = **3 min** no progress
- Kill at `STALE_STREAK_KILL_THRESHOLD` = 20 intervals × 30s = **10 min** no progress
  - Heartbeat calls `CancellationToken::cancel()` → `wrap_with_heartbeat` in `worker_lane.rs` aborts inner future
  - `mark_job_failed` writes `failed` status with reason → semaphore permit released → lane slot freed

The watchdog handles the **crash** case (process died). The heartbeat handles the **hang** case (process alive, job stuck).

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
