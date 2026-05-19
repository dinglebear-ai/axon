# Lite Mode — Dependency-Free Job Queue & Pipeline

**Date:** 2026-03-24
**Status:** Approved — ready for implementation planning
**Branch target:** `feat/lite-mode`

---

## Problem

Running axon currently requires three external services just to track job progress and queue work:

- **Postgres** — job persistence (6 tables, state machine)
- **RabbitMQ** — job dispatch (AMQP queues, worker consumers)
- **Redis** — cancel signals and health checks

This is excessive for single-machine use. Spider.rs — the library axon wraps — needs none of it. The barrier to entry is high and the operational overhead is real. A newcomer has to understand Docker Compose, three services, and a separate worker process just to crawl a website.

---

## Goal

A **lite mode** that replaces all three services with:

- **SQLite** — job persistence (same schema, same state machine)
- **In-process tokio tasks** — workers run inside the same process as the CLI command
- **In-memory `CancellationToken` map** — cancel signals

Lite mode must offer **feature parity** with the full stack for single-machine use. It runs as a **dual mode** alongside the full stack — both compile into the same binary, selected at runtime via `AXON_LITE=1` or `--lite`. The full stack is preserved as an opt-in for future multi-machine scaling validation.

**Migration path:** Run lite in production, confirm parity, then remove the full stack entirely.

---

## Architecture

### Backend Trait

A `JobBackend` trait abstracts over both modes. The rest of the codebase receives `Arc<dyn JobBackend>` and never touches storage directly.

```
┌─────────────────────────────────────────────────────────┐
│                    axon CLI / MCP                       │
└────────────────────┬────────────────────────────────────┘
                     │
              ┌──────▼──────┐
              │ JobBackend  │  (trait, Arc<dyn>, runtime dispatch)
              └──────┬──────┘
          ┌──────────┴──────────┐
   ┌──────▼──────┐       ┌──────▼──────┐
   │ LiteBackend │       │ FullBackend │
   │ SQLite +    │       │ Postgres +  │
   │ tokio tasks │       │ AMQP +      │
   └─────────────┘       │ Redis       │
                         └─────────────┘
```

**`JobBackend` trait (approximate method surface):**

```rust
#[async_trait]
pub trait JobBackend: Send + Sync {
    async fn enqueue_crawl(&self, url: &str, config: &Config) -> Result<JobId>;
    async fn enqueue_embed(&self, input: &str, config: &Config) -> Result<JobId>;
    async fn enqueue_extract(&self, urls: &[String], config: &Config) -> Result<JobId>;
    async fn enqueue_ingest(&self, target: &str, config: &Config) -> Result<JobId>;
    // ... one enqueue per job type

    async fn job_status(&self, id: JobId, kind: CommandKind) -> Result<Option<JobStatusRow>>;
    async fn cancel_job(&self, id: JobId, kind: CommandKind) -> Result<bool>;
    async fn list_jobs(&self, kind: CommandKind) -> Result<Vec<JobSummary>>;
    async fn cleanup_jobs(&self, kind: CommandKind) -> Result<u64>;
}
```

The exact signatures will be refined during implementation. The key constraint: no storage-specific types leak through the trait boundary.

**Construction site:** `Arc<dyn JobBackend>` is created once in `lib.rs::run()` (or `run_once()`) immediately after `Config` is resolved — before any command dispatch. The `Arc` is threaded into `RunContext` (or equivalent) and passed to CLI handlers, MCP routes, and web routes via the existing context-passing pattern.

### Crate Layout

```
crates/jobs/
├── backend.rs          ← JobBackend trait
├── lite/
│   ├── store.rs        ← SQLite pool, migrations, job ops
│   ├── workers.rs      ← in-process tokio task workers
│   └── cancel.rs       ← DashMap<JobId, CancellationToken>
├── full/               ← existing code, reorganized
│   ├── pool.rs
│   ├── amqp.rs
│   └── ...
└── common/             ← JobStatus, JobId, JobPayload, watchdog, heartbeat
```

### Mode Selection

```bash
# env var — add to .env for persistent lite mode
AXON_LITE=1 axon crawl https://example.com

# CLI flag — one-off
axon --lite crawl https://example.com
```

`Config` gains:
- `lite_mode: bool` — set from `AXON_LITE` env or `--lite` flag
- `sqlite_path: PathBuf` — set from `AXON_SQLITE_PATH` env or `--sqlite-path` flag

Both modes compile into the same binary. No separate builds. No compile-time feature gating (yet).

---

## SQLite Job Store

### Schema

Same 6 tables as Postgres (`axon_crawl_jobs`, `axon_embed_jobs`, `axon_extract_jobs`, `axon_ingest_jobs`, `axon_refresh_jobs`, `axon_graph_jobs`). SQLite-compatible types:

