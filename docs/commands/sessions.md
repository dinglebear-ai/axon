# axon sessions
Last Modified: 2026-03-09

Ingest local AI session history (Claude, Codex, Gemini) into Qdrant.

> For implementation details and troubleshooting see [`docs/ingest/sessions.md`](../ingest/sessions.md).

## Synopsis

```bash
axon sessions [FLAGS]
axon sessions <SUBCOMMAND> [ARGS]
```

## Provider Sources

| Provider | Path |
|----------|------|
| Claude | `~/.claude/projects/` |
| Codex | `~/.codex/sessions/` |
| Gemini | `~/.gemini/history/`, `~/.gemini/tmp/` |

## Flags

All global flags apply. Sessions-specific and key flags:

| Flag | Default | Description |
|------|---------|-------------|
| `--claude` | off | Include Claude sessions. Presence flag — include to enable. |
| `--codex` | off | Include Codex sessions. Presence flag — include to enable. |
| `--gemini` | off | Include Gemini sessions. Presence flag — include to enable. |
| `--project <name>` | — | Case-insensitive substring filter on project name. |
| `--wait <bool>` | `false` | Block until ingestion completes; otherwise enqueue async ingest job. |
| `--collection <name>` | `axon` | Target Qdrant collection. |
| `--json` | `false` | Machine-readable output. |

Provider selection rule:
- If none of `--claude/--codex/--gemini` is set, all providers are ingested.

## Job Subcommands

```bash
axon sessions status <job_id>
axon sessions cancel <job_id>
axon sessions errors <job_id>
axon sessions list
axon sessions cleanup
axon sessions clear
axon sessions recover
axon sessions worker
```

These subcommands operate on the shared ingest queue across source types.

## Examples

```bash
# Async enqueue (default) for all providers
axon sessions

# Sync run for Codex only
axon sessions --codex --wait true

# Claude + Gemini filtered to project name match
axon sessions --claude --gemini --project axon --wait true

# Check job status
axon sessions status 550e8400-e29b-41d4-a716-446655440000

# Enqueue through the canonical server
AXON_SERVER_URL=http://127.0.0.1:8001 axon sessions --codex --json

# Server mode: wait for prepared-session async ingest
AXON_SERVER_URL=http://127.0.0.1:8001 axon sessions --claude --project axon-rust --wait true --json
```

## Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `AXON_SERVER_URL` | — | Routes supported commands, including `sessions`, through a running `axon serve` HTTP endpoint. |
| `AXON_MCP_HTTP_TOKEN` | — | Bearer token sent to authenticated Axon HTTP servers. |
| `AXON_SESSION_INGEST_MAX_BYTES` | `20971520` | Maximum bytes read from one session file before skipping it. |
| `AXON_SESSION_INGEST_MAX_TOTAL_TEXT_BYTES` | `104857600` | Maximum total prepared text accepted in one server-mode sessions request. |
| `AXON_COLLECTION` | `axon` | Default target collection when `--collection` is not supplied. |
| `QDRANT_URL` | `http://127.0.0.1:53333` | Vector store endpoint used by local mode and by the server process. |
| `TEI_URL` | — | Embedding endpoint used by local mode and by the server process. |

## External Dependency Install Instructions

Local `axon sessions` parsing/redaction uses the Axon binary only. Embedding still requires the normal Axon runtime services:

```bash
# Install/build axon from this repository
cargo build --release --bin axon
./target/release/axon --version

# Start required local services
just services-up
./scripts/axon doctor
```

For server mode, start a server built from the same revision before running the client:

```bash
AXON_MCP_HTTP_HOST=127.0.0.1 AXON_MCP_HTTP_PORT=8001 axon serve mcp --transport http
AXON_SERVER_URL=http://127.0.0.1:8001 axon sessions --codex --wait true
```

## Notes

- Local session text is decoded and redacted before embedding.
- Job records for local queued runs are stored in `axon_ingest_jobs` with `source_type='sessions'`.
- In server mode (`AXON_SERVER_URL`), `axon sessions` decodes and redacts local files on the client, uploads a bounded prepared-session payload to `POST /v1/ingest/sessions/prepared`, and the server embeds it asynchronously through the ingest worker queue. `--wait true` polls server job state and does not spawn host-local workers.
- Legacy remote `source_type="sessions"` ingest is rejected so an Axon server cannot scan its own `~/.claude`, `~/.codex`, or `~/.gemini` paths by accident.
