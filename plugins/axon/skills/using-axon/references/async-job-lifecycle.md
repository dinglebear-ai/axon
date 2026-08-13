# Durable Job Lifecycle

Axon has **one** durable job model. Every async operation — source indexing,
extraction, watch execution — lives in the same SQLite job tables with attempts,
stages, events, heartbeats, artifacts, and provider reservations. There is no
per-family job store and no per-command lifecycle surface.

A source job keeps **one job id** across resolve, acquire, ledger generation,
prepare, embed, publish, graph, and cleanup. There is no child embedding handoff
to chase.

## Starting work

| Surface | Default | Force the other mode |
|---|---|---|
| MCP `action: "source"` | **synchronous** — returns the finished `SourceResult` | `detached: true` → returns `job_id`, `status`, `poll_after_ms` |
| CLI `axon <source>` | **enqueued** — returns a job id and auto-spawns a worker | `--wait true` → blocks until the job completes |

```json
{ "action": "source", "source": "https://docs.example.com", "scope": "site", "detached": true }
{ "action": "extract", "urls": ["https://example.com/pricing"] }
```

`extract` still defaults its `subaction` to `start`, so the bare call above
enqueues. `source` uses `detached` instead of a start subaction.

## Managing jobs

```json
{ "action": "jobs", "subaction": "list",    "limit": 25 }
{ "action": "jobs", "subaction": "get",     "job_id": "<uuid>" }
{ "action": "jobs", "subaction": "status",  "job_id": "<uuid>" }
{ "action": "jobs", "subaction": "events",  "job_id": "<uuid>" }
{ "action": "jobs", "subaction": "stream",  "job_id": "<uuid>", "after_sequence": 0 }
{ "action": "jobs", "subaction": "cancel",  "job_id": "<uuid>", "reason": "…" }
{ "action": "jobs", "subaction": "retry",   "job_id": "<uuid>" }
{ "action": "jobs", "subaction": "recover" }
{ "action": "jobs", "subaction": "cleanup", "older_than": "<timestamp>" }
{ "action": "jobs", "subaction": "clear" }
```

Filters accepted by `list`: `status`, `kind`, `source_id`, `watch_id`, `limit`,
`cursor`. `retry` accepts `retry_mode` and `from_phase` to resume from a
specific pipeline stage rather than restarting the whole job. `clear` removes
terminal rows only — active jobs must be cancelled or recovered first.

CLI mirror: `axon jobs <list|get|events|stream|cancel|retry|recover|cleanup|clear>`,
plus the CLI-only `axon jobs worker` (runs a standalone worker process) and
`axon monitor jobs --jsonl` (line-oriented lifecycle event stream).

Global snapshot: `{ "action": "status" }` / `axon status`.

## Remaining subaction families

Only two actions still carry their own subaction namespace:

- **`extract`** — `start` (default), plus the generated lifecycle conveniences
  projected over the unified job store: `status`, `cancel`, `errors`, `list`,
  `cleanup`, `clear`, `worker`, `recover`.
- **`memory`** — `remember`, `list`, `search`, `show`, `link`, `supersede`,
  `context`, `reinforce`, `contradict`, `pin`, `archive`, `forget`, `review`,
  `compact`, `import`, `export`.

Everything else routes on `action` alone.

## Workers

Detached jobs only advance while a process with workers is running. The CLI
auto-spawns one behind a SQLite drain lock; `axon serve` and HTTP-mode
`axon mcp` host the worker runtime in-process alongside the web/API/MCP
surfaces. If a detached job never leaves `pending`, check for a live worker
before blaming the pipeline.

## Watches

A watch persists a canonical source request plus a schedule. Each due tick
leases the watch, enqueues **one `source` job**, and records the job id in
`axon_source_watch_runs` — the source pipeline owns the actual work.

```bash
axon watch create "https://docs.example.com" --every-seconds 3600
axon watch list
axon watch exec <watch-id>
axon watch history <watch-id>
```