```sql
CREATE TABLE IF NOT EXISTS axon_crawl_jobs (
    id          TEXT PRIMARY KEY,   -- UUID as text
    status      TEXT NOT NULL DEFAULT 'pending',
    url         TEXT NOT NULL,
    config_json TEXT NOT NULL,
    result_json TEXT,
    error_text  TEXT,
    created_at  INTEGER NOT NULL,   -- Unix ms
    updated_at  INTEGER NOT NULL,
    started_at  INTEGER,
    finished_at INTEGER
);
```

### Atomic Job Claiming

SQLite lacks `FOR UPDATE SKIP LOCKED` but `BEGIN IMMEDIATE` achieves the same result — write-locks the DB for the duration of the transaction, serializing concurrent claim attempts from multiple tokio tasks:

```sql
BEGIN IMMEDIATE;
SELECT id FROM axon_crawl_jobs WHERE status='pending' ORDER BY created_at LIMIT 1;
UPDATE axon_crawl_jobs SET status='running', started_at=? WHERE id=?;
COMMIT;
```

### Connection

- `sqlx::SqlitePool` with `max_connections=4`
- `PRAGMA journal_mode=WAL` at connect time — readers never block writers
- Migrations via `sqlx::migrate!` at startup

### Multi-Process Access

Multiple `axon` processes running simultaneously against the same `jobs.db` is fully supported. SQLite WAL mode + `BEGIN IMMEDIATE` serializes concurrent writes correctly — two processes claiming jobs will never double-claim. Each process manages its own in-process workers and jobs independently.

**Cross-process cancel:** `axon crawl cancel <id>` from one terminal correctly cancels a job running in another terminal via a two-step mechanism: (1) UPDATE the SQLite row to `canceled`, (2) each running job polls `WHERE id=? AND status='canceled'` every 3 seconds — same interval as the current Redis cancel poll. On detection, the in-process `CancellationToken` is fired. This means cancel latency is up to 3 seconds, identical to the current Redis behavior.

**Cross-process job pickup:** Each process only processes jobs it enqueued itself — `Notify` handles are in-process. A job enqueued by Process A will not be picked up by Process B's workers. This is the expected behavior: each `axon` invocation is self-contained.

### File Location (priority order)

1. `--sqlite-path <path>` / `AXON_SQLITE_PATH`
2. `$XDG_DATA_HOME/axon/jobs.db`
3. `~/.local/share/axon/jobs.db`
4. `./axon-jobs.db` (fallback)

---

## In-Process Worker Engine

### How Workers Run

`LiteBackend::new()` spawns one tokio task per job type at startup. Each worker loops:

1. Wait on `tokio::sync::Notify` (woken by `enqueue_job`) or a 5s poll timeout
2. `claim_next_pending()` — atomic SQLite claim
3. Run the job handler (same `run_crawl_once`, `run_embed_once`, etc.)
4. `mark_job_completed()` / `mark_job_failed()`
5. Back to step 1

```
enqueue_job()
    ├─ INSERT INTO axon_crawl_jobs (status='pending')
    └─ notify.notify_one()
              │
              ▼
    crawl_worker_task (sleeping on notify.notified())
              ├─ claim_next_pending()
              ├─ run_crawl_once(...)
              └─ mark_completed() / mark_failed()
```

### UX Change

| Today (full stack) | Lite mode |
|---|---|
| Terminal 1: `axon crawl worker` | (not needed) |
| Terminal 2: `axon crawl https://example.com` | `axon crawl https://example.com` |

In lite mode, workers start automatically when the process starts. The process exits when the job completes (or immediately on `--wait false`).

### `--wait` Semantics (preserved)

- `--wait true` (default for most commands) — process stays alive until job done
- `--wait false` — enqueues, prints job ID, exits. Because workers are in-process, the full job lifecycle is:
  `pending → running → (process exits, worker task dies) → pending (reclaimed by watchdog on next invocation)`
  The job is written to SQLite as `running` before the worker task is killed. On next startup, the watchdog reclaims it to `pending` and workers pick it up again. Partial crawl results are not preserved across this cycle — the job restarts from the beginning.

### Worker Concurrency

One worker task per job type. Actual crawl concurrency is handled inside spider.rs (hundreds of concurrent HTTP connections) — the worker task is just the orchestrator. No need for multiple worker tasks per type.

### `axon crawl worker` in Lite Mode

Returns a friendly no-op:
```
Lite mode is active — workers run in-process automatically.
Run 'axon crawl https://example.com' directly; no separate worker needed.
```

---

## Cancel & Watchdog

