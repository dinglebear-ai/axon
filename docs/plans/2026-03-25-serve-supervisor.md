# Serve Supervisor

Date: 2026-03-25

## Goal

Promote `axon serve` from a single axum backend into the local dev supervisor for the full Axon stack.

## Approved Design

`axon serve` becomes a long-running supervisor that:

- checks required infrastructure containers before startup
- starts the Rust websocket/download backend
- starts the HTTP MCP server
- starts worker processes when not in lite mode
- starts the shell websocket server
- starts the Next.js dev server
- restarts failed children with bounded exponential backoff
- shuts down the full child tree on Ctrl-C

## Mode Rules

### Full mode

When `AXON_LITE != 1`, `axon serve` requires these containers to be running and healthy:

- `axon-postgres`
- `axon-redis`
- `axon-rabbitmq`
- `axon-qdrant`
- `axon-tei`
- `axon-chrome`

It then starts:

- serve runtime child
- MCP HTTP child
- `crawl worker`
- `embed worker`
- `extract worker`
- `ingest worker`
- `refresh worker`
- `graph worker` when graph config is present
- `apps/web/shell-server.mjs`
- `apps/web` Next.js dev server

### Lite mode

When `AXON_LITE=1`, `axon serve` only requires:

- `axon-qdrant`
- `axon-tei`

It starts:

- serve runtime child
- MCP HTTP child
- shell server
- Next.js dev server

Separate workers are not started in lite mode because the job backend already runs in-process.

## Process Model

The public `serve` command acts as the supervisor. The websocket/download backend moves behind an internal runtime mode so it can be spawned and restarted like the other children.

Each child has:

- a name
- argv
- working directory
- extra env
- restart eligibility

## Failure Model

- Any managed child that exits is restarted.
- Restart delay uses bounded exponential backoff per child.
- Backoff resets after a stable uptime window.
- Supervisor shutdown terminates all children, waits briefly, then force-kills stragglers.

## Guardrails

- `axon serve` does not auto-start Docker containers.
- Preflight failures report missing or unhealthy services clearly.
- Missing `node` or `pnpm` fail fast before partial startup.
- Docs and `Justfile` should point developers to `axon serve` as the primary local entrypoint.

## Tests

- required-service selection in full vs lite mode
- child-spec selection in full vs lite mode
- restart backoff behavior
- graph worker inclusion only when graph is configured
