# Job Lifecycle
Last Modified: 2026-07-15

The async-job state machine for Axon. Jobs use SQLite persistence and
in-process Tokio workers; there is no message broker, Postgres, or Redis
runtime.

For the target contract, see
[`../pipeline-unification/runtime/job-contract.md`](../pipeline-unification/runtime/job-contract.md).
For crate/module ownership, see
[`../../crates/axon-jobs/src/CLAUDE.md`](../../crates/axon-jobs/src/CLAUDE.md)
and
[`../../crates/axon-services/src/CLAUDE.md`](../../crates/axon-services/src/CLAUDE.md).

## Job Kinds

Durable jobs live in the `jobs` table. The canonical public/runtime job kinds
are:

- `source`
- `extract`
- `watch`
- `map`
- `research`
- `ask`
- `query`
- `retrieve`
- `memory`
- `graph`
- `prune`
- `provider_probe`
- `reset`

Web source/scrape/map work is represented as `source` with a `SourceRequest`
scope (`page`, `site`, or `docs`). Local embedding and ingest-like source
families also enter through `source` unless they are a dedicated non-source
operation such as `extract`.

## State Model

Lifecycle status values are enforced by the durable jobs schema and mirrored in
`axon_api::source::LifecycleStatus`:

```text
queued -> running -> completed
queued -> running -> completed_degraded
queued -> running -> failed
queued -> canceling -> canceled
queued -> waiting -> queued
running -> waiting -> queued
running -> expired
running -> skipped
```

The durable store also accepts `pending` and `blocked` for compatibility with
internal scheduler states. Transports render these states through typed job DTOs
instead of table-specific rows.

## Enqueue Flow

1. A CLI, REST, MCP, or service caller builds a typed request, such as
   `SourceRequest`, `ExtractRequest`, or a job-control request.
2. `axon-services` creates a durable `JobCreateRequest` with:
   - `kind`
   - `intent`
   - `status`
   - `phase`
   - `request_json`
   - `auth_snapshot_json`
   - config/stage metadata
3. `SqliteUnifiedJobStore` inserts the row into `jobs` and writes supporting
   attempt/stage/event rows as applicable.
4. `ServiceJobRuntime::notify_unified()` wakes in-process workers when present.

Fire-and-forget clients receive a job id. `--wait true` callers enqueue and
poll until the job reaches a terminal state or the configured wait timeout is
hit.

## Worker Flow

Workers are in-process and are spawned by the service runtime:

```text
spawn_workers
├─ unified durable worker
├─ watch scheduler
├─ watchdog / stale recovery
└─ provider reservation maintenance
```

The unified worker claims runnable jobs from `jobs`, respecting priority,
cooldown, deadlines, auth snapshots, and configured concurrency. Site-scope
source jobs get a separate conservative concurrency rail because they share the
Chrome/CDP runtime.

Runner registration lives in `crates/axon-services/src/runtime/job_runners.rs`.
The source runner handles `SourceRequest` jobs; extract, memory, provider probe,
and other operational runners are registered explicitly.

## Progress

Progress is written as durable job counts/current/error/warning data and is
rendered through:

- CLI status and monitor views
- REST `/v1/jobs` routes
- MCP task status/progress helpers
- logs and tracing events

Source jobs keep one job id across acquire, prepare, embed, publish, graph, and
cleanup. There is no child embedding handoff for web source/scrape/map work.

## Cancellation And Recovery

Cancellation uses the durable job API:

- `axon jobs cancel <job_id>`
- REST job cancel routes
- MCP job/task cancel routes

The runtime records cancel intent and in-process workers observe cancellation at
safe interruption points. Recovery and cleanup operate against the same durable
job model:

- `axon jobs recover`
- `axon jobs cleanup`
- `axon jobs clear`

The watchdog reclaims stale running jobs based on configured stale/confirm
thresholds and provider cooldown state.

## Operational Commands

Use the generic jobs surface for lifecycle operations:

```bash
axon jobs list
axon jobs status <job_id>
axon jobs cancel <job_id>
axon jobs recover
axon jobs cleanup
axon jobs clear
```

Source projections still offer command-specific convenience:

```bash
axon https://example.com --scope site --wait true
axon scrape https://example.com --wait true
axon extract https://example.com --wait true
```

## Failure Modes

| Layer | Symptom | Recovery |
|---|---|---|
| Worker panic or process kill | Job remains running until stale threshold | Watchdog/startup recovery reclaims it |
| Provider saturation | Job waits with cooldown metadata | Worker claim skips until cooldown clears |
| Cancellation | Job moves through canceling/canceled | Retry or re-enqueue if needed |
| Queue cleanup | Terminal jobs removed by cleanup policy | Re-run the source or inspect artifacts before cleanup |
| Unknown job id | Status returns not found | Check `axon jobs list` and request logs |

## Source Map

Active code:

- `crates/axon-jobs/src/unified.rs`
- `crates/axon-jobs/src/workers.rs`
- `crates/axon-jobs/src/workers/unified.rs`
- `crates/axon-jobs/src/workers/watchdog.rs`
- `crates/axon-services/src/runtime.rs`
- `crates/axon-services/src/runtime/sqlite.rs`
- `crates/axon-services/src/runtime/job_runners.rs`
- `crates/axon-services/src/jobs.rs`
- `crates/axon-web/src/server/handlers/jobs.rs`
- `crates/axon-mcp/src/server/tasks.rs`
