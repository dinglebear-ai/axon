# Runtime Jobs

Last Modified: 2026-07-19

One SQLite durable job model owns lifecycle, attempts, stages, events,
heartbeats, artifacts, and provider reservations. Source jobs keep one job id
across resolve, acquire, prepare, embed, publish, graph, and cleanup. There is
no per-source-family job store.

> Contract source:
> [`docs/pipeline-unification/runtime/job-contract.md`](../../pipeline-unification/runtime/job-contract.md).
> Implementation: [`crates/axon-jobs/src/`](../../../crates/axon-jobs/src/)
> (`JobStore`, `ServiceJobRuntime`). Workers run in-process in the same Tokio
> runtime as `axon serve` — no broker, Postgres, Redis, or AMQP.

## Job kinds (canonical)

`source`, `extract`, `watch`, `map`, `research`, `ask`, `query`, `retrieve`,
`memory`, `graph`, `prune`, `provider_probe`, `reset`. Watch executions use
`job_kind=watch` + `job_intent=exec`. Legacy family kinds (`Crawl`/`Embed`/
`Ingest`) are absent.

`job_intent` ∈ `index`/`refresh`/`watch`/`map`/`retrieve`/`answer`/`extract`/`prune`.

## Lifecycle statuses

`queued`, `pending`, `running`, `waiting`, `blocked`, `canceling`,
`completed`, `completed_degraded`, `failed`, `canceled`, `expired`, `skipped`.

State machine transitions are enforced by the store; any unlisted transition
fails without mutating. Notable: `running → {waiting, canceling, completed,
completed_degraded, failed}`; `waiting → {running, canceling, failed, expired}`;
all terminal statuses (`completed`/`completed_degraded`/`failed`/`canceled`/
`expired`/`skipped`) have no outgoing transition. `completed_degraded` = success
with explicit degradation codes / affected stages / missing optional
capabilities.

Retry is **not** a transition — it appends a new `JobAttempt` under the same
`job_id`.

## Entity tree

`Job → JobAttempt → JobStage → JobEvent → JobHeartbeat → JobArtifact →
JobResult → JobStatus`. Every async/detached op returns a `JobDescriptor`
(`kind`, `id`, `status_url`, `events_url`, `stream_url`, `poll_after_ms`).

Attempts/stages/events/heartbeats/artifacts all carry `job_id`, `attempt`,
and where relevant `stage_id`, `batch_id`, `reservation_id`, `checkpoint_id`.

## `--wait true` vs detached

- `axon <source>` without `--wait` enqueues a detached `source` job (trusted
  local CLI auth snapshot) and returns the descriptor. The CLI then probes the
  worker drain lock and auto-spawns a detached `axon jobs worker` when no
  worker is alive (`crates/axon-cli/src/commands/source.rs` →
  `detach::ensure_worker_process`).
- `--wait true` enqueues, starts in-process workers, and polls to terminal
  state (timeout `AXON_JOB_WAIT_TIMEOUT_SECS`, default 300s).

A long-lived `axon serve` (or `jobs worker`) hosts workers for detached jobs.

## Worker spawn

`ServiceJobRuntime::notify_unified()` (`crates/axon-jobs/src/runtime.rs`) wakes
the unified durable-job worker; returns false when the runtime is enqueue-only.
Called from `ServiceContext::notify_unified` and worker_loop / watch /
extract / search_source_index / memory::sync.

Site-scope source jobs get a separate conservative Chrome/CDP concurrency rail.

## Stale recovery

| Env var | TOML key | Default | Purpose |
|---|---|---|---|
| `AXON_JOB_STALE_TIMEOUT_SECS` | `workers.watchdog-stale-timeout-secs` | 300 | seconds a running job may stay idle before stale |
| `AXON_JOB_STALE_CONFIRM_SECS` | `workers.watchdog-confirm-secs` | 60 | seconds stale must stay unchanged before reclaim |
| `AXON_WATCHDOG_SWEEP_SECS` | — | 15 | periodic sweep interval |
| `AXON_WORKER_STARVATION_SECS` | — | 120 | lane starvation safety net (0 disables) |
| `AXON_JOBS_WORKER_IDLE_EXIT_SECS` | `jobs.worker-idle-exit-secs` | 300 | spawned-worker linger/exit |

Cancellation records intent; workers observe at safe interruption points; the
watchdog reclaims stale jobs based on the timeouts above.

## CLI

```bash
axon jobs list                 # all jobs
axon jobs get <id>             # status, stages, counts, errors
axon jobs events <id>          # paged event log
axon jobs stream               # live event stream
axon jobs cancel <id>
axon jobs retry <id>           # appends a new attempt (--from-phase, --idempotency-key)
axon jobs recover              # reclaim stale running jobs (admin)
axon jobs cleanup              # remove old terminal jobs
axon jobs clear                # clear all rows (admin)
axon jobs worker [--idle-exit-secs N]  # standalone worker process
```

## REST + MCP

Generic `/v1/jobs` collection (read/write/admin split is scope-based):
`GET /v1/jobs`, `GET /v1/jobs/{id}`, `GET /v1/jobs/{id}/events`,
`GET /v1/jobs/{id}/stream`, `POST /v1/jobs/{id}/cancel`,
`POST /v1/jobs/{id}/retry`, plus `/cleanup` and `/recover`. MCP:
`action=jobs subaction=get|events|...`.

## Retention

Terminal job rows 30d, detailed events 14d, failed job events 60d, cleanup debt
until completed, config snapshots ≥ as long as terminal jobs.

## Layering

`axon-jobs` is forbidden from depending on `axon-services` or transport crates
(enforced by `cargo xtask check-layering`). DTOs live in `axon-api`.

If the job model changes, update this file and
[`crates/axon-jobs/src/CLAUDE.md`](../../../crates/axon-jobs/src/CLAUDE.md) in
the same PR.
