# CLI Overview

Last Modified: 2026-07-19

The Axon CLI is a transport over the same `axon-services` layer used by MCP and
REST. Parsing and terminal rendering live in `axon-cli` (and `axon-core`
config modules); pipeline behavior lives behind `axon-services`.

```text
axon ask "..."   ──┐
POST /v1/ask     ──┼──→  axon-api DTO  ──→  axon-services  ──→  domain crates
MCP action=ask   ──┘
```

`axon ask` maps to `POST /v1/ask` and MCP `action=ask`; `axon <source>` maps
to `POST /v1/sources` and MCP `action=source`. One service layer, three
transports.

## Command surface

`axon <source>` is the canonical acquisition and indexing command — scope
selects the strategy (`--scope page|site|docs|repo|package|subreddit`).
`axon scrape <url>` is a retained one-page projection (`scope=page`,
`limits.max_pages=1`). `axon map <source>` discovers without embedding.

Headline command groups:

| Group | Commands |
|---|---|
| Acquisition | `source` (`axon <source>`), `scrape`, `map`, `sessions` |
| Retrieval/synthesis | `query`, `retrieve`, `ask`, `chat`, `summarize`, `evaluate`, `suggest`, `search`, `research` |
| Extraction/inspection | `extract` (+ lifecycle), `brand`, `diff`, `endpoints`, `screenshot` |
| Lifecycle | `jobs list/get/events/stream/cancel/retry/recover/cleanup/clear/worker`, `watch create/.../history` |
| Memory | `memory remember/list/search/show/link/supersede/context` |
| Cleanup | `prune plan/exec`, `reset plan/exec`, `migrate` |
| Discovery | `sources`, `domains`, `stats`, `status`, `monitor jobs` |
| Runtime/setup | `serve`, `serve mcp`, `mcp`, `doctor`, `preflight`, `smoke`, `compose`, `setup`, `config`, `update` |
| Resources | `artifacts`, `uploads`, `collections`, `graph`, `providers`, `capabilities` |

The authoritative, always-current registry is generated:
[`commands.md`](commands.md) (grouped tables) and [`commands.json`](commands.json)
(machine-readable, 110 commands, `contract_version: 2026-06-30`). Regenerated
by `cargo xtask schemas cli`; cross-checked against the live clap tree.
Per-command flags: `axon <cmd> --help`.

## Foreground vs. detached

Commands that mutate durable state can run foreground or detached:

- `--wait true` — enqueue, start in-process workers, poll to terminal state
  (timeout `AXON_JOB_WAIT_TIMEOUT_SECS`, default 300s).
- *no flag* — enqueue, return a job id, exit. A worker process (`axon serve`
  or `axon jobs worker`) must be running for the job to advance.

Detached commands in the registry are marked `~` in `commands.md`; mutating
commands are marked `*`. `AXON_JOBS_AUTO_WORKER=true` (default) spawns a
short-lived worker for detached CLI invocations.

## Output contract

- **stdout** carries JSON data (machine-readable). `--json` makes this explicit
  and uses transport-neutral DTO envelopes.
- **stderr** carries progress and logs (spinners via indicatif, tracing via
  `log_info`/`log_done`). Keep this split intact so server-mode and MCP
  callers can parse stdout cleanly.

`--json` is accepted on all commands that print results.

## Generated references

- [`commands.md`](commands.md) — grouped command tables (human-readable).
- [`commands.json`](commands.json) — machine-readable registry.
- [`axon-help.md`](axon-help.md) — help snapshot.

## Ownership

Parsing and terminal rendering belong to `axon-cli` and `axon-core` config
modules. Pipeline behavior belongs behind `axon-services`. The CLI never
imports a domain crate's internal `::ops::*` modules (enforced by
`cargo xtask check-layering`).

If the CLI surface changes, update `crates/axon-core/src/config/cli.rs` and
re-run `cargo xtask schemas cli` in the same PR.