### Cancel (replaces Redis)

`LiteBackend` holds `cancel_tokens: DashMap<JobId, CancellationToken>`.

On job claim: insert a fresh token. Pass a clone to the job handler.

On `axon crawl cancel <id>`:
```
cancel(id)
    ├─ UPDATE ... SET status='canceled' WHERE id=?
    └─ tokens.get(&id) → token.cancel()   ← fires immediately for same-process jobs
```

For same-process cancel: token fires immediately, spider shuts down, partial results saved. For cross-process cancel: the UPDATE propagates; the running job's 3-second SQLite poll detects `status='canceled'` and fires its local `CancellationToken`. Cancel latency is up to 3 seconds — identical to the current Redis poll interval.

Token removed from map when job finishes (any terminal state).

### Watchdog (startup reclaim)

On `LiteBackend::new()`, before workers start:

```sql
UPDATE axon_crawl_jobs
SET status='pending', error_text='reclaimed after unexpected shutdown'
WHERE status='running'
  AND updated_at < (unixepoch('subsec') * 1000) - :stale_threshold_ms
```

Threshold: `(AXON_JOB_STALE_TIMEOUT_SECS + AXON_JOB_STALE_CONFIRM_SECS) * 1000`. Same values as full stack.

Jobs stuck in `running` at startup (from a previous crash) are reset to `pending` and picked up by workers immediately.

Jobs killed mid-run with no token (SIGKILL, OOM) are handled correctly — no token needed for the reclaim path.

### Heartbeat Watchdog

The ongoing heartbeat watchdog (background task monitoring `result_json` for progress, killing stuck jobs via `CancellationToken` after 10 min) is **unchanged** — it's already storage-agnostic and receives the job's `CancellationToken` directly.

---

## `axon doctor` in Lite Mode

Reports SQLite file path, accessibility, and job counts by status. Does **not** check Postgres/Redis/AMQP connectivity (those aren't running). Reports Qdrant, TEI, and Chrome as usual.

All existing job management subcommands work identically:
- `axon crawl list` — reads SQLite
- `axon crawl cancel <id>` — SQLite + in-memory token
- `axon crawl errors <id>` — reads `error_text` from SQLite
- `axon crawl cleanup` / `clear` — deletes SQLite rows
- `axon status` — reads all job tables from SQLite

---

## Dependencies

**Add:** `sqlx` `sqlite` feature (same crate already used for Postgres — just add the feature flag)

**Add:** `AXON_LITE` and `AXON_SQLITE_PATH` to `.env.example`

**Timestamp normalization:** Postgres uses `TIMESTAMPTZ` (RFC3339 strings via sqlx); SQLite stores Unix milliseconds as `INTEGER`. Any code that surfaces timestamps to the web UI, MCP layer, or `axon status` output must normalize through a shared `JobStatusRow` type with `DateTime<Utc>` fields — the backend implementations handle the conversion internally, so callers always see consistent types.

**Keep (for now):** `lapin`, `redis`, `sqlx/postgres` — full stack stays until parity is confirmed

**Remove (future):** `lapin`, `redis`, `sqlx/postgres` once full stack is retired. Three services drop out of `docker-compose.services.yaml` — only Qdrant, TEI, and Chrome remain.

---

## What Does NOT Change

- All CLI commands and flags — same interface
- Job state machine (`JobStatus` enum, same transitions)
- `result_json` / `config_json` payload format
- Heartbeat, watchdog thresholds and logic
- Spider.rs crawl engine, TEI embedding, Qdrant upsert — none of this touches the job backend
- MCP server — calls the same service layer, gets the same results
- Web UI — same WebSocket bridge, same job status polling

---

## Infrastructure Impact

| Service | Full stack | Lite mode |
|---|---|---|
| Postgres | Required | Not needed |
| RabbitMQ | Required | Not needed |
| Redis | Required | Not needed |
| Qdrant | Required | Required |
| TEI | Required | Required |
| Chrome | Required (for Chrome render mode) | Required (for Chrome render mode) |
| SQLite | Not used | Single file, auto-created |

`docker-compose.services.yaml` in lite mode: 3 services instead of 6.

---

## Migration Path

1. **Implement dual mode** — `AXON_LITE=1` activates lite, full stack remains default
2. **Set `AXON_LITE=1` in `.env.example`** — make lite the recommended default for new users
3. **Run lite in production** — validate feature parity over several weeks of real use
4. **Remove full stack** — delete `crates/jobs/full/`, remove `lapin`/`redis`/`sqlx-postgres`, slim `docker-compose.services.yaml` to 3 services
5. **Remove dual-mode abstraction** — `JobBackend` trait can be collapsed, `LiteBackend` becomes the only backend
