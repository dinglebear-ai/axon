# axon fresh
Last Modified: 2026-06-26

<!-- BEGIN GENERATED ACTION SURFACES -->
## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon fresh ...` |
| REST | Not inventoried |
| MCP | Not exposed as a dedicated MCP action. |
| Service | `Not inventoried` |

Parity notes: This action page is missing from docs/reference/api-parity.md.
<!-- END GENERATED ACTION SURFACES -->


Manage CLI-created freshness schedules for embedding-producing commands.

> Current runtime only. The #298 target replaces this old `--fresh` schedule
> model with source-backed freshness via `axon <source> --watch`,
> `axon watch <source>`, and `axon watch exec <source>` where applicable.

## Surfaces

| Surface | Entry point |
|---|---|
| CLI | `axon fresh ...` |
| REST | Not implemented in v1 |
| MCP | Not implemented in v1 |
| Service | `services::freshness::{list,run_now,history}` |

Freshness management is CLI-only in v1. REST, MCP, web, and palette management
surfaces are deferred follow-up work.

## Synopsis

```bash
axon scrape <url> --fresh 1d
axon crawl <url> --fresh 1d
axon embed <input> --fresh 7d
axon ingest <target> --fresh 7d

axon fresh list [--json]
axon fresh run-now <id> [--json]
axon fresh history <id> [--limit N] [--json]
```

`--fresh` accepts whole-day durations from `1d` through `366d`. Uppercase,
fractional, zero, and sub-day values are rejected.

## Commands

| Command | Description |
|---|---|
| `list` | List active freshness schedules. |
| `run-now <id>` | Lease and run one schedule immediately. |
| `history <id>` | Show recent runs for one schedule. |

## Examples

```bash
# Keep a documentation page fresh daily
axon scrape https://modelcontextprotocol.io/specification --fresh 1d

# Keep a bounded docs crawl fresh daily
axon crawl https://modelcontextprotocol.io/docs/getting-started/intro --fresh 1d

# Keep a GitHub/RSS/reddit/etc. ingest fresh weekly
axon ingest unraid/api --fresh 7d

# Inspect and manually trigger
axon fresh list --json
axon fresh run-now <id> --json
axon fresh history <id> --json
```

## Behavior Notes

- Schedules are stored in the SQLite jobs database.
- `axon serve` and `axon mcp` start the in-process freshness scheduler.
- One-shot CLI creation only writes the schedule unless `--wait true` is also
  supplied, in which case Axon creates the schedule and immediately runs it once.
- Scheduled `crawl`, `embed`, and `ingest` runs enqueue normal jobs and preserve
  existing queue caps and worker behavior.
- Scheduled `scrape` runs inline inside the bounded freshness executor because
  scrape has no dedicated job family in v1.
- Replay payloads are versioned and revalidated before dispatch. Secret-bearing
  headers such as `Authorization`, `Cookie`, and `X-API-Key` are rejected at
  schedule creation and are not persisted in SQLite history.
