# monitor
Last Modified: 2026-06-01

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon monitor ...` |
| REST | Not inventoried |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `Not inventoried` |

Parity notes: This action page is missing from docs/reference/api-parity.md.
<!-- END GENERATED ACTION SURFACES -->


Monitor job lifecycle events as a line-oriented stream — useful for shells, dashboards, and CI.

## Synopsis

```bash
axon monitor jobs [OPTIONS]
```

## Subcommands

| Subcommand | Description |
|------------|-------------|
| `jobs` | Emit unified source, extract, prune, and retrieval job **start**, **completion**, **failure**, and **cancel** events. |

## `monitor jobs` flags

| Flag | Default | Description |
|------|---------|-------------|
| `--watch` | `false` | Keep polling and streaming events instead of emitting one batch and exiting. |
| `--jsonl` | `false` | Emit one compact JSON object per event (newline-delimited). |
| `--interval-secs <n>` | *(internal default)* | Poll interval while `--watch` is active. |
| `--state-file <path>` | *(internal default)* | State file used to suppress duplicate events across runs. |

## Usage

```bash
# Emit the current batch of job events once and exit
axon monitor jobs

# Continuously stream job lifecycle events as JSONL
axon monitor jobs --watch --jsonl

# Poll every 2 seconds, deduping via an explicit state file
axon monitor jobs --watch --interval-secs 2 --state-file /tmp/axon-monitor.json
```

## Behavior

- Each event line corresponds to a job-state transition (start, completion, failure, cancel) across canonical job kinds such as source and extract.
- Without `--watch`, it emits a single batch of new events since the last run and exits — ideal for cron or one-shot checks.
- With `--watch`, it polls on `--interval-secs` and streams events until interrupted.
- The state file records last-seen statuses so repeated runs do not re-emit events that were already reported.
- `--jsonl` makes the stream machine-parseable; without it, events render as human-readable lines.

## See also

- [`status`](status.md) — point-in-time snapshot of the job queue.
- [Job lifecycle](../job-lifecycle.md) — job states and the SQLite-backed queue.
